//! buzuq — ataylab yadro xotirasini öqişga urinadi. Yadro buni taqiqlaydi va
//! jarayonni tugatadi — tizimning qolgan qismi işlayveradi.
#![no_std]
#![no_main]

use zamin::{println, tr};

zamin::entry!(main);

fn main() -> i64 {
    println!(
        "  {}",
        tr([
            "Yadro xotirasini öqişga urinaman (0x40200000)...",
            "Пытаюсь прочитать память ядра (0x40200000)...",
            "Trying to read kernel memory (0x40200000)...",
        ])
    );
    let v = unsafe { core::ptr::read_volatile(0x4020_0000 as *const u64) };
    println!("  ??? {:#x}", v); // bu qatorga heç qaçon yetib kelinmaydi
    0
}
