//! hisobla — tub sonlarni sanaydi. Uzoq hisob-kitob davomida boşqa dasturlar
//! ham işlayveradi: bu köp vazifalilikni (preemptive multitasking) körsatadi.
#![no_std]
#![no_main]

use zamin::{println, tr};

zamin::entry!(main);

const LIMIT: u32 = 600_000;

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
    let start = zamin::uptime_ms();
    let mut count = 0;
    for step in 1..=4u32 {
        let lo = LIMIT / 4 * (step - 1);
        let hi = LIMIT / 4 * step;
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
