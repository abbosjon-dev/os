//! Nur OS — Rust'da noldan yozilgan, ARM64 uchun operatsion tizim yadrosi.

#![no_std]
#![no_main]

mod exceptions;
mod power;
mod uart;

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

global_asm!(include_str!("arch/boot.s"));
global_asm!(include_str!("arch/vectors.s"));

unsafe extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

fn current_el() -> u64 {
    let el: u64;
    unsafe { asm!("mrs {}, CurrentEL", out(reg) el) };
    (el >> 2) & 0b11
}

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    uart::console().init();
    exceptions::init();

    println!();
    println!("  _   _              ___  ____  ");
    println!(" | \\ | |_   _ _ __  / _ \\/ ___| ");
    println!(" |  \\| | | | | '__|| | | \\___ \\ ");
    println!(" | |\\  | |_| | |   | |_| |___) |");
    println!(" |_| \\_|\\__,_|_|    \\___/|____/ ");
    println!();
    println!("Nur OS v{} — AArch64", env!("CARGO_PKG_VERSION"));
    println!("Exception level: EL{}", current_el());
    let (start, end) = (&raw const __kernel_start as usize, &raw const __kernel_end as usize);
    println!("Yadro xotirada: {:#x}..{:#x} ({} KiB)", start, end, (end - start) / 1024);

    // Istisnolar tizimini sinab ko'ramiz.
    unsafe { asm!("brk #42") };

    println!("Hammasi joyida. Tizim o'chirilmoqda...");
    power::shutdown();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n*** KERNEL PANIC ***\n{}", info);
    power::halt();
}
