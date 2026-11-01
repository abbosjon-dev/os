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
    0
}
