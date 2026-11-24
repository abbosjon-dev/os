//! Zamin OS — Rustʼda noldan yozilgan, ARM64 uçun operatsion tizim yadrosi.
//! Dasturçi: Abbosjon Dev — https://zamin.app — info@zamin.app

#![no_std]
#![no_main]

extern crate alloc;

#[allow(dead_code)] // ayrim doimiylar faqat foydalanuvçi kutubxonasida kerak
mod abi;
mod accounts;
mod acpi;
mod audio;
mod config;
mod cpuid;
mod crypto;
mod drivers;
mod dtb;
mod edit;
mod efi;
mod exceptions;
mod fs;
mod gfx;
mod gfx_font;
mod gic;
mod gui;
mod canvas;
mod clip;
mod css;
mod html;
mod i18n;
mod image;
mod ipc;
mod js;
mod line;
mod matn;
mod irq;
mod klog;
mod memory;
mod net;
mod pipe;
mod power;
mod random;
mod security;
mod setup;
mod shell;
mod sync;
mod task;
mod termbuf;
mod time;
mod timer;
mod uart;
mod video;
mod wallpaper;

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
pub extern "C" fn kernel_main(dtb_addr: usize) -> ! {
    // 1) Device Tree: EFI yuklovçi manzilini x0 da beradi; `-kernel` da QEMU uni RAM boşiga qöyadi.
    //    Undan: UART va GIC manzillari, RAM hajmi, til (bootargs: `til=ru`), PSCI usuli.
    //    x0 faqat RAM dagi tekislangan manzil bölsa işlatiladi (aks holda — QEMU standarti).
    let dtb_ok = dtb_addr % 8 == 0 && (RAM_START..1 << 48).contains(&dtb_addr);
    let fdt = unsafe { dtb::Fdt::from_addr(if dtb_ok { dtb_addr } else { RAM_START }) }
        .or_else(|| unsafe { dtb::Fdt::from_addr(RAM_START) });
    if let Some(u) = fdt.as_ref().and_then(|f| f.find_compatible("arm,pl011")) {
        if u.nreg > 0 {
            uart::configure(u.reg[0].0 as usize, u.irq.filter(|i| i.0 == 0).map(|i| i.1 + 32));
        }
    }
    if let Some(f) = fdt.as_ref() {
        gic::configure(f);
        timer::configure(f);
    }
    uart::console().init();
    exceptions::init();
    let efi = efi::boot_info();
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
    cpuid::report();

    if let Some(e) = efi {
        println!(
            "  UEFI: xotira xaritasi {} hudud, oddiy RAM {:#x}..{:#x}, band {} hudud; GICv{}",
            e.efi_regions, e.ram_lo, e.ram_hi, e.reserved_count, gic::version()
        );
        if let Some(g) = e.gop() {
            println!(
                "  GOP: {}x{}, format {}, qator {} piksel, framebuffer {:#x} ({} KiB), rejim {}/{}",
                g.width, g.height, if g.format == efi::GOP_RGBX { "RGBX" } else { "BGRX" },
                g.stride, g.base, g.size / 1024, g.mode, g.max_mode
            );
        }
        // QEMU fw_cfg (sinov: ramfb ni boşqa ölçam / qator qadami / RGBX ga qayta sozlaş)
        let fw = fdt.as_ref().and_then(|f| f.find_compatible("qemu,fw-cfg-mmio")).filter(|n| n.nreg > 0).map(|n| n.reg[0].0 as usize);
        let fw = fw.or_else(|| e.acpi().filter(|a| &a.oem == b"BOCHS ").map(|_| drivers::fwcfg::VIRT_BASE));
        if let Some(b) = fw {
            drivers::fwcfg::configure(b);
        }
        if let Some(a) = e.acpi() {
            acpi::report(a, |l| println!("{}", l));
            println!("  ACPI → DTB: taymer INTID {}, PSCI CPU_ON uçun {} ta MPIDR", timer::irq(), fdt.map_or(0, |f| f.cpus().1));
        }
    }
    let efi_ram = efi.filter(|e| e.ram_hi > e.ram_lo).map(|e| {
        // Yadro joylaşgan 1 GiB dan boşlab (yadro bağlangan manzil RAM boşiga bağliq)
        let lo = (e.ram_lo as usize).min(RAM_START);
        (lo, e.ram_hi as usize - lo)
    });
    let (ram_start, ram_size) = match fdt.and_then(|f| f.memory()).or(efi_ram) {
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
    if let Some(e) = efi {
        for &[a, b] in &e.reserved[..e.reserved_count as usize] {
            frame::reserve(a as usize, b as usize);
        }
    }
    let (free, total) = frame::stats();
    tprintln!(msg::MEM_FRAMES, free, total);

    // 4) Kernel heap
    heap::init();
    tprintln!(msg::HEAP_READY, heap::stats().1 >> 20);

    // Haqiqiy vaqt: PL031 (DTB) → UEFI GetTime + taymer hisoblagiçi → 0
    let src = timer::configure_rtc(fdt.as_ref(), efi.and_then(|e| e.efi_time()));
    println!("  Vaqt manbai: {}, {} UTC", src.name(), time::format_datetime(&time::utc(timer::unix_time())));

    // PCIe (bölsa): virtio-pci qurilmalari — MMU dan keyin, jarayonlardan oldin (ECAM/BAR xaritasi)
    if drivers::pci::init(fdt.as_ref()) > 0 {
        let names: Vec<&str> = drivers::pci::virtio_types()
            .iter()
            .map(|t| match t {
                1 => "net",
                2 => "blk",
                4 => "rng",
                16 => "gpu",
                18 => "input",
                25 => "sound",
                _ => "?",
            })
            .collect();
        println!("  PCIe: virtio-pci qurilmalari: {}", names.join(", "));
    }

    // 5) Uzilişlar va taymer
    irq::init();
    tprintln!(msg::IRQ_READY, gic::line_count(), timer::HZ, timer::frequency() / 1_000_000);
    drivers::pwrbtn::init(fdt.as_ref(), efi.and_then(|e| e.acpi()));

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
        Some(bytes) => tprintln!(msg::DISK_FOUND, drivers::virtio_blk::kind(), bytes >> 20),
        None => tprintln!(msg::DISK_NONE),
    }
    if let Some((num, lba)) = drivers::virtio_blk::partition() {
        tprintln!(msg::DISK_GPT, num, lba);
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
    let (kaslr_spots, kaslr_ok) = task::kaslr_check();
    assert!(kaslr_ok, "KASLR: yadro steklari sohasi notoʻgʻri");
    tprintln!(msg::TEST_KASLR, kaslr_spots);
    fs::init(task::programs::PROGRAMS);
    let (files, dirs, _) = fs::stats();
    tprintln!(msg::FS_READY, files, dirs);
    tprintln!(msg::SCHED_READY, 1000 / timer::HZ, task::programs::PROGRAMS.len());
    task::spawn_kernel("tekşiruv", boot_tests, 0).expect("tekşiruv oqimi");
    task::spawn_kernel("saqlovçi", saver, 0).expect("saqlovçi oqimi");
    task::spawn_kernel("quvvat", power::button_worker, 0).expect("quvvat oqimi");

    // 9) Qolgan protsessor yadrolarini uyğotamiz (PSCI CPU_ON)
    unsafe extern "C" {
        fn secondary_entry();
    }
    // CPU lar ro'yxati DTB `/cpus` dan (ACPI rejimida — MADT GICC MPIDR laridan yasalgan);
    // ro'yxat bölmasa — QEMU virt odati: MPIDR = 0..MAX_CPUS. Yadro raqami — MPIDR.Aff0.
    let (list, n) = fdt.map_or(([0; 8], 0), |f| f.cpus());
    let me = { let m: u64; unsafe { asm!("mrs {}, mpidr_el1", out(reg) m) }; m & 0xff_00ff_ffff };
    let mut want = 1;
    let mut seen = 1u64 << (me & 0xff).min(63);
    let targets: Vec<u64> = if n > 0 { list[..n].to_vec() } else { (0..task::MAX_CPUS as u64).collect() };
    for &m in &targets {
        let id = (m & 0xff) as usize;
        if m == me || id >= task::MAX_CPUS || seen & (1 << id) != 0 {
            continue;
        }
        seen |= 1 << id;
        if power::cpu_on(m, secondary_entry as *const () as usize, id as u64) {
            want += 1;
        }
    }
    let t0 = timer::uptime_ms();
    while task::cpus_online() < want && timer::uptime_ms() - t0 < 500 {
        core::hint::spin_loop();
    }
    tprintln!(msg::SMP_READY, task::cpus_online());

    // 10) Tarmoq: virtio-net, stek va DHCP (fon)
    match drivers::virtio_net::init() {
        Some(mac) => {
            tprintln!(msg::NET_NIC, format_args!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]));
            net::init();
            task::spawn_kernel("dhcp", net::dhcp::worker, 0).expect("dhcp oqimi");
            task::spawn_kernel("ipv6", net::ipv6::worker, 0).expect("ipv6 oqimi");
        }
        None => tprintln!(msg::NET_NONE),
    }

    // Övoz: virtio-sound (bölsa) — ijro fon oqimi
    if drivers::virtio_snd::init() {
        tprintln!(msg::SND_READY);
        task::spawn_kernel("övoz", drivers::virtio_snd::player, 0).expect("övoz oqimi");
    } else {
        tprintln!(msg::SND_NONE);
    }

    // 11) Ikkinçi disk — FAT fayl tizimi (bölsa) /fat ga ulanadi
    if drivers::virtio_blk::init_fat().is_some() {
        let n = fs::mount_fat();
        tprintln!(msg::FAT_MOUNTED, n);
    }

    // 12) Grafika: virtio-gpu (bölsa) — ekranga iş stolini çizamiz
    if gfx::init(1024, 768) {
        let (w, h) = gfx::size();
        tprintln!(msg::GPU_READY, gfx::backend(), w, h);
        let inputs = drivers::virtio_input::init();
        println!("  kiriş: {} ta virtio-input qurilma (klaviatura/planşet)", inputs);
        // USB (xHCI): Netcup kabi VPS larda VNC kirişi qemu-xhci + usb-kbd/usb-tablet orqali
        let usb = match drivers::xhci::init() {
            Some((m, n)) => {
                println!("  {}", m);
                n
            }
            None => 0,
        };
        let inputs = inputs + usb;
        draw_desktop();
        if inputs > 0 {
            // Grafik muhit — kiriş bilan (alohida oqim)
            task::spawn_kernel("grafik", gui::run, 0).expect("grafik oqim");
        }
    } else {
        tprintln!(msg::GPU_NONE);
    }

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
/// Oddiy grafik iş stoli (namuna): fon, yuqori panel, logotip, panellar.
fn draw_desktop() {
    use gfx::*;
    let (w, h) = size();
    gradient(0x16335e, 0x0a1322); // boşlangʼiç ekran (gui keyin egallaydi)
    let hero = (h / 7).clamp(40, 140);
    draw_text_centered(w / 2, h / 2 - hero, "Zamin OS", OQ, hero);
    let sub = (h / 36).clamp(14, 28);
    draw_text_centered(w / 2, h / 2 + hero / 3, "Rust'da noldan yozilgan ARM64 OS", 0x9fb0c3, sub);
    draw_text_centered(w / 2, h / 2 + hero / 3 + sub * 2, "yuklanmoqda…", 0x6c7f97, sub);
    flush();
}

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

/// Kamida 100 ms işlaydi va qaysi yadrolarda işlaganini belgilaydi.
extern "C" fn smp_probe(_: usize) {
    let start = timer::uptime_ms();
    // Kamida 100 ms, so'ng ikkinçi yadro körinmaguncha (yuklangan xostda 2 s gaça) aylanadi —
    // vCPU vaqtinça rejalaştirilmasa ham test tasodifan yiqilmaydi.
    loop {
        SEEN_CPUS.fetch_or(1 << task::cpu_id(), Ordering::Relaxed);
        let el = timer::uptime_ms() - start;
        if (el >= 100 && SEEN_CPUS.load(Ordering::Relaxed).count_ones() >= 2) || el >= 2000 {
            break;
        }
        core::hint::spin_loop();
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

        // SGI: dasturiy uziliş (GICv2: GICD_SGIR, GICv3: ICC_SGI1R_EL1) boşqa barça yadrolarga yetişi kerak
        // (oqim yadrodan yadroga köçişi mumkin — kutilgan niqob har yuborişda yangilanadi)
        let all = (1u64 << task::cpus_online()) - 1;
        let (mut want, mut got) = (0u64, 0u64);
        irq::take_sgi_seen();
        for _ in 0..20 {
            want |= all & !(1u64 << task::cpu_id());
            gic::send_sgi_others(irq::SGI_RESCHEDULE);
            task::sleep_ms(5);
            got |= irq::take_sgi_seen();
            if got & want == want {
                break;
            }
        }
        assert!(got & want == want, "SGI yetmadi: {:#x} / {:#x}", got, want);
        tprintln!(msg::TEST_SGI, (got & want).count_ones(), gic::version());
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
        // Javob halqasi uziliş işlovçisidan oldin körinişi mumkin (uziliş boşqa CPU ga yoki
        // biroz keçikib keladi) — hisoblagiçni qisqa muddat (≤ 1 s) kutamiz.
        let t0 = timer::counter();
        let limit = timer::frequency();
        while drivers::virtio_blk::irq_count() <= before && timer::counter().wrapping_sub(t0) < limit {
            core::hint::spin_loop();
        }
        assert!(drivers::virtio_blk::irq_count() > before, "disk uzilişi kelmadi");
        tprintln!(msg::TEST_DISK_IRQ);
    }

    // Yadro steki qörğoni: töliş aniqlanişi va faqat şu oqim tugatilişi kerak
    let bomb = task::spawn_kernel_ext("stek-sinovi", stack_bomb, 0, false).expect("oqim");
    assert_eq!(task::wait(bomb), Some(-11), "stek tölişi aniqlanmadi");
    tprintln!(msg::TEST_STACK_GUARD);

    // Foydalanuvçi rejimi: `sinov` dasturi 42 bilan çiqişi va xotira töliq qaytişi kerak
    let image = task::programs::find("sinov").expect("sinov dasturi yöq");
    // Isitiş: birinçi işga tuşiriş yadro heapini kerakli hajmgaça östiradi
    // (heap bir marta ösadi, qaytib qisqarmaydi) — şunda öljov aniq böladi.
    if let Ok(p) = task::spawn_user("sinov", image, "", task::SpawnOptions::kernel(false)) {
        task::wait(p);
    }
    // Öljov paytida fon oqimlari (tarmoq: DHCP/DNS, IPv6 RA/ping) yadro heapini östirişi mumkin —
    // şuning uçun 3 marta urinamiz: haqiqiy sizib çiqiş har safar takrorlanadi.
    let (mut frames_before, mut frames_after) = (0, 1);
    for _ in 0..3 {
        frames_before = frame::stats().0;
        let pid = task::spawn_user("sinov", image, "", task::SpawnOptions::kernel(false)).expect("sinov");
        let code = task::wait(pid);
        assert_eq!(code, Some(42), "sinov dasturi xato kod qaytardi");
        frames_after = frame::stats().0;
        if frames_after == frames_before {
            break;
        }
    }
    assert_eq!(frames_after, frames_before, "jarayon xotirasi töliq qaytmadi");
    tprintln!(msg::TEST_USER, 42);

    // Terminal: buyruqlar tarixi va Tab bilan töldiriş (sof mantiq, `terminal --self-test`)
    let image = task::programs::find("terminal").expect("terminal dasturi yöq");
    let pid = task::spawn_user("terminal", image, "--self-test", task::SpawnOptions::kernel(false)).expect("terminal");
    assert_eq!(task::wait(pid), Some(0), "terminal tarix/tab sinovi yiqildi");
    // Seriya qobiq matn buyruqlari (qidir/bosh/oxir/hajm/nusxa) — umumiy src/matn.rs
    assert_eq!(matn::self_test(), 0, "matn buyruqlari sinovi yiqildi");
    println!("  [ok] matn buyruqlari: qidir -i/-n, bosh/oxir -n N, hajm (qator/söz/bayt), nusxa/köçir manzili");

    // FAT yoziş: ikkinçi disk bölsa — fayl yozib, qayta öqib tekşiramiz
    if drivers::virtio_blk::fat_present() {
        use crate::fs::fat;
        let content = "Zamin OS — FAT yoziş sinovi (round-trip)\n".as_bytes();
        fat::write_root_file("ZTEST.TXT", content).expect("FAT yoziş");
        let files = fat::read_root().expect("FAT öqiş");
        let ok = files.iter().any(|(n, d)| n == "ZTEST.TXT" && d.as_slice() == content);
        assert!(ok, "FAT round-trip muvaffaqiyatsiz");
        tprintln!(msg::TEST_FAT_WRITE);
    }

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

    match time::self_test() {
        Ok(n) => println!("  [ok] EFI_TIME → Unix: kabisa yil, 2038 dan keyin, oy oxiri, xato sanalar ({} sinov)", n),
        Err(e) => panic!("EFI_TIME → Unix aylantiriş tekşiruvi yiqildi: {}", e),
    }

    // X.509: sertifikat tahlilçisi (muddat va host nomi tekşiruvi)
    net::x509::self_test().expect("X.509 tahlilçi tekşiruvi yiqildi");
    tprintln!(msg::TEST_X509);

    // Rasm dekoderlari: PNG, GIF, BMP, PNM, QOI
    match image::self_test() {
        Ok(n) => tprintln!(msg::TEST_IMAGE, n),
        Err(fmt) => panic!("rasm dekoderi yiqildi: {}", fmt),
    }

    // USB HID boot hisobotlari → Linux tugma kodlari (xHCI drayveri uçun)
    match drivers::usb_hid::self_test() {
        Ok(n) => println!("  [ok] USB HID: modifikatorlar, 6 tugmali rollover, böşatiş/uşlab turiş, planşet koordinata şkalasi ({} sinov)", n),
        Err(e) => panic!("USB HID sinovi yiqildi: {}", e),
    }

    // GPT bölimlar jadvali (VPS disk obrazi)
    match drivers::virtio_blk::self_test_gpt() {
        Ok(n) => println!("  [ok] GPT: Zamin bölimi, CRC32 (sarlavha/jadval), begona GPT ga tegilmaydi ({} sinov)", n),
        Err(e) => panic!("GPT tahlilçisi yiqildi: {}", e),
    }

    // virtio-scsi: SCSI buyruq bloklari (CDB)
    match drivers::virtio_scsi::self_test() {
        Ok(n) => println!("  [ok] virtio-scsi CDB: READ/WRITE(10) va (16) LBA/uzunlik maydonlari, LUN manzili, sense kaliti ({} sinov)", n),
        Err(e) => panic!("virtio-scsi CDB sinovi yiqildi: {}", e),
    }

    // JavaScript interpretatori va HTML/CSS tahlilçisi
    match js::self_test() {
        Ok(n) => println!("  [ok] JavaScript: {} ta sinov (funksiyalar, yopilmalar, massivlar, shablonlar); çaqiruv ~{} bayt stek", n, js::stack_per_call()),
        Err(e) => panic!("JavaScript interpretatori yiqildi: {}", e),
    }
    match js::self_test_canvas() {
        Ok(n) => println!("  [ok] <canvas> 2D: fillRect, strokeRect, yöl (lineTo/arc), fillText ({} sinov)", n),
        Err(e) => panic!("<canvas> 2D tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_rowspan() {
        Ok(n) => println!("  [ok] HTML jadval: rowspan ({} katak, joy egallovçilar bilan)", n),
        Err(e) => panic!("HTML jadval tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_position() {
        Ok(n) => println!("  [ok] CSS joylaşuv: position (relative/absolute) va float/clear ({} sinov)", n),
        Err(e) => panic!("CSS position/float tekşiruvi yiqildi: {}", e),
    }
    match gfx::self_test_round() {
        Ok(n) => println!("  [ok] yumaloq burçaklar: qamrov (r=1..30), çegara 1..4 px, gradient, opacity 128, kasr x=10.5 + toq r=7 + soya — yorugʻ/töq fonda töq piksel yöq ({} sinov)", n),
        Err(e) => panic!("yumaloq burçak tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_dir() {
        Ok(n) => println!("  [ok] dir=rtl va text-align (start/end/left/justify) ({} sinov)", n),
        Err(e) => panic!("dir/text-align tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_media() {
        Ok(n) => println!("  [ok] CSS @media (min/max-width) va :hover ({} sinov)", n),
        Err(e) => panic!("CSS @media/:hover tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_decor() {
        Ok(n) => println!("  [ok] CSS linear-gradient (töxtaşlar, burçak) va box-shadow ({} sinov)", n),
        Err(e) => panic!("CSS gradient/soya tekşiruvi yiqildi: {}", e),
    }
    match css::self_test_vars() {
        Ok(n) => println!("  [ok] CSS maxsus xossalar var(--x) (zaxira, meros, sikl) va calc() ({} sinov)", n),
        Err(e) => panic!("CSS var()/calc() tekşiruvi yiqildi: {}", e),
    }
    match drivers::fwcfg::self_test() {
        Ok(n) => println!("  [ok] ramfb spetsifikatsiyasi (WxH@qadam:format) tahlili ({} sinov)", n),
        Err(e) => panic!("ramfb tekşiruvi yiqildi: {}", e),
    }
    match css::self_test_selectors() {
        Ok(n) => println!("  [ok] CSS tanlagiçlar: > + ~, [atribut], :nth-child(an+b), :last-child, :not(), specificity ({} sinov)", n),
        Err(e) => panic!("CSS tanlagiç tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_box_height() {
        Ok(n) => println!("  [ok] CSS height/min-height, line-height va text-transform ({} sinov)", n),
        Err(e) => panic!("CSS height/line-height/text-transform tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_overflow() {
        Ok(n) => println!("  [ok] CSS overflow, white-space, text-overflow: ellipsis va opacity ({} sinov)", n),
        Err(e) => panic!("CSS overflow/white-space/opacity tekşiruvi yiqildi: {}", e),
    }
    match css::self_test_inline() {
        Ok(n) => println!("  [ok] CSS display: inline-block, vertical-align va visibility: hidden (joy saqlanadi, meros) ({} sinov)", n),
        Err(e) => panic!("CSS inline-block/visibility tekşiruvi yiqildi: {}", e),
    }
    match gui::self_test_inline() {
        Ok(n) => println!("  [ok] inline-block maketi: 3 quti bir qatorda (x o'sadi), 400 px da öraş, fon/radius, visibility: hidden joyi saqlanadi ({} sinov)", n),
        Err(e) => panic!("inline-block maket tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_nested() {
        Ok(n) => println!("  [ok] Ichma-ich konteynerlar: ota foni, çegarasi va padding'i bolalarni o'rab oladi ({} sinov)", n),
        Err(e) => panic!("ichma-ich konteyner tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_av() {
        Ok(n) => println!("  [ok] <audio>/<video>: WAV ijro havolasi (virtio-sound) va video poster ({} sinov)", n),
        Err(e) => panic!("audio/video tekşiruvi yiqildi: {}", e),
    }
    match html::self_test_iframe() {
        Ok(n) => println!("  [ok] <iframe>: srcdoc va bir xil manbali src ichki hujjat chizilişi ({} sinov)", n),
        Err(e) => panic!("iframe tekşiruvi yiqildi: {}", e),
    }
    match termbuf::self_test() {
        Ok(n) => println!("  [ok] terminal scrollback: {} qator çegarasi, söz/belgi böyiça öraş, UTF-8, tanlov va söz çegarasi ({} sinov)", termbuf::MAX_LINES, n),
        Err(e) => panic!("terminal scrollback tekşiruvi yiqildi: {}", e),
    }
    match termbuf::self_test_sgr() {
        Ok(n) => println!("  [ok] SGR: \\x1b[1;31m (qalin qizil), \\x1b[0m, \\x1b[92m, 2 öqişga bölingan \\x1b[3 + 4m, 38;5;n / 39 / 49, noma'lum CSI va OSC eʼtiborsiz, tarix kesilişida atributlar saqlanadi ({} sinov)", n),
        Err(e) => panic!("SGR tekşiruvi yiqildi: {}", e),
    }
    match gui::self_test_term_ctrl() {
        Ok(n) => println!("  [ok] terminal Ctrl: C (töxtatiş / qatorni bekor qiliş), L (tozalaş), U (qatorni öçiriş), D (yopiş), oldingi plan bor/yöq ({} sinov)", n),
        Err(e) => panic!("terminal Ctrl tekşiruvi yiqildi: {}", e),
    }
    match edit::self_test() {
        Ok(n) => println!("  [ok] muharrir undo/qidiruv/tanlaş: söz guruhlaş, {} qadam, UTF-8 ustun, köp qatorli nusxa/kesiş/joylaş ({} sinov)", edit::MAX_STEPS, n),
        Err(e) => panic!("muharrir undo/qidiruv tekşiruvi yiqildi: {}", e),
    }
    match gui::self_test_files_sort() {
        Ok(n) => println!("  [ok] fayllar saralaş: nom/hajm/sana/tur, teskari tartib, papkalar birinçi, human_size ({} sinov)", n),
        Err(e) => panic!("fayllar saralaş tekşiruvi yiqildi: {}", e),
    }
    match gui::self_test_files_filter() {
        Ok(n) => println!("  [ok] fayllar filtr: katta-kiçik harfsiz qidiruv (ö/ğ/ş/ç, kirill), böş so'rov — hammasi, yöl bölaklari va '…' qisqartiriş ({} sinov)", n),
        Err(e) => panic!("fayllar filtr tekşiruvi yiqildi: {}", e),
    }
    match accounts::self_test() {
        Ok(n) => println!("  [ok] hisob sozlamalari: parolni özgartiriş tekşiruvi, set_password (yangi — ötadi, eski — yöq), login validatsiyasi ({} sinov)", n),
        Err(e) => panic!("hisob sozlamalari tekşiruvi yiqildi: {}", e),
    }
    match net::webstore::self_test() {
        Ok(n) => println!("  [ok] localStorage/cookie diskka saqlaş (~/.zamin): kodlaş ({} yozuv)", n),
        Err(e) => panic!("localStorage/cookie saqlaş tekşiruvi yiqildi: {}", e),
    }
    match net::ipcfg::self_test() {
        Ok(n) => println!("  [ok] tarmoq sozlamalari: parse_ipv4, niqob (/24 va nuqtali), validate_static (tarmoq/broadcast/0.0.0.0/uzluksiz bölmagan niqob rad), /32 va on-link bölmagan şlyuz, saqlaş → öqiş aylanişi ({} sinov)", n),
        Err(e) => panic!("tarmoq sozlamalari tekşiruvi yiqildi: {}", e),
    }
    match net::ipv6::self_test() {
        Ok(n) => println!("  [ok] IPv6: EUI-64 link-local, solicited-node manzil va 33:33:ff MAC filtri, ICMPv6 summasi (psevdo-sarlavha) ma'lum vektorga teng, NS/NA tahlili, RA prefiksi → SLAAC, RFC 5952 format, statik /64 saqlaş ({} sinov)", n),
        Err(e) => panic!("IPv6 tekşiruvi yiqildi: {}", e),
    }
    match drivers::virtio_snd::self_test_volume() {
        Ok(n) => println!("  [ok] övoz balandligi: scale_samples 100%/50%/0%/övozsiz, i16 çegarasi ({} sinov)", n),
        Err(e) => panic!("övoz balandligi tekşiruvi yiqildi: {}", e),
    }
    // Audio dekoderi: WAV
    audio::self_test().expect("audio (WAV) dekoderi yiqildi");
    tprintln!(msg::TEST_AUDIO);

    // Video dekoderi: MJPEG/AVI + konteyner taniş (MP4/MOV/MKV/WebM)
    video::self_test().expect("video (MJPEG) dekoderi yiqildi");
    tprintln!(msg::TEST_VIDEO);

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
