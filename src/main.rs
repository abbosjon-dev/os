//! Zamin OS — Rust'da noldan yozilgan, ARM64 uchun operatsion tizim yadrosi.
//! Dasturchi: Abbosjon Dev — https://zamin.app — info@zamin.app

#![no_std]
#![no_main]

extern crate alloc;

mod dtb;
mod exceptions;
mod memory;
mod power;
mod sync;
mod uart;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use memory::{PAGE_SIZE, frame, heap, mmu};

global_asm!(include_str!("arch/boot.s"));
global_asm!(include_str!("arch/vectors.s"));

unsafe extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

/// QEMU virt: RAM boshi va DTB manzili.
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

    println!("{}", BANNER);
    println!("Zamin OS v{} — AArch64", env!("CARGO_PKG_VERSION"));
    println!("Dasturchi: Abbosjon Dev | zamin.app | info@zamin.app");
    println!();
    println!("[boot] Exception level: EL{}", current_el());

    // 1) Device Tree: RAM hajmi va mashina modeli
    let fdt = unsafe { dtb::Fdt::from_addr(RAM_START) };
    let (ram_start, ram_size) = match fdt.and_then(|f| f.memory()) {
        Some(m) => m,
        None => {
            println!("[dtb ] topilmadi, standart RAM hajmi ishlatiladi");
            (RAM_START, DEFAULT_RAM_SIZE)
        }
    };
    if let Some(f) = fdt {
        println!("[dtb ] qurilma tavsifi o'qildi ({} KiB)", f.total_size() / 1024);
    }
    println!("[mem ] RAM: {:#x}..{:#x} ({} MiB)", ram_start, ram_start + ram_size, ram_size >> 20);

    let kstart = &raw const __kernel_start as usize;
    let kend = &raw const __kernel_end as usize;
    println!("[mem ] yadro: {:#x}..{:#x} ({} KiB)", kstart, kend, (kend - kstart) / 1024);

    // 2) MMU — atomik amallar (spinlock) shundan keyin ishonchli ishlaydi
    mmu::init(ram_start, ram_size);
    println!("[mmu ] yoqildi: 4 KiB sahifalar, {}-bit VA, keshlar yoqiq", mmu::VA_BITS);

    // 3) Fizik sahifa allokatori
    frame::init(ram_start, ram_size, kend);
    let (free, total) = frame::stats();
    println!("[mem ] sahifalar: {} bo'sh / {} jami", free, total);

    // 4) Kernel heap
    heap::init();
    println!("[heap] {} MiB tayyor", heap::stats().1 >> 20);

    self_tests();

    let (used, total) = heap::stats();
    let (free, _) = frame::stats();
    println!("[mem ] heap ishlatilmoqda: {} / {} bayt, bo'sh sahifalar: {}", used, total, free);
    println!();
    println!("Hammasi joyida. Tizim o'chirilmoqda...");
    power::shutdown();
}

/// Yadro o'z-o'zini tekshirishi.
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
    assert_eq!(heap::stats().0, 0, "heap'da xotira oqib ketdi");
    // Ko'p kichik ajratish/bo'shatish — bo'laklanish va birlashtirish
    for round in 0..50 {
        let parts: Vec<Box<[u8]>> = (0..64).map(|i| alloc::vec![round as u8; 16 + i * 24].into_boxed_slice()).collect();
        assert!(parts.iter().all(|p| p.iter().all(|&x| x == round as u8)));
    }
    assert_eq!(heap::stats().0, 0, "birlashtirishdan keyin heap bo'sh bo'lishi kerak");
    println!("[test] heap: Box/Vec/String va 3200 ajratish — OK");

    // MMU: yangi sahifani yuqori virtual manzilga xaritalash
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
    println!("[test] mmu: {:#x} -> {:#x} xaritalash, yozish va olib tashlash — OK", TEST_VA, pa);

    let _ = PAGE_SIZE;
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n*** KERNEL PANIC ***\n{}", info);
    power::halt();
}
