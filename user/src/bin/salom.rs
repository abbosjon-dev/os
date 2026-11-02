//! salom — oddiy salomlaşiş dasturi.
#![no_std]
#![no_main]

use zamin::{println, tr};

zamin::entry!(main);

fn main() -> i64 {
    println!(
        "{}",
        tr([
            "Salom! Men Zamin OS foydalanuvçi dasturiman.",
            "Привет! Я пользовательская программа Zamin OS.",
            "Hello! I am a Zamin OS user program.",
        ])
    );
    println!(
        "  PID: {}, {}: {} ms",
        zamin::pid(),
        tr(["tizim vaqti", "время системы", "system time"]),
        zamin::uptime_ms()
    );
    // ASLR: har işga tuşganda stek va heap manzili boşqaça
    let on_stack = 0u8;
    let on_heap = zamin::Box::new(0u8);
    println!(
        "  {}: {:#x}, heap: {:#x}",
        tr(["stek", "стек", "stack"]),
        &on_stack as *const u8 as usize,
        &*on_heap as *const u8 as usize
    );
    0
}
