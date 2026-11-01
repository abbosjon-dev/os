//! Zamin OS — Rustʼda noldan yozilgan, ARM64 uçun operatsion tizim yadrosi.
//! Dasturçi: Abbosjon Dev — https://zamin.app — info@zamin.app

#![no_std]
#![no_main]

extern crate alloc;

mod dtb;
mod exceptions;
mod gic;
mod i18n;
mod irq;
mod memory;
mod power;
mod shell;
mod sync;
mod timer;
mod uart;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use i18n::msg;
use memory::{PAGE_SIZE, frame, heap, mmu};

global_asm!(include_str!("arch/boot.s"));
global_asm!(include_str!("arch/vectors.s"));

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

    self_tests();

    println!();
    let mut shell = shell::Shell::new();
    shell.start();
    loop {
        while let Some(b) = uart::read_byte() {
            shell.feed(b);
        }
        gic::wait_for_interrupt_unless(uart::has_input);
    }
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

    // Heap: Box, Vec, String
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
    assert_eq!(heap::stats().0, 0, "heapʼda xotira oqib ketdi");
    // Köp kiçik ajratiş/böşatiş — bölaklaniş va birlaştiriş
    for round in 0..50 {
        let parts: Vec<Box<[u8]>> = (0..64).map(|i| alloc::vec![round as u8; 16 + i * 24].into_boxed_slice()).collect();
        assert!(parts.iter().all(|p| p.iter().all(|&x| x == round as u8)));
    }
    assert_eq!(heap::stats().0, 0, "birlaştirişdan keyin heap böş bölişi kerak");
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
    println!("\n{}\n{}", i18n::pick(&msg::PANIC), info);
    power::halt();
}
