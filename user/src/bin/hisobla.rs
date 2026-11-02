//! hisobla — tub sonlarni sanaydi. Uzoq hisob-kitob davomida boşqa dasturlar
//! ham işlayveradi: bu köp vazifalilikni (preemptive multitasking) körsatadi.
//! Çegara argument orqali beriladi: `işlat hisobla 1000000`.
#![no_std]
#![no_main]

use zamin::{println, tr};

zamin::entry!(main);

const DEFAULT_LIMIT: u32 = 600_000;

fn is_prime(n: u32) -> bool {
    if n < 2 {
        return false;
    }
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            return false;
        }
        d += 1;
    }
    true
}

fn main() -> i64 {
    let pid = zamin::pid();
    let limit: u32 = zamin::args().trim().parse().unwrap_or(DEFAULT_LIMIT).max(4);
    let start = zamin::uptime_ms();
    let mut count = 0;
    for step in 1..=4u32 {
        let lo = limit / 4 * (step - 1);
        let hi = if step == 4 { limit } else { limit / 4 * step };
        count += (lo..hi).filter(|&n| is_prime(n)).count();
        println!(
            "  [hisobla {}] {}%: {} {}",
            pid,
            step * 25,
            count,
            tr(["ta tub son", "простых чисел", "primes"])
        );
    }
    println!(
        "  [hisobla {}] {} {} ms",
        pid,
        tr(["tayyor:", "готово:", "done:"]),
        zamin::uptime_ms() - start
    );
    0
}
