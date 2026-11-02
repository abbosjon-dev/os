//! soat — har soniyada hozirgi vaqtni körsatadi (5 marta). Uxlaş paytida protsessor
//! boşqa vazifalarga beriladi.
#![no_std]
#![no_main]

use zamin::{println, time, tr};

zamin::entry!(main);

fn main() -> i64 {
    let pid = zamin::pid();
    for _ in 0..5 {
        let t = time::now();
        println!(
            "  [soat {}] {} {:02}:{:02}:{:02}  ({:04}-{:02}-{:02})",
            pid,
            tr(["vaqt", "время", "time"]),
            t.hour,
            t.minute,
            t.second,
            t.year,
            t.month,
            t.day
        );
        zamin::sleep_ms(1000);
    }
    0
}
