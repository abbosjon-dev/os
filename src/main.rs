//! Zamin OS — Rustʼda noldan yozilgan, ARM64 uçun operatsion tizim yadrosi.
//! Dasturçi: Abbosjon Dev — https://zamin.app — info@zamin.app

#![no_std]
#![no_main]

extern crate alloc;

#[allow(dead_code)] // ayrim doimiylar faqat foydalanuvçi kutubxonasida kerak
mod abi;
mod accounts;
mod config;
mod crypto;
mod drivers;
mod dtb;
mod exceptions;
mod fs;
mod gic;
mod i18n;
mod ipc;
mod line;
mod irq;
mod klog;
mod memory;
mod pipe;
mod power;
mod random;
mod security;
mod setup;
mod shell;
mod sync;
mod task;
mod time;
mod timer;
mod uart;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicU64, Ordering};
use i18n::msg;
use memory::{PAGE_SIZE, frame, heap, mmu};

global_asm!(include_str!("arch/boot.s"));
global_asm!(include_str!("arch/vectors.s"));
global_asm!(include_str!("arch/switch.s"));

unsafe extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

/// QEMU virt: RAM boşi va DTB manzili.
const RAM_START: usize = 0x4000_0000;
const DEFAULT_RAM_SIZE: usize = 128 * 1024 * 1024;

const BANNER: &str = r"
 _____               _          ___  ____
|__  /__ _ _ __ ___ (_)_ __    / _ \/ ___|
  / // _` | '_ ` _ \| | '_ \  | | | \___ \
 / /| (_| | | | | | | | | | | | |_| |___) |
/____\__,_|_| |_| |_|_|_| |_|  \___/|____/
";

fn current_el() -> u64 {
    let el: u64;
    unsafe { asm!("mrs {}, CurrentEL", out(reg) el) };
    (el >> 2) & 0b11
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    uart::console().init();
    exceptions::init();

    // 1) Device Tree: RAM hajmi, til (bootargs: `til=ru`), PSCI usuli
    let fdt = unsafe { dtb::Fdt::from_addr(RAM_START) };
    if let Some(lang) = fdt.as_ref().and_then(boot_lang) {
        i18n::set(lang);
    }
    power::init(fdt.as_ref());

    println!("{}", BANNER);
    println!("Zamin OS v{} — AArch64", env!("CARGO_PKG_VERSION"));
    tprintln!(msg::DEVELOPER);
    println!();
    tprintln!(msg::BOOT_LANG, i18n::pick(&msg::LANG_NAME));
    tprintln!(msg::BOOT_EL, current_el());

    let (ram_start, ram_size) = match fdt.and_then(|f| f.memory()) {
        Some(m) => m,
        None => {
            tprintln!(msg::DTB_MISSING);
            (RAM_START, DEFAULT_RAM_SIZE)
        }
    };
    if let Some(f) = fdt {
        tprintln!(msg::DTB_OK, f.total_size() / 1024);
    }
    tprintln!(
        msg::MEM_RAM,
        format_args!("{:#x}", ram_start),
        format_args!("{:#x}", ram_start + ram_size),
        ram_size >> 20
    );

    let kstart = &raw const __kernel_start as usize;
    let kend = &raw const __kernel_end as usize;
    tprintln!(
        msg::MEM_KERNEL,
        format_args!("{:#x}", kstart),
        format_args!("{:#x}", kend),
        (kend - kstart) / 1024
    );

    // 2) MMU — atomik amallar (spinlock) şundan keyin işonçli işlaydi
    mmu::init(ram_start, ram_size);
    tprintln!(msg::MMU_ON, mmu::VA_BITS);
    mmu::enable_pan();

    // 3) Fizik sahifa allokatori
    frame::init(ram_start, ram_size, kend);
    let (free, total) = frame::stats();
    tprintln!(msg::MEM_FRAMES, free, total);

    // 4) Kernel heap
    heap::init();
    tprintln!(msg::HEAP_READY, heap::stats().1 >> 20);

    // 5) Uzilişlar va taymer
    irq::init();
    tprintln!(msg::IRQ_READY, gic::line_count(), timer::HZ, timer::frequency() / 1_000_000);

    // 6) Tasodifiy sonlar generatori (kalitlar, nonce, tuz uçun)
    random::init();
    tprintln!(msg::TIME_NOW, time::format_datetime(&time::now()), time::tz_name());
    let (virtio, rndr) = random::sources();
    tprintln!(
        msg::RNG_READY,
        format_args!("{}{}timer", if virtio { "virtio-rng, " } else { "" }, if rndr { "RNDR, " } else { "" })
    );

    // 7) Disk va saqlangan sozlamalar (til, hisob)
    let disk = drivers::virtio_blk::init();
    match disk {
        Some(bytes) => tprintln!(msg::DISK_FOUND, bytes >> 20),
        None => tprintln!(msg::DISK_NONE),
    }
    if let Some(p) = config::take_panic() {
        tprintln!(msg::PREV_PANIC, p);
    }
    match config::load() {
        Some(s) => {
            i18n::set(s.lang);
            let who: Vec<String> = s.accounts.iter().map(|a| a.username.clone()).collect();
            tprintln!(msg::CONFIG_LOADED, who.join(", "));
            accounts::set_all(s.accounts);
        }
        None => tprintln!(msg::CONFIG_FRESH),
    }

    self_tests();

    // 8) Jarayonlar: şu kod PID 0 ("yadro") bölib qoladi
    task::init();
    fs::init(task::programs::PROGRAMS);
    let (files, dirs, _) = fs::stats();
    tprintln!(msg::FS_READY, files, dirs);
    tprintln!(msg::SCHED_READY, 1000 / timer::HZ, task::programs::PROGRAMS.len());
    task::spawn_kernel("tekşiruv", boot_tests, 0).expect("tekşiruv oqimi");
    task::spawn_kernel("saqlovçi", saver, 0).expect("saqlovçi oqimi");

    // 9) Qolgan protsessor yadrolarini uyğotamiz (PSCI CPU_ON)
    unsafe extern "C" {
        fn secondary_entry();
    }
    for cpu in 1..task::MAX_CPUS {
        power::cpu_on(cpu as u64, secondary_entry as *const () as usize, cpu as u64);
    }
    let t0 = timer::uptime_ms();
    while task::cpus_online() < task::MAX_CPUS && timer::uptime_ms() - t0 < 500 {
        core::hint::spin_loop();
    }
    tprintln!(msg::SMP_READY, task::cpus_online());

    // PID 0: boşqa iş bölmaganda — tugagan vazifalarni tozalaş va uxlaş
    loop {
        task::reap_detached();
        unsafe { asm!("wfi") };
    }
}

/// Ikkilamçi yadro: MMU, istisnolar, uzilişlar, keyin öz böş vazifasi.
#[unsafe(no_mangle)]
pub extern "C" fn secondary_main(cpu: usize) -> ! {
    mmu::enable_this_cpu();
    mmu::enable_pan();
    exceptions::init();
    irq::init_secondary();
    task::init_cpu(cpu);
    gic::irqs_enable();
    loop {
        task::reap_detached();
        unsafe { asm!("wfi") };
    }
}

/// Fon oqimi: har 2 soniyada özgarişlarni şifrlab diskka yozadi.
extern "C" fn saver(_: usize) {
    loop {
        task::sleep_ms(2000);
        if fs::take_dirty() && fs::commit().is_err() {
            fs::set_dirty(); // kalit yöq yoki disk xatosi — keyinroq qayta urinamiz
        }
    }
}

static COUNTER_A: AtomicU64 = AtomicU64::new(0);
static COUNTER_B: AtomicU64 = AtomicU64::new(0);

/// Heç qaçon navbat bermaydigan oqim: faqat taymer uni töxtata oladi.
extern "C" fn busy_counter(which: usize) {
    let c = if which == 0 { &COUNTER_A } else { &COUNTER_B };
    let start = timer::uptime_ms();
    while timer::uptime_ms() - start < 100 {
        c.fetch_add(1, Ordering::Relaxed);
    }
}

const MUTEX_ROUNDS: u64 = 200;
static MUTEX_COUNTER: sync::Mutex<u64> = sync::Mutex::new(0);

extern "C" fn mutex_worker(_: usize) {
    for _ in 0..MUTEX_ROUNDS {
        let mut g = MUTEX_COUNTER.lock();
        let v = *g;
        task::yield_now(); // boşqalar şu payt qulfni kutib uxlaşi kerak
        *g = v + 1;
    }
}

static SEEN_CPUS: AtomicU64 = AtomicU64::new(0);

/// 100 ms band işlaydi va qaysi yadrolarda işlaganini belgilaydi.
extern "C" fn smp_probe(_: usize) {
    let start = timer::uptime_ms();
    while timer::uptime_ms() - start < 100 {
        SEEN_CPUS.fetch_or(1 << task::cpu_id(), Ordering::Relaxed);
    }
}

/// Ataylab yadro stekini töldiradi: har darajada 1 KiB, 100 daraja > 32 KiB.
#[inline(never)]
fn deep_recursion(n: u64) -> u64 {
    let buf = core::hint::black_box([n as u8; 1024]);
    if n == 0 { buf[0] as u64 } else { deep_recursion(n - 1) + buf[(n % 1024) as usize] as u64 }
}

extern "C" fn stack_bomb(_: usize) {
    core::hint::black_box(deep_recursion(100));
}

/// Rejalaştiruvçi va foydalanuvçi rejimini tekşiradi, keyin terminalni oçadi.
extern "C" fn boot_tests(_: usize) {
    // Ikki "oçköz" oqim bir vaqtda — ikkalasi ham oldinga siljişi kerak
    let a = task::spawn_kernel("hisoblagiç-a", busy_counter, 0).expect("oqim");
    let b = task::spawn_kernel("hisoblagiç-b", busy_counter, 1).expect("oqim");
    task::sleep_ms(150);
    task::wait(a);
    task::wait(b);
    let (ca, cb) = (COUNTER_A.load(Ordering::Relaxed), COUNTER_B.load(Ordering::Relaxed));
    assert!(ca > 0 && cb > 0, "oqimlar almaşmayapti: {} / {}", ca, cb);
    tprintln!(msg::TEST_SCHED, ca, cb);

    // Fuzz: tizim çaqiruvlariga köplab tasodifiy argumentlar — yadro qulamasligi kerak
    let fz = task::programs::find("fuzz").expect("fuzz dasturi");
    let pid = task::spawn_user("fuzz", fz, "15000", task::SpawnOptions::kernel(false)).expect("fuzz");
    assert_eq!(task::wait(pid), Some(0), "fuzz tugamadi");
    tprintln!(msg::TEST_FUZZ, 15000);

    // SMP: oqimlar turli yadrolarda işlaşi kerak
    if task::cpus_online() > 1 {
        let ids: Vec<_> = (0..task::cpus_online())
            .map(|i| task::spawn_kernel_ext("smp-sinovi", smp_probe, i, false).expect("oqim"))
            .collect();
        for id in ids {
            task::wait(id);
        }
        let seen = SEEN_CPUS.load(Ordering::Relaxed).count_ones();
        assert!(seen >= 2, "oqimlar faqat bitta yadroda işladi");
        tprintln!(msg::TEST_SMP, seen);
    }

    // Mutex: 4 oqim, qulf içida ataylab navbat böşatiladi — natija aniq bölişi kerak
    let ids: Vec<_> = (0..4).map(|i| task::spawn_kernel_ext("mutex-sinovi", mutex_worker, i, false).expect("oqim")).collect();
    for id in ids {
        task::wait(id);
    }
    let total = *MUTEX_COUNTER.lock();
    assert_eq!(total, 4 * MUTEX_ROUNDS, "mutex himoyasi buzildi");
    tprintln!(msg::TEST_MUTEX, total);

    // Disk: söröv tugaşi uziliş orqali kelişi kerak (vazifa uxlab kutadi)
    if drivers::virtio_blk::present() {
        let before = drivers::virtio_blk::irq_count();
        let mut sector = [0u8; drivers::virtio_blk::SECTOR];
        drivers::virtio_blk::read(0, &mut sector).expect("disk öqiş");
        assert!(drivers::virtio_blk::irq_count() > before, "disk uzilişi kelmadi");
        tprintln!(msg::TEST_DISK_IRQ);
    }

    // Yadro steki qörğoni: töliş aniqlanişi va faqat şu oqim tugatilişi kerak
    let bomb = task::spawn_kernel_ext("stek-sinovi", stack_bomb, 0, false).expect("oqim");
    assert_eq!(task::wait(bomb), Some(-11), "stek tölişi aniqlanmadi");
    tprintln!(msg::TEST_STACK_GUARD);

    // Foydalanuvçi rejimi: `sinov` dasturi 42 bilan çiqişi va xotira töliq qaytişi kerak
    let frames_before = frame::stats().0;
    let image = task::programs::find("sinov").expect("sinov dasturi yöq");
    let pid = task::spawn_user("sinov", image, "", task::SpawnOptions::kernel(false)).expect("sinov");
    let code = task::wait(pid);
    assert_eq!(code, Some(42), "sinov dasturi xato kod qaytardi");
    assert_eq!(frame::stats().0, frames_before, "jarayon xotirasi töliq qaytmadi");
    tprintln!(msg::TEST_USER, 42);

    println!();
    task::spawn_kernel("terminal", shell::run, drivers::virtio_blk::present() as usize).expect("terminal");
}

/// Yuklaş parametrlaridan tilni öqiydi: `til=uz|ru|en` (yoki `lang=`).
fn boot_lang(fdt: &dtb::Fdt) -> Option<i18n::Lang> {
    let raw = fdt.top_level_prop("chosen", "bootargs")?;
    let args = core::ffi::CStr::from_bytes_until_nul(raw).ok()?.to_str().ok()?;
    args.split_whitespace()
        .filter_map(|a| a.strip_prefix("til=").or_else(|| a.strip_prefix("lang=")))
        .find_map(i18n::Lang::from_code)
}

/// Yadro öz-özini tekşirişi.
fn self_tests() {
    // Istisnolar
    unsafe { asm!("brk #42") };

    // Heap: Box, Vec, String (test boşidagi holatga qaytişi kerak)
    let heap_base = heap::stats().0;
    let b = Box::new(0x5a5a_u64);
    let mut v: Vec<u32> = Vec::new();
    for i in 0..10_000 {
        v.push(i * i);
    }
    let sum: u64 = v.iter().map(|&x| x as u64).sum();
    let s = String::from("Zamin") + " OS";
    assert_eq!(*b, 0x5a5a);
    assert_eq!(sum, 333_283_335_000);
    assert_eq!(s, "Zamin OS");
    drop((b, v, s));
    assert_eq!(heap::stats().0, heap_base, "heapʼda xotira oqib ketdi");
    // Köp kiçik ajratiş/böşatiş — bölaklaniş va birlaştiriş
    for round in 0..50 {
        let parts: Vec<Box<[u8]>> = (0..64).map(|i| alloc::vec![round as u8; 16 + i * 24].into_boxed_slice()).collect();
        assert!(parts.iter().all(|p| p.iter().all(|&x| x == round as u8)));
    }
    assert_eq!(heap::stats().0, heap_base, "birlaştirişdan keyin heap boşlangʼiç holatga qaytişi kerak");
    tprintln!(msg::TEST_HEAP);

    // MMU: yangi sahifani yuqori virtual manzilga xaritalaş
    const TEST_VA: usize = 0x10_0000_0000; // 64 GiB
    let pa = frame::alloc_zeroed().expect("frame");
    mmu::map_page(TEST_VA, pa, mmu::Kind::KernelData).expect("map_page");
    assert_eq!(mmu::translate(TEST_VA + 0x123), Some(pa + 0x123));
    unsafe {
        (TEST_VA as *mut u64).write_volatile(0xC0FFEE);
        assert_eq!((pa as *const u64).read_volatile(), 0xC0FFEE);
    }
    assert_eq!(mmu::unmap_page(TEST_VA), Some(pa));
    assert_eq!(mmu::translate(TEST_VA), None);
    frame::free(pa);
    tprintln!(msg::TEST_MMU, format_args!("{:#x}", TEST_VA), format_args!("{:#x}", pa));

    let _ = PAGE_SIZE;

    // W^X: kod yozilmaydi, maʼlumot bajarilmaydi
    let code = kernel_main as *const () as usize;
    let ro = BANNER.as_ptr() as usize;
    let data = &raw const COUNTER_A as usize;
    let heap = Box::new(0u8);
    assert_eq!(mmu::kernel_access(code), Some((false, true)), "kod sahifasi");
    assert_eq!(mmu::kernel_access(ro), Some((false, false)), "rodata sahifasi");
    assert_eq!(mmu::kernel_access(data), Some((true, false)), "data sahifasi");
    assert_eq!(mmu::kernel_access(&*heap as *const u8 as usize), Some((true, false)), "heap sahifasi");
    drop(heap);
    tprintln!(msg::TEST_WX);

    // PAN: oddiy holatda yoqiq, faqat nusxalaş oraliğida öçiq
    if mmu::pan_enabled() {
        assert!(mmu::pan_state(), "PAN yoqilmagan");
        assert!(!mmu::with_user_access(mmu::pan_state), "PAN öçmadi");
        assert!(mmu::pan_state(), "PAN qayta yoqilmadi");
        tprintln!(msg::TEST_PAN);
    }

    if let Err(alg) = crypto::self_test() {
        panic!("kriptografiya tekşiruvi yiqildi: {}", alg);
    }
    tprintln!(msg::TEST_SHA);

    // Taymer uzilişlari: 50 ms içida kamida 3 ta tick kelişi kerak
    let start_ticks = timer::ticks();
    let start = timer::uptime_ms();
    while timer::uptime_ms() - start < 50 {
        core::hint::spin_loop();
    }
    let got = timer::ticks() - start_ticks;
    assert!(got >= 3, "taymer uzilişlari kelmayapti ({} ta)", got);
    tprintln!(msg::TEST_TIMER, got);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    uart::force_unlock_console();
    println!("\n{}\n{}", i18n::pick(&msg::PANIC), info);
    // Xabarni diskka ham yozamiz: keyingi yuklanişda körsatiladi (heap işlatilmaydi)
    struct Buf([u8; 480], usize);
    impl core::fmt::Write for Buf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let n = s.len().min(self.0.len() - self.1);
            self.0[self.1..self.1 + n].copy_from_slice(&s.as_bytes()[..n]);
            self.1 += n;
            Ok(())
        }
    }
    let mut b = Buf([0; 480], 0);
    let _ = core::fmt::write(&mut b, format_args!("CPU {}, {} ms: {}", task::cpu_id(), timer::uptime_ms(), info));
    config::save_panic(&b.0[..b.1]);
    power::halt();
}
