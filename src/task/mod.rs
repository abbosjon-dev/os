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

use crate::exceptions::TrapFrame;
use crate::memory::mmu::{self, AddressSpace, Kind};
use crate::memory::{PAGE_SIZE, frame};
use crate::sync::{SpinLock, irq_restore, irq_save};
use crate::timer;
use crate::{fs, ipc};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

pub type Pid = usize;

const MAX_TASKS: usize = 64;
const KSTACK_PAGES: usize = 8; // 32 KiB
pub const USER_STACK_TOP: usize = mmu::USER_END - PAGE_SIZE; // ostida böş "qörğon" sahifa
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
    Dead(i64),
}

/// Jarayon oçgan resurs (fayl yoki kanal) — "dastak".
#[derive(Clone, Copy)]
pub enum Handle {
    File { node: fs::NodeId, offset: usize, append: bool },
    Channel(ipc::ChanId),
}

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
    /// Heç kim kutmaydi — tugagaç avtomatik tozalanadi.
    detached: bool,
    handles: Vec<Option<Handle>>,
    heap_end: usize,
    ctx: Context,
    kstack: Option<usize>,
    aspace: Option<AddressSpace>,
}

impl Drop for Task {
    fn drop(&mut self) {
        if let Some(base) = self.kstack {
            for i in 0..KSTACK_PAGES {
                frame::free(base + i * PAGE_SIZE);
            }
        }
        // aspace özi böşatiladi (AddressSpace::drop)
    }
}

struct Scheduler {
    tasks: [Option<Box<Task>>; MAX_TASKS],
    current: usize,
    next_pid: Pid,
}

static SCHED: SpinLock<Scheduler> =
    SpinLock::new(Scheduler { tasks: [const { None }; MAX_TASKS], current: 0, next_pid: 1 });
static STARTED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn switch_context(old: *mut Context, new: *const Context);
    fn kthread_trampoline();
    fn restore_and_eret();
}

#[derive(Debug)]
#[allow(dead_code)] // maydonlar `{:?}` orqali körsatiladi
pub enum SpawnError {
    OutOfMemory,
    TooManyTasks,
    ArgsTooLong,
    BadImage(elf::ElfError),
}

/// Joriy kod (kernel_main) PID 0 — "yadro" vazifasiga aylanadi.
pub fn init() {
    let idle = Box::new(Task {
        pid: 0,
        name: String::from("yadro"),
        state: State::Running,
        user: false,
        parent: 0,
        detached: false,
        handles: Vec::new(),
        heap_end: USER_HEAP_BASE,
        ctx: Context::default(),
        kstack: None,
        aspace: None,
    });
    SCHED.lock().tasks[0] = Some(idle);
    STARTED.store(true, Ordering::Release);
}

fn alloc_kstack() -> Result<usize, SpawnError> {
    frame::alloc_contiguous(KSTACK_PAGES).ok_or(SpawnError::OutOfMemory)
}

fn insert(mut task: Box<Task>) -> Result<Pid, SpawnError> {
    let mut s = SCHED.lock();
    let slot = (1..MAX_TASKS).find(|&i| s.tasks[i].is_none()).ok_or(SpawnError::TooManyTasks)?;
    let pid = s.next_pid;
    s.next_pid += 1;
    task.pid = pid;
    s.tasks[slot] = Some(task);
    Ok(pid)
}

/// Yangi yadro oqimini yaratadi: `entry(arg)` alohida stekda işlaydi.
pub fn spawn_kernel(name: &str, entry: extern "C" fn(usize), arg: usize) -> Result<Pid, SpawnError> {
    let kstack = alloc_kstack()?;
    let mut ctx = Context::default();
    ctx.x19_x28[0] = entry as *const () as u64;
    ctx.x19_x28[1] = arg as u64;
    ctx.lr = kthread_trampoline as *const () as u64;
    ctx.sp = (kstack + KSTACK_PAGES * PAGE_SIZE) as u64;
    insert(Box::new(Task {
        pid: 0,
        name: String::from(name),
        state: State::Ready,
        user: false,
        parent: 0,
        detached: true,
        handles: Vec::new(),
        heap_end: USER_HEAP_BASE,
        ctx,
        kstack: Some(kstack),
        aspace: None,
    }))
}

/// ELF faylidan foydalanuvçi jarayonini yaratadi.
/// `args` foydalanuvçi stekining tepasiga yoziladi: dastur uni x0 (manzil), x1 (uzunlik) da oladi.
pub fn spawn_user(name: &str, image: &[u8], args: &str, detached: bool, parent: Pid) -> Result<Pid, SpawnError> {
    if args.len() > MAX_ARGS {
        return Err(SpawnError::ArgsTooLong);
    }
    let mut aspace = AddressSpace::new_user().ok_or(SpawnError::OutOfMemory)?;
    let entry = elf::load(&mut aspace, image).map_err(SpawnError::BadImage)?;
    for i in 1..=USER_STACK_PAGES {
        let pa = frame::alloc_zeroed().ok_or(SpawnError::OutOfMemory)?;
        if aspace.map(USER_STACK_TOP - i * PAGE_SIZE, pa, Kind::UserData).is_err() {
            frame::free(pa);
            return Err(SpawnError::OutOfMemory);
        }
    }

    // Argumentlar: stek tepasiga (16 bayt tekislangan), stek ulardan pastda boşlanadi.
    let args_addr = (USER_STACK_TOP - args.len()) & !15;
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
    let top = kstack + KSTACK_PAGES * PAGE_SIZE;
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
    ctx.lr = restore_and_eret as *const () as u64;
    ctx.sp = frame_ptr as u64;

    let task = Box::new(Task {
        pid: 0,
        name: String::from(name),
        state: State::Ready,
        user: true,
        parent,
        detached,
        handles: Vec::new(),
        heap_end: USER_HEAP_BASE,
        ctx,
        kstack: Some(kstack),
        aspace: Some(aspace),
    });
    insert(task)
}

/// Keyingi tayyor vazifani tanlab, unga ötadi. IRQ öçiq holatda çaqirilişi kerak.
fn schedule() {
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

        let cur = s.current;
        if let Some(t) = s.tasks[cur].as_mut() {
            if t.state == State::Running {
                t.state = State::Ready;
            }
        }
        // Round-robin: joriydan keyingi tayyor vazifa; heç kim bölmasa — PID 0.
        let next = (1..=MAX_TASKS)
            .map(|k| (cur + k) % MAX_TASKS)
            .find(|&i| i != 0 && matches!(s.tasks[i].as_ref().map(|t| t.state), Some(State::Ready)))
            .unwrap_or(0);

        let next_task = s.tasks[next].as_mut().expect("vazifa yöq");
        next_task.state = State::Running;
        if next == cur {
            return;
        }
        s.current = next;
        let new_root = next_task.aspace.as_ref().map_or(mmu::kernel_root_pa(), |a| a.root_pa());
        let new: *const Context = &next_task.ctx;
        let old: *mut Context = &mut s.tasks[cur].as_mut().unwrap().ctx;
        mmu::activate(new_root);
        (old, new)
    };
    unsafe { switch_context(old, new) };
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

fn block_current(state: State) {
    let d = irq_save();
    {
        let mut s = SCHED.lock();
        let cur = s.current;
        s.tasks[cur].as_mut().unwrap().state = state;
    }
    schedule();
    irq_restore(d);
}

pub fn sleep_ms(ms: u64) {
    block_current(State::Sleeping(timer::uptime_ms() + ms));
}

/// Klaviaturadan maʼlumot kelgunça kutadi.
pub fn wait_input(has_input: impl Fn() -> bool) {
    let d = irq_save();
    if !has_input() {
        block_current(State::WaitingInput);
    }
    irq_restore(d);
}

/// Klaviatura uzilişidan çaqiriladi.
pub fn wake_input() {
    if !STARTED.load(Ordering::Acquire) {
        return;
    }
    let mut s = SCHED.lock();
    for t in s.tasks.iter_mut().flatten() {
        if t.state == State::WaitingInput {
            t.state = State::Ready;
        }
    }
}

/// Joriy vazifani tugatadi. Qaytmaydi.
pub fn exit_current(code: i64) -> ! {
    irq_save();
    {
        let mut s = SCHED.lock();
        let cur = s.current;
        let pid = s.tasks[cur].as_ref().unwrap().pid;
        let me = s.tasks[cur].as_mut().unwrap();
        me.state = State::Dead(code);
        me.handles.clear();
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
    schedule();
    unreachable!("tugagan vazifa qayta işga tuşdi");
}

#[unsafe(no_mangle)]
extern "C" fn kthread_exit() -> ! {
    exit_current(0)
}

/// `pid` tugagunça kutadi, çiqiş kodini qaytaradi va uni tozalaydi.
pub fn wait(pid: Pid) -> Option<i64> {
    loop {
        let d = irq_save();
        let found = {
            let mut s = SCHED.lock();
            let slot = s.tasks.iter().position(|t| t.as_ref().is_some_and(|t| t.pid == pid));
            match slot {
                None => Some(None),
                Some(i) => match s.tasks[i].as_ref().unwrap().state {
                    State::Dead(code) => {
                        let task = s.tasks[i].take();
                        drop(s);
                        drop(task); // xotira qaytariladi
                        Some(Some(code))
                    }
                    _ => {
                        let cur = s.current;
                        s.tasks[cur].as_mut().unwrap().state = State::WaitingPid(pid);
                        None
                    }
                },
            }
        };
        match found {
            Some(r) => {
                irq_restore(d);
                return r;
            }
            None => {
                schedule();
                irq_restore(d);
            }
        }
    }
}

/// Heç kim kutmayotgan tugagan vazifalarni tozalaydi (PID 0 dan çaqiriladi).
pub fn reap_detached() {
    loop {
        let task = {
            let mut s = SCHED.lock();
            let cur = s.current;
            let slot = (1..MAX_TASKS).find(|&i| {
                i != cur
                    && s.tasks[i].as_ref().is_some_and(|t| t.detached && matches!(t.state, State::Dead(_)))
            });
            match slot {
                Some(i) => s.tasks[i].take(),
                None => return,
            }
        };
        drop(task);
    }
}

/// Foydalanuvçi jarayonini tugatadi (joriy bölmasa). Yadro oqimlarini tugatib bölmaydi.
pub fn kill(pid: Pid) -> bool {
    let mut s = SCHED.lock();
    let cur = s.current;
    let Some(i) = s.tasks.iter().position(|t| t.as_ref().is_some_and(|t| t.pid == pid)) else {
        return false;
    };
    let t = s.tasks[i].as_mut().unwrap();
    if !t.user || i == cur || matches!(t.state, State::Dead(_)) {
        return false;
    }
    t.state = State::Dead(-9);
    for t in s.tasks.iter_mut().flatten() {
        if t.state == State::WaitingPid(pid) {
            t.state = State::Ready;
        }
        if t.parent == pid {
            t.detached = true;
        }
    }
    true
}

pub fn current_pid() -> Pid {
    let s = SCHED.lock();
    s.tasks[s.current].as_ref().map_or(0, |t| t.pid)
}

/// Joriy jarayonning manzil maydonida `[addr, addr+len)` foydalanuvçi xotirasimi
/// (`write` bölsa — yoziş ham mumkinmi)?
pub fn user_range_ok(addr: usize, len: usize, write: bool) -> bool {
    let Some(end) = addr.checked_add(len) else { return false };
    if addr < mmu::USER_BASE || end > mmu::USER_END {
        return false;
    }
    let s = SCHED.lock();
    let Some(aspace) = s.tasks[s.current].as_ref().and_then(|t| t.aspace.as_ref()) else {
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

fn with_current<R>(f: impl FnOnce(&mut Task) -> R) -> R {
    let mut s = SCHED.lock();
    let cur = s.current;
    f(s.tasks[cur].as_mut().expect("joriy vazifa yöq"))
}

/// Joriy jarayonga yangi dastak qöşadi. Natija: dastak raqami.
pub fn add_handle(h: Handle) -> Option<usize> {
    with_current(|t| {
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
    with_current(|t| t.handles.get(i).copied().flatten())
}

pub fn set_handle(i: usize, h: Handle) {
    with_current(|t| {
        if let Some(slot) = t.handles.get_mut(i) {
            *slot = Some(h);
        }
    })
}

pub fn close_handle(i: usize) -> bool {
    with_current(|t| t.handles.get_mut(i).and_then(|h| h.take()).is_some())
}

/// Joriy jarayonga `size` bayt yangi (nollangan) xotira beradi. Natija: manzil.
pub fn map_memory(size: usize) -> Option<usize> {
    let pages = size.div_ceil(PAGE_SIZE);
    with_current(|t| {
        let start = t.heap_end;
        if pages == 0 || start + pages * PAGE_SIZE > USER_HEAP_BASE + USER_HEAP_MAX {
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
    let d = irq_save();
    if !ready() {
        block_current(State::WaitingChannel(id));
    }
    irq_restore(d);
}

/// Kanalga xabar kelganda çaqiriladi.
pub fn wake_channel(id: ipc::ChanId) {
    let mut s = SCHED.lock();
    for t in s.tasks.iter_mut().flatten() {
        if t.state == State::WaitingChannel(id) {
            t.state = State::Ready;
        }
    }
}

/// `jarayonlar` buyruği uçun maʼlumot.
pub struct TaskInfo {
    pub pid: Pid,
    pub name: String,
    pub state: State,
    pub user: bool,
}

/// Barça vazifalar, PID böyiça tartiblangan.
pub fn snapshot() -> alloc::vec::Vec<TaskInfo> {
    let mut v: alloc::vec::Vec<TaskInfo> = {
        let s = SCHED.lock();
        s.tasks
            .iter()
            .flatten()
            .map(|t| TaskInfo { pid: t.pid, name: t.name.clone(), state: t.state, user: t.user })
            .collect()
    };
    v.sort_by_key(|t| t.pid);
    v
}
