//! soat — har soniyada vaqtni körsatadi (5 marta). Uxlaş paytida protsessor
//! boşqa vazifalarga beriladi.
#![no_std]
#![no_main]

use zamin::{println, tr};

zamin::entry!(main);

fn main() -> i64 {
    let pid = zamin::pid();
    for _ in 0..5 {
        let s = zamin::uptime_ms() / 1000;
        println!(
            "  [soat {}] {} {:02}:{:02}:{:02}",
            pid,
            tr(["vaqt", "время", "time"]),
            s / 3600,
            (s / 60) % 60,
            s % 60
        );
        zamin::sleep_ms(1000);
    }
    0
}
