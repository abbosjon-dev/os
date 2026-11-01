//! Jarayonlar va rejalaştiruvçi (scheduler).
//!
//! - Har bir vazifa (task) — yadro oqimi yoki foydalanuvçi jarayoni — öz yadro stekiga ega.
//! - Rejalaştiruvçi "round-robin": taymer har 10 ms da navbatdagi tayyor vazifaga ötadi.
//! - Uzilişlar va tizim çaqiruvlari kadrlari şu vazifaning yadro stekida saqlanadi,
//!   şuning uçun almaştiriş istalgan joyda (masalan, taymer uzilişi içida) xavfsiz.
//! - PID 0 — "yadro" (böş turgan vaqtda işlaydigan vazifa): u hamişa tayyor.

pub mod elf;
pub mod programs;
pub mod syscall;
pub mod uaccess;

use crate::exceptions::TrapFrame;
use crate::memory::mmu::{self, AddressSpace, Kind};
use crate::memory::{PAGE_SIZE, frame};
use crate::sync::{SpinLock, irq_restore, irq_save};
use crate::timer;
use crate::{fs, ipc};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub type Pid = usize;

const MAX_TASKS: usize = 64;
/// Eng köp protsessor yadrolari (QEMU virt: -smp 4).
pub const MAX_CPUS: usize = 4;
const KSTACK_PAGES: usize = 8; // 32 KiB
/// Stek oynasi: 64 KiB (pastki yarmi — qörğon). Qarang: arch/vectors.s
const KSTACK_WINDOW: usize = 64 * 1024;
/// Foydalanuvçi steki tepasining eng yuqori qiymati; haqiqiy tepa tasodifiy (ASLR).
pub const USER_STACK_TOP: usize = mmu::USER_END - PAGE_SIZE;
/// ASLR: stek tepasi 0..16 MiB pastga, heap boşi 0..1 GiB yuqoriga tasodifiy suriladi.
const STACK_RANDOM_PAGES: u64 = 4096;
const HEAP_RANDOM_PAGES: u64 = 262_144;
const USER_STACK_PAGES: usize = 16; // 64 KiB
/// Foydalanuvçi heapʼi şu manzildan boşlab yuqoriga östiriladi (SYS_MAP).
pub const USER_HEAP_BASE: usize = 0x60_0000_0000;
const USER_HEAP_MAX: usize = 256 << 20; // jarayonga 256 MiB gaça
const MAX_HANDLES: usize = 32;
pub const MAX_ARGS: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    Running,
    Sleeping(u64),
    WaitingInput,
    WaitingPid(Pid),
    WaitingChannel(ipc::ChanId),
    /// Umumiy hodisa (mutex, disk va h.k.) — kalit bilan
    WaitingEvent(usize),
    Dead(i64),
}

/// Jarayon oçgan resurs (fayl yoki kanal) — "dastak".
#[derive(Clone, Copy)]
pub enum Handle {
    File { node: fs::NodeId, offset: usize, append: bool },
    Channel(ipc::ChanId),
    /// Konsol: öqiş — klaviaturadan bitta qator, yoziş — ekranga
    Console,
    PipeRead(crate::pipe::PipeId),
    PipeWrite(crate::pipe::PipeId),
}

impl Handle {
    /// Dastak nusxalanganda (bola jarayonga berilganda) quvur hisoblagiçini oşiradi.
    fn retain(self) -> Handle {
        match self {
            Handle::PipeRead(id) => crate::pipe::add_ref(id, false),
            Handle::PipeWrite(id) => crate::pipe::add_ref(id, true),
            _ => {}
        }
        self
    }

    /// Dastak yopildi.
    fn release(self) {
        match self {
            Handle::PipeRead(id) => crate::pipe::release(id, false),
            Handle::PipeWrite(id) => crate::pipe::release(id, true),
            _ => {}
        }
    }
}

/// Dastaklar röyxatini yopadi (rejalaştiruvçi qulfidan taşqarida çaqirilişi kerak).
fn release_handles(handles: Vec<Option<Handle>>) {
    for h in handles.into_iter().flatten() {
        h.release();
    }
}

/// Konsolning oldingi plandagi jarayoni: Ctrl+C uni töxtatadi.
static FOREGROUND: AtomicUsize = AtomicUsize::new(0);

/// Saqlangan registrlar (arch/switch.s bilan bir xil tartibda).
#[repr(C)]
#[derive(Default)]
struct Context {
    x19_x28: [u64; 10],
    x29: u64,
    lr: u64,
    sp: u64,
}

pub struct Task {
    pub pid: Pid,
    pub name: String,
    pub state: State,
    pub user: bool,
    /// Uni işga tuşirgan jarayon
    pub parent: Pid,
    /// Oqim bölsa — u tegişli jarayon (yetakçi) PID i; jarayon yoki yadro oqimi bölsa — 0.
    /// Jarayon resurslari (manzil maydoni, dastaklar, muhit, joriy papka) yetakçida.
    pub leader: Pid,
    /// Sahifa jadvali (oqimlar yetakçinikini işlatadi)
    root: usize,
    /// Heç kim kutmaydi — tugagaç avtomatik tozalanadi.
    detached: bool,
    /// Konteksti hozir biror yadroda "jonli" (işlayapti yoki almaştiriş tugamagan)
    on_cpu: bool,
    /// Oxirgi marta qaysi yadroda işlagan
    pub last_cpu: usize,
    handles: Vec<Option<Handle>>,
    /// Muhit özgaruvçilari (bola jarayonga meros bölib ötadi)
    env: Vec<(String, String)>,
    /// Işonçli (yadro işga tuşirgan terminal): tizim buyruqlariga ruxsat
    trusted: bool,
    /// Töxtatiş sörovi: jarayon xavfsiz nuqtada (foydalanuvçi rejimiga qaytişda) özi tugaydi
    killed: bool,
    /// Joriy papka (nisbiy yöllar şunga nisbatan)
    cwd: String,
    heap_base: usize,
    heap_end: usize,
    ctx: Context,
    kstack: Option<usize>,
    aspace: Option<AddressSpace>,
}

impl Drop for Task {
    fn drop(&mut self) {
        if let Some(slot) = self.kstack {
            free_kstack(slot);
        }
        // aspace özi böşatiladi (AddressSpace::drop)
    }
}

/// Band yadro stek oynalari (bit = oyna raqami).
static KSTACK_SLOTS: SpinLock<u64> = SpinLock::new(0);

fn kstack_window(slot: usize) -> usize {
    mmu::KSTACK_BASE + slot * KSTACK_WINDOW
}

/// Stek tepasi (SP boşlangʼiç qiymati).
fn kstack_top(slot: usize) -> usize {
    kstack_window(slot) + KSTACK_WINDOW
}

/// Yadro steki: böş oyna topiladi, uning yuqori yarmiga sahifalar xaritalanadi.
/// Pastki yarmi xaritalanmaydi — stek tölib ketsa, apparat xatosi yuz beradi.
fn alloc_kstack() -> Result<usize, SpawnError> {
    let slot = {
        let mut used = KSTACK_SLOTS.lock();
        let slot = (!*used).trailing_zeros() as usize;
        if slot >= MAX_TASKS {
            return Err(SpawnError::TooManyTasks);
        }
        *used |= 1 << slot;
        slot
    };
    let base = kstack_top(slot) - KSTACK_PAGES * PAGE_SIZE;
    for i in 0..KSTACK_PAGES {
        let ok = frame::alloc_zeroed()
            .map(|pa| (pa, mmu::map_page(base + i * PAGE_SIZE, pa, Kind::KernelData)))
            .is_some_and(|(pa, r)| r.is_ok() || {
                frame::free(pa);
                false
            });
        if !ok {
            // Qisman xaritalangan sahifalarni qaytaramiz
            for j in 0..i {
                if let Some(pa) = mmu::unmap_page(base + j * PAGE_SIZE) {
                    frame::free(pa);
                }
            }
            *KSTACK_SLOTS.lock() &= !(1 << slot);
            return Err(SpawnError::OutOfMemory);
        }
    }
    Ok(slot)
}

fn free_kstack(slot: usize) {
    let base = kstack_top(slot) - KSTACK_PAGES * PAGE_SIZE;
    for i in 0..KSTACK_PAGES {
        if let Some(pa) = mmu::unmap_page(base + i * PAGE_SIZE) {
            frame::free(pa); // nollanadi
        }
    }
    *KSTACK_SLOTS.lock() &= !(1 << slot);
}

struct Scheduler {
    tasks: [Option<Box<Task>>; MAX_TASKS],
    /// Har bir yadroda hozir işlayotgan vazifa uyaçasi
    current: [usize; MAX_CPUS],
    next_pid: Pid,
}

static SCHED: SpinLock<Scheduler> = SpinLock::new(Scheduler {
    tasks: [const { None }; MAX_TASKS],
    current: [0; MAX_CPUS],
    next_pid: 1,
});
static STARTED: AtomicBool = AtomicBool::new(false);
/// Har bir yadro: oxirgi almaştirişda kimdan ketdik (`finish_switch` uçun).
static PREV: [AtomicUsize; MAX_CPUS] = [const { AtomicUsize::new(NO_TASK) }; MAX_CPUS];
const NO_TASK: usize = usize::MAX;
/// Işlayotgan yadrolar soni.
static CPUS_ONLINE: AtomicUsize = AtomicUsize::new(0);

/// Joriy yadro raqami (MPIDR_EL1.Aff0).
pub fn cpu_id() -> usize {
    let mpidr: u64;
    unsafe { asm!("mrs {}, mpidr_el1", out(reg) mpidr, options(nomem, nostack)) };
    (mpidr & 0xff) as usize % MAX_CPUS
}

pub fn cpus_online() -> usize {
    CPUS_ONLINE.load(Ordering::Acquire)
}

unsafe extern "C" {
    fn switch_context(old: *mut Context, new: *const Context);
    fn kthread_trampoline();
    fn user_trampoline();
}

#[derive(Debug)]
#[allow(dead_code)] // maydonlar `{:?}` orqali körsatiladi
pub enum SpawnError {
    OutOfMemory,
    TooManyTasks,
    ArgsTooLong,
    BadImage(elf::ElfError),
}

fn idle_task(cpu: usize) -> Box<Task> {
    Box::new(Task {
        pid: 0,
        name: alloc::format!("yadro/{}", cpu),
        state: State::Running,
        user: false,
        parent: 0,
        leader: 0,
        root: mmu::kernel_root_pa(),
        detached: false,
        on_cpu: true,
        last_cpu: cpu,
        handles: Vec::new(),
        env: Vec::new(),
        trusted: false,
        killed: false,
        cwd: String::from("/"),
        heap_base: USER_HEAP_BASE,
        heap_end: USER_HEAP_BASE,
        ctx: Context::default(),
        kstack: None,
        aspace: None,
    })
}

/// Birinçi yadro: joriy kod (kernel_main) PID 0 — "yadro/0" vazifasiga aylanadi.
pub fn init() {
    init_cpu(0);
    STARTED.store(true, Ordering::Release);
}

/// Har bir yadro öz böş vazifasini (PID 0, uyaça = yadro raqami) röyxatga oladi.
pub fn init_cpu(cpu: usize) {
    {
        let mut s = SCHED.lock();
        s.tasks[cpu] = Some(idle_task(cpu));
        s.current[cpu] = cpu;
    }
    CPUS_ONLINE.fetch_add(1, Ordering::AcqRel);
}

/// Yangi tayyor vazifa bor — uxlab yotgan boşqa yadrolarni uyğotamiz.
fn kick_others() {
    if cpus_online() > 1 {
        crate::gic::send_sgi_others(crate::irq::SGI_RESCHEDULE);
    }
}

fn insert(mut task: Box<Task>) -> Result<Pid, SpawnError> {
    let pid = {
        let mut s = SCHED.lock();
        let slot = (MAX_CPUS..MAX_TASKS).find(|&i| s.tasks[i].is_none()).ok_or(SpawnError::TooManyTasks)?;
        let pid = s.next_pid;
        s.next_pid += 1;
        task.pid = pid;
        s.tasks[slot] = Some(task);
        pid
    };
    kick_others();
    Ok(pid)
}

/// Yangi yadro oqimini yaratadi: `entry(arg)` alohida stekda işlaydi.
pub fn spawn_kernel(name: &str, entry: extern "C" fn(usize), arg: usize) -> Result<Pid, SpawnError> {
    spawn_kernel_ext(name, entry, arg, true)
}

/// `detached = false` — tugagaç `wait` bilan çiqiş kodini oliş mumkin.
pub fn spawn_kernel_ext(name: &str, entry: extern "C" fn(usize), arg: usize, detached: bool) -> Result<Pid, SpawnError> {
    let kstack = alloc_kstack()?;
    let mut ctx = Context::default();
    ctx.x19_x28[0] = entry as *const () as u64;
    ctx.x19_x28[1] = arg as u64;
    ctx.lr = kthread_trampoline as *const () as u64;
    ctx.sp = kstack_top(kstack) as u64;
    insert(Box::new(Task {
        pid: 0,
        name: String::from(name),
        state: State::Ready,
        user: false,
        parent: 0,
        leader: 0,
        root: mmu::kernel_root_pa(),
        detached,
        on_cpu: false,
        last_cpu: 0,
        handles: Vec::new(),
        env: Vec::new(),
        trusted: false,
        killed: false,
        cwd: String::from("/"),
        heap_base: USER_HEAP_BASE,
        heap_end: USER_HEAP_BASE,
        ctx,
        kstack: Some(kstack),
        aspace: None,
    }))
}

/// ELF faylidan foydalanuvçi jarayonini yaratadi.
/// `args` foydalanuvçi stekining tepasiga yoziladi: dastur uni x0 (manzil), x1 (uzunlik) da oladi.
/// Jarayon qanday muhitda işga tuşadi.
pub struct SpawnOptions {
    pub detached: bool,
    pub parent: Pid,
    /// stdin, stdout, stderr
    pub stdio: [Handle; 3],
    pub env: Vec<(String, String)>,
    pub trusted: bool,
    pub cwd: String,
}

impl SpawnOptions {
    /// Yadrodan işga tuşirilgan dastur: konsol, standart muhit.
    pub fn kernel(detached: bool) -> SpawnOptions {
        SpawnOptions {
            detached,
            parent: 0,
            stdio: [Handle::Console; 3],
            env: default_env(),
            trusted: false,
            cwd: default_cwd(),
        }
    }
}

/// Standart joriy papka: kirgan bölsa — uy papkasi.
pub fn default_cwd() -> String {
    String::from(if crate::accounts::current().is_some() { "~" } else { "/" })
}

/// Standart muhit.
pub fn default_env() -> Vec<(String, String)> {
    let mut env = alloc::vec![(String::from("PATH"), String::from("/dasturlar:~/dasturlar"))];
    if let Some(a) = crate::accounts::current() {
        env.push((String::from("FOYDALANUVCHI"), a.username.clone()));
        env.push((String::from("UY"), String::from("~")));
    }
    env
}

pub fn spawn_user(name: &str, image: &[u8], args: &str, opts: SpawnOptions) -> Result<Pid, SpawnError> {
    let SpawnOptions { detached, parent, stdio, env, trusted, cwd } = opts;
    if args.len() > MAX_ARGS {
        return Err(SpawnError::ArgsTooLong);
    }
    let mut aspace = AddressSpace::new_user().ok_or(SpawnError::OutOfMemory)?;
    let entry = elf::load(&mut aspace, image).map_err(SpawnError::BadImage)?;
    // ASLR: har bir jarayonda stek va heap boşqa manzilda
    let stack_top = USER_STACK_TOP - (crate::random::u64() % STACK_RANDOM_PAGES) as usize * PAGE_SIZE;
    let heap_base = USER_HEAP_BASE + (crate::random::u64() % HEAP_RANDOM_PAGES) as usize * PAGE_SIZE;
    for i in 1..=USER_STACK_PAGES {
        let pa = frame::alloc_zeroed().ok_or(SpawnError::OutOfMemory)?;
        if aspace.map(stack_top - i * PAGE_SIZE, pa, Kind::UserData).is_err() {
            frame::free(pa);
            return Err(SpawnError::OutOfMemory);
        }
    }

    // Argumentlar: stek tepasiga (16 bayt tekislangan), stek ulardan pastda boşlanadi.
    let args_addr = (stack_top - args.len()) & !15;
    for (i, b) in args.bytes().enumerate() {
        let va = args_addr + i;
        let pa = aspace.translate(va).ok_or(SpawnError::OutOfMemory)?;
        unsafe { (pa as *mut u8).write(b) };
    }

    let kstack = match alloc_kstack() {
        Ok(k) => k,
        Err(e) => return Err(e), // aspace özi böşatiladi
    };
    // Yadro stekining tepasida tayyor kadr: `restore_and_eret` uni EL0 ga "qaytaradi".
    let top = kstack_top(kstack);
    let frame_ptr = (top - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;
    unsafe {
        let mut x = [0u64; 31];
        x[0] = args_addr as u64;
        x[1] = args.len() as u64;
        frame_ptr.write(TrapFrame {
            x,
            elr: entry as u64,
            spsr: 0, // EL0t, uzilişlar yoqiq
            sp_el0: args_addr as u64,
        });
    }
    let mut ctx = Context::default();
    ctx.lr = user_trampoline as *const () as u64;
    ctx.sp = frame_ptr as u64;

    let task = Box::new(Task {
        pid: 0,
        name: String::from(name),
        state: State::Ready,
        user: true,
        parent,
        leader: 0,
        root: aspace.root_pa(),
        detached,
        on_cpu: false,
        last_cpu: 0,
        handles: stdio.iter().map(|h| Some(h.retain())).collect(),
        env,
        trusted,
        killed: false,
        cwd,
        heap_base,
        heap_end: heap_base,
        ctx,
        kstack: Some(kstack),
        aspace: Some(aspace),
    });
    insert(task)
}

/// Keyingi tayyor vazifani tanlab, unga ötadi. IRQ öçiq holatda çaqirilişi kerak.
///
/// Köp yadroli xavfsizlik: tanlangan vazifa `on_cpu = true` bölib qoladi;
/// eski vazifaning `on_cpu` belgisi faqat uning konteksti töliq saqlangandan keyin
/// (`finish_switch`) tozalanadi — şungaça uni boşqa yadro olib keta olmaydi.
fn schedule() {
    let cpu = cpu_id();
    let (old, new) = {
        let mut guard = SCHED.lock();
        let s = &mut *guard;
        let now = timer::uptime_ms();
        for t in s.tasks.iter_mut().flatten() {
            if let State::Sleeping(until) = t.state {
                if now >= until {
                    t.state = State::Ready;
                }
            }
        }

        let cur = s.current[cpu];
        if let Some(t) = s.tasks[cur].as_mut() {
            if t.state == State::Running {
                t.state = State::Ready;
            }
        }
        // Round-robin: joriydan keyingi tayyor (va boşqa yadroda işlamayotgan) vazifa;
        // heç kim bölmasa — şu yadroning böş vazifasi.
        let next = (1..=MAX_TASKS)
            .map(|k| (cur + k) % MAX_TASKS)
            .find(|&i| {
                i >= MAX_CPUS
                    && s.tasks[i].as_ref().is_some_and(|t| t.state == State::Ready && (!t.on_cpu || i == cur))
            })
            .unwrap_or(cpu);

        let next_task = s.tasks[next].as_mut().expect("vazifa yöq");
        next_task.state = State::Running;
        next_task.on_cpu = true;
        next_task.last_cpu = cpu;
        if next == cur {
            return;
        }
        s.current[cpu] = next;
        PREV[cpu].store(cur, Ordering::Release);
        let new_root = next_task.root;
        let new: *const Context = &next_task.ctx;
        let old: *mut Context = &mut s.tasks[cur].as_mut().unwrap().ctx;
        mmu::activate(new_root);
        (old, new)
    };
    unsafe { switch_context(old, new) };
    finish_switch();
}

/// Almaştirişdan keyin yangi vazifa içida çaqiriladi: eski vazifa endi
/// boşqa yadroda işga tuşirilişi (yoki tozalanişi) mumkin.
#[unsafe(no_mangle)]
pub extern "C" fn finish_switch() {
    let prev = PREV[cpu_id()].swap(NO_TASK, Ordering::AcqRel);
    if prev != NO_TASK {
        if let Some(t) = SCHED.lock().tasks[prev].as_mut() {
            t.on_cpu = false;
        }
    }
}

/// Taymer yoki boşqa uziliş tugagaç çaqiriladi (IRQ kontekstida).
pub fn preempt() {
    if STARTED.load(Ordering::Acquire) {
        schedule();
    }
}

/// Joriy vazifa ixtiyoriy ravişda navbatni böşatadi.
pub fn yield_now() {
    let d = irq_save();
    schedule();
    irq_restore(d);
}

/// Şart rost bölsa, joriy vazifani `state` holatiga ötkazib uxlatadi.
/// Şart rejalaştiruvçi qulfi ostida tekşiriladi: uyğotuvçi ham şu qulfni oladi,
/// şuning uçun "tekşirdik → uyğotiş keldi → uxladik" poygasi (yöqolgan uyğotiş) bölmaydi.
fn block_current_if(state: State, cond: impl Fn() -> bool) {
    block_inner(state, cond, true)
}

/// `interruptible` — töxtatiş sörovi bölsa uxlamaydi (çaqiruvçi tekşirib, çiqib ketadi).
/// Mutex kutişi uzilmaydi: qulf olinmagunça kutiladi (aks holda holat buziladi).
fn block_inner(state: State, cond: impl Fn() -> bool, interruptible: bool) {
    let d = irq_save();
    {
        let mut s = SCHED.lock();
        let cur = s.current[cpu_id()];
        let t = s.tasks[cur].as_mut().unwrap();
        if t.state == State::Running && !(interruptible && t.killed) && cond() {
            t.state = state;
        }
    }
    schedule();
    irq_restore(d);
}

/// Joriy jarayonga töxtatiş sörovi kelganmi.
pub fn current_killed() -> bool {
    let s = SCHED.lock();
    s.tasks[s.current[cpu_id()]].as_ref().is_some_and(|t| t.killed)
}

/// Xavfsiz nuqta (foydalanuvçi rejimiga qaytişdan oldin): töxtatiş sörovi bölsa — tugaymiz.
pub fn check_killed() {
    if current_killed() {
        exit_current(-9);
    }
}

pub fn sleep_ms(ms: u64) {
    block_current_if(State::Sleeping(timer::uptime_ms() + ms), || true);
}

/// Klaviaturadan maʼlumot kelgunça kutadi.
pub fn wait_input(has_input: impl Fn() -> bool) {
    block_current_if(State::WaitingInput, || !has_input());
}

fn wake_where(pred: impl Fn(State) -> bool) {
    if !STARTED.load(Ordering::Acquire) {
        return;
    }
    let mut woke = false;
    {
        let mut s = SCHED.lock();
        for t in s.tasks.iter_mut().flatten() {
            if pred(t.state) {
                t.state = State::Ready;
                woke = true;
            }
        }
    }
    if woke {
        kick_others();
    }
}

/// Klaviatura uzilişidan çaqiriladi.
pub fn wake_input() {
    wake_where(|st| st == State::WaitingInput);
}

/// Joriy vazifani tugatadi (foydalanuvçi jarayoni bölsa — butun jarayonni). Qaytmaydi.
pub fn exit_current(code: i64) -> ! {
    exit_inner(code, false)
}

/// Faqat joriy oqimni tugatadi (jarayon davom etadi).
pub fn exit_thread(code: i64) -> ! {
    exit_inner(code, true)
}

fn exit_inner(code: i64, thread_only: bool) -> ! {
    irq_save();
    let handles;
    {
        let mut s = SCHED.lock();
        let cur = s.current[cpu_id()];
        let pid = s.tasks[cur].as_ref().unwrap().pid;
        let me = s.tasks[cur].as_mut().unwrap();
        me.state = State::Dead(code);
        handles = core::mem::take(&mut me.handles); // oqimda böş: dastaklar yetakçida
        let (leader, is_thread_exit) = (if me.leader != 0 { me.leader } else { me.pid }, thread_only);
        if me.user && !is_thread_exit {
            // Jarayon tugaydi: uning barça oqimlari (va yetakçisi) ham xavfsiz nuqtada töxtaydi
            for t in s.tasks.iter_mut().flatten() {
                if (t.leader == leader || t.pid == leader) && !matches!(t.state, State::Dead(_)) {
                    t.killed = true;
                    if !matches!(t.state, State::Running | State::Ready | State::WaitingEvent(_)) {
                        t.state = State::Ready;
                    }
                }
            }
        }
        // Yetakçi tugasa — oqimlarini endi heç kim kutmaydi
        if s.tasks[cur].as_ref().unwrap().leader == 0 {
            for t in s.tasks.iter_mut().flatten() {
                if t.leader == pid {
                    t.detached = true;
                }
            }
        }
        for t in s.tasks.iter_mut().flatten() {
            if t.state == State::WaitingPid(pid) {
                t.state = State::Ready;
            }
            // Yetim qolgan bolalar endi heç kim kutmaydi — tugagaç özi tozalanadi.
            if t.parent == pid {
                t.detached = true;
            }
        }
    }
    release_handles(handles); // quvurlarning narigi tomoni EOF ni körsin
    kick_others();
    schedule();
    unreachable!("tugagan vazifa qayta işga tuşdi");
}

#[unsafe(no_mangle)]
extern "C" fn kthread_exit() -> ! {
    exit_current(0)
}

/// `pid` tugagunça kutadi, çiqiş kodini qaytaradi va uni tozalaydi.
pub fn wait(pid: Pid) -> Option<i64> {
    enum Step {
        Done(Option<i64>),
        Sleep,
        Retry,
    }
    loop {
        let d = irq_save();
        let (step, task) = {
            let mut s = SCHED.lock();
            let cur = s.current[cpu_id()];
            match s.tasks.iter().position(|t| t.as_ref().is_some_and(|t| t.pid == pid && t.pid != 0)) {
                None => (Step::Done(None), None),
                Some(i) => {
                    let t = s.tasks[i].as_ref().unwrap();
                    let threads_left = has_threads(&s, pid);
                    match (t.state, t.on_cpu) {
                        // Tugagan, konteksti saqlangan va oqimlari qolmagan — tozalaş mumkin
                        (State::Dead(code), false) if !threads_left => (Step::Done(Some(code)), s.tasks[i].take()),
                        // Tugagan, lekin hali boşqa yadrodan çiqib ulgurmagan
                        (State::Dead(_), true) => (Step::Retry, None),
                        _ => {
                            s.tasks[cur].as_mut().unwrap().state = State::WaitingPid(pid);
                            (Step::Sleep, None)
                        }
                    }
                }
            }
        };
        drop(task); // xotira qaytariladi (qulfsiz)
        match step {
            Step::Done(r) => {
                irq_restore(d);
                return r;
            }
            Step::Sleep | Step::Retry => {
                schedule();
                irq_restore(d);
                if current_killed() {
                    return None; // kutayotgan özi töxtatildi
                }
            }
        }
    }
}

/// Jarayonning hali tozalanmagan oqimlari bormi (bölsa, yetakçi resurslari böşatilmaydi).
fn has_threads(s: &Scheduler, pid: Pid) -> bool {
    pid != 0 && s.tasks.iter().flatten().any(|t| t.leader == pid)
}

/// Joriy jarayonda yangi oqim: `entry(arg)` foydalanuvçi rejimida, `stack_top` stekda.
pub fn spawn_thread(entry: usize, stack_top: usize, arg: u64) -> Result<Pid, SpawnError> {
    let (leader, root, cwd) = {
        let s = SCHED.lock();
        let cur = s.current[cpu_id()];
        let p = proc_slot(&s, cur);
        let t = s.tasks[p].as_ref().unwrap();
        if !t.user {
            return Err(SpawnError::BadImage(elf::ElfError::NotElf));
        }
        (t.pid, t.root, t.cwd.clone())
    };
    let kstack = alloc_kstack()?;
    let top = kstack_top(kstack);
    let frame_ptr = (top - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;
    unsafe {
        let mut x = [0u64; 31];
        x[0] = arg;
        frame_ptr.write(TrapFrame { x, elr: entry as u64, spsr: 0, sp_el0: (stack_top & !15) as u64 });
    }
    let mut ctx = Context::default();
    ctx.lr = user_trampoline as *const () as u64;
    ctx.sp = frame_ptr as u64;
    insert(Box::new(Task {
        pid: 0,
        name: String::from("oqim"),
        state: State::Ready,
        user: true,
        parent: leader,
        leader,
        root,
        detached: false,
        on_cpu: false,
        last_cpu: 0,
        handles: Vec::new(),
        env: Vec::new(),
        trusted: false,
        killed: false,
        cwd,
        heap_base: 0,
        heap_end: 0,
        ctx,
        kstack: Some(kstack),
        aspace: None,
    }))
}

/// Oqim uçun hodisa kaliti (futex): manzil maydoni + manzil.
pub fn futex_key(addr: usize) -> usize {
    let s = SCHED.lock();
    let root = s.tasks[s.current[cpu_id()]].as_ref().map_or(0, |t| t.root);
    0x4000_0000_0000_0000 | (root << 20) ^ addr
}

/// Kutmasdan tekşiradi: `Some(Some(kod))` — tugagan (va tozalandi), `Some(None)` — işlayapti,
/// `None` — bunday jarayon yöq.
pub fn try_wait(pid: Pid) -> Option<Option<i64>> {
    let task = {
        let mut s = SCHED.lock();
        let i = s.tasks.iter().position(|t| t.as_ref().is_some_and(|t| t.pid == pid && t.pid != 0))?;
        let t = s.tasks[i].as_ref().unwrap();
        match (t.state, t.on_cpu) {
            (State::Dead(_), false) if !has_threads(&s, pid) => s.tasks[i].take(),
            _ => return Some(None),
        }
    };
    let code = match task.as_ref().map(|t| t.state) {
        Some(State::Dead(c)) => c,
        _ => 0,
    };
    drop(task);
    Some(Some(code))
}

/// Heç kim kutmayotgan tugagan vazifalarni tozalaydi (böş vazifalardan çaqiriladi).
pub fn reap_detached() {
    loop {
        let task = {
            let mut s = SCHED.lock();
            let slot = (MAX_CPUS..MAX_TASKS).find(|&i| {
                s.tasks[i].as_ref().is_some_and(|t| {
                    t.detached && !t.on_cpu && matches!(t.state, State::Dead(_)) && !has_threads(&s, t.pid)
                })
            });
            match slot {
                Some(i) => s.tasks[i].take(),
                None => return,
            }
        };
        drop(task);
    }
}

/// Foydalanuvçi jarayonini tugatadi. Yadro oqimlarini tugatib bölmaydi.
/// Boşqa yadroda işlayotgan jarayon keyingi almaştirişda töxtaydi.
/// Foydalanuvçi jarayonini töxtatiş sörovi. Jarayon darhol emas, xavfsiz nuqtada —
/// foydalanuvçi rejimiga qaytayotganda — özi tugaydi: yadro içida (masalan, Mutex uşlab
/// turganda) uzib qöyilmaydi. Kutayotgan bölsa — uyğotiladi.
pub fn kill(pid: Pid) -> bool {
    {
        let mut s = SCHED.lock();
        let Some(i) = s.tasks.iter().position(|t| t.as_ref().is_some_and(|t| t.pid == pid)) else {
            return false;
        };
        let t = s.tasks[i].as_ref().unwrap();
        if !t.user || matches!(t.state, State::Dead(_)) {
            return false;
        }
        let leader = if t.leader != 0 { t.leader } else { t.pid };
        for t in s.tasks.iter_mut().flatten() {
            if (t.pid == leader || t.leader == leader) && !matches!(t.state, State::Dead(_)) {
                t.killed = true;
                if !matches!(t.state, State::Running | State::Ready | State::WaitingEvent(_)) {
                    t.state = State::Ready; // uxlayotgan / kutayotgan — uyğotamiz
                }
            }
        }
    }
    kick_others();
    true
}

/// Ctrl+C: oldingi plandagi foydalanuvçi jarayonini töxtatadi.
pub fn interrupt_foreground() {
    let pid = FOREGROUND.load(Ordering::Acquire);
    if pid != 0 {
        kill(pid);
    }
}

pub fn set_foreground(pid: Pid) {
    FOREGROUND.store(pid, Ordering::Release);
}

/// Joriy jarayon raqami (oqimdan çaqirilsa — jarayonniki).
pub fn current_pid() -> Pid {
    let s = SCHED.lock();
    let cur = s.current[cpu_id()];
    s.tasks[cur].as_ref().map_or(0, |t| if t.leader != 0 { t.leader } else { t.pid })
}

/// Joriy oqim raqami.
pub fn current_tid() -> Pid {
    let s = SCHED.lock();
    s.tasks[s.current[cpu_id()]].as_ref().map_or(0, |t| t.pid)
}

/// Joriy jarayonning manzil maydonida `[addr, addr+len)` foydalanuvçi xotirasimi
/// (`write` bölsa — yoziş ham mumkinmi)?
pub fn user_range_ok(addr: usize, len: usize, write: bool) -> bool {
    let Some(end) = addr.checked_add(len) else { return false };
    if addr < mmu::USER_BASE || end > mmu::USER_END {
        return false;
    }
    let s = SCHED.lock();
    let p = proc_slot(&s, s.current[cpu_id()]);
    let Some(aspace) = s.tasks[p].as_ref().and_then(|t| t.aspace.as_ref()) else {
        return false;
    };
    let mut page = addr & !(PAGE_SIZE - 1);
    while page < end {
        if !aspace.user_accessible(page, write) {
            return false;
        }
        page += PAGE_SIZE;
    }
    true
}

/// Uyaça raqami: oqim bölsa — uning jarayon yetakçisi.
fn proc_slot(s: &Scheduler, slot: usize) -> usize {
    match s.tasks[slot].as_ref().map(|t| t.leader) {
        Some(leader) if leader != 0 => s
            .tasks
            .iter()
            .position(|t| t.as_ref().is_some_and(|t| t.pid == leader && t.leader == 0))
            .unwrap_or(slot),
        _ => slot,
    }
}

/// Joriy jarayon (oqimdan çaqirilsa ham — yetakçi) resurslari bilan işlaş.
fn with_proc<R>(f: impl FnOnce(&mut Task) -> R) -> R {
    let mut s = SCHED.lock();
    let cur = s.current[cpu_id()];
    let p = proc_slot(&s, cur);
    f(s.tasks[p].as_mut().expect("joriy jarayon yöq"))
}

/// Joriy jarayonga yangi dastak qöşadi. Natija: dastak raqami.
pub fn add_handle(h: Handle) -> Option<usize> {
    with_proc(|t| {
        if let Some(i) = t.handles.iter().position(|x| x.is_none()) {
            t.handles[i] = Some(h);
            Some(i)
        } else if t.handles.len() < MAX_HANDLES {
            t.handles.push(Some(h));
            Some(t.handles.len() - 1)
        } else {
            None
        }
    })
}

pub fn get_handle(i: usize) -> Option<Handle> {
    with_proc(|t| t.handles.get(i).copied().flatten())
}

pub fn set_handle(i: usize, h: Handle) {
    with_proc(|t| {
        if let Some(slot) = t.handles.get_mut(i) {
            *slot = Some(h);
        }
    })
}

pub fn close_handle(i: usize) -> bool {
    match with_proc(|t| t.handles.get_mut(i).and_then(|h| h.take())) {
        Some(h) => {
            h.release();
            true
        }
        None => false,
    }
}

/// Muhit özgaruvçisi.
pub fn get_env(name: &str) -> Option<String> {
    with_proc(|t| t.env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
}

pub fn set_env(name: &str, value: &str) {
    with_proc(|t| {
        t.env.retain(|(k, _)| k != name);
        if !value.is_empty() {
            t.env.push((String::from(name), String::from(value)));
        }
    })
}

pub fn current_env() -> Vec<(String, String)> {
    with_proc(|t| t.env.clone())
}

pub fn is_trusted() -> bool {
    with_proc(|t| t.trusted)
}

pub fn cwd() -> String {
    with_proc(|t| t.cwd.clone())
}

pub fn set_cwd(path: &str) {
    with_proc(|t| t.cwd = String::from(path))
}

/// Joriy jarayonga `size` bayt yangi (nollangan) xotira beradi. Natija: manzil.
pub fn map_memory(size: usize) -> Option<usize> {
    let pages = size.div_ceil(PAGE_SIZE);
    with_proc(|t| {
        let start = t.heap_end;
        if pages == 0 || start + pages * PAGE_SIZE > t.heap_base + USER_HEAP_MAX {
            return None;
        }
        let aspace = t.aspace.as_mut()?;
        for i in 0..pages {
            let pa = frame::alloc_zeroed()?;
            if aspace.map(start + i * PAGE_SIZE, pa, Kind::UserData).is_err() {
                frame::free(pa);
                return None;
            }
            // Xaritalangan sahifalar jarayon bilan birga böşatiladi.
            t.heap_end = start + (i + 1) * PAGE_SIZE;
        }
        Some(start)
    })
}

pub fn parent_of(pid: Pid) -> Option<Pid> {
    let s = SCHED.lock();
    s.tasks.iter().flatten().find(|t| t.pid == pid).map(|t| t.parent)
}

/// Kanal xabarini kutadi (`ready()` rost bölsa — kutmaydi).
pub fn wait_channel(id: ipc::ChanId, ready: impl Fn() -> bool) {
    block_current_if(State::WaitingChannel(id), || !ready());
}

/// `still_waiting()` rost bölsa, `key` hodisasi kelgunça uxlaydi.
/// Rejalaştiruvçi hali işga tuşmagan bölsa (yuklaniş), şunçaki kutib turadi.
pub fn wait_event(key: usize, still_waiting: impl Fn() -> bool) {
    if !STARTED.load(Ordering::Acquire) {
        while still_waiting() {
            core::hint::spin_loop();
        }
        return;
    }
    block_current_if(State::WaitingEvent(key), still_waiting);
}

/// Uzilmaydigan kutiş (Mutex uçun): töxtatiş sörovi bölsa ham kutiladi.
pub fn wait_event_uninterruptible(key: usize, still_waiting: impl Fn() -> bool) {
    if !STARTED.load(Ordering::Acquire) {
        while still_waiting() {
            core::hint::spin_loop();
        }
        return;
    }
    block_inner(State::WaitingEvent(key), still_waiting, false);
}

/// `key` hodisasini kutayotgan barça vazifalarni uyğotadi (uzilişdan ham çaqirsa böladi).
pub fn wake_event(key: usize) {
    wake_where(|st| st == State::WaitingEvent(key));
}

/// Kanalga xabar kelganda çaqiriladi.
pub fn wake_channel(id: ipc::ChanId) {
    wake_where(|st| st == State::WaitingChannel(id));
}

/// `jarayonlar` buyruği uçun maʼlumot.
pub struct TaskInfo {
    pub pid: Pid,
    pub name: String,
    pub state: State,
    pub user: bool,
    pub cpu: usize,
}

/// Barça vazifalar, PID böyiça tartiblangan.
pub fn snapshot() -> alloc::vec::Vec<TaskInfo> {
    let mut v: alloc::vec::Vec<TaskInfo> = {
        let s = SCHED.lock();
        s.tasks
            .iter()
            .flatten()
            .map(|t| TaskInfo { pid: t.pid, name: t.name.clone(), state: t.state, user: t.user, cpu: t.last_cpu })
            .collect()
    };
    v.sort_by_key(|t| t.pid);
    v
}
