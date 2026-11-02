//! oqimlar — bitta jarayon içida bir neçta oqim: tub sonlarni parallel sanaydi va
//! tezlikni bitta oqim bilan soliştiradi. `oqimlar [oqimlar soni] [çegara]`
#![no_std]
#![no_main]

use zamin::{Vec, println, thread, tr};

zamin::entry!(main);

fn is_prime(n: u64) -> bool {
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

fn count(lo: u64, hi: u64) -> i64 {
    (lo..hi).filter(|&n| is_prime(n)).count() as i64
}

fn run(threads: u64, limit: u64) -> (i64, u64) {
    let t0 = zamin::uptime_ms();
    // Toq-juft böliş: har bir oqimga teng ulüş (kiçik sonlar tez, katta sonlar sekin)
    let handles: Vec<_> = (0..threads)
        .map(|k| {
            thread::spawn(move || {
                let mut c = 0;
                let mut lo = k * 1000;
                while lo < limit {
                    c += count(lo, (lo + 1000).min(limit));
                    lo += threads * 1000;
                }
                c
            })
        })
        .collect();
    let total = handles.into_iter().map(|h| h.join()).sum();
    (total, zamin::uptime_ms() - t0)
}

fn main() -> i64 {
    let mut it = zamin::args().split_whitespace();
    let threads: u64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(4).clamp(1, 16);
    let limit: u64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(3_000_000);

    let (one, t1) = run(1, limit);
    println!("  1 {}: {} {} — {} ms", tr(["oqim", "поток", "thread"]), one, tr(["ta tub son", "простых", "primes"]), t1);
    let (many, tn) = run(threads, limit);
    println!("  {} {}: {} {} — {} ms", threads, tr(["oqim", "потоков", "threads"]), many, tr(["ta tub son", "простых", "primes"]), tn);
    if one != many {
        println!("  {}", tr(["XATO: natijalar har xil!", "ОШИБКА: результаты различаются!", "ERROR: results differ!"]));
        return 1;
    }
    println!("  {}: {}.{}x", tr(["Tezlaşiş", "Ускорение", "Speedup"]), t1 / tn.max(1), (t1 * 10 / tn.max(1)) % 10);
    0
}
