//! sinov — tizim çaqiruvlarini tekşiradi. Hammasi töğri bölsa, kod 42 bilan çiqadi.
//! Yadro uni yuklaniş paytida öz-özini tekşiriş uçun işga tuşiradi.
//! Xato bölsa, qaysi tekşiruv yiqilganini bildiruvçi kod qaytaradi.
#![no_std]
#![no_main]

use zamin::abi::{ERR_BAD_ADDRESS, ERR_DENIED, ERR_NOT_FOUND};
use zamin::{Channel, String, Vec, format, fs};

zamin::entry!(main);

fn main() -> i64 {
    // Bola rejimi: özini "bola" argumenti bilan işga tuşirganda — 7 qaytaradi.
    if zamin::args() == "bola" {
        return 7;
    }

    // 1–6: asosiy çaqiruvlar
    if zamin::pid() == 0 {
        return 1;
    }
    let t0 = zamin::uptime_ms();
    zamin::sleep_ms(20);
    if zamin::uptime_ms() < t0 + 20 {
        return 2;
    }
    zamin::yield_now();
    if zamin::write(b"") != 0 {
        return 3;
    }
    if zamin::write_raw(0x4020_0000, 16) != ERR_BAD_ADDRESS {
        return 4; // yadro xotirasi rad etilişi kerak
    }
    if zamin::write_raw(0x50_0000_0000, 16) != ERR_BAD_ADDRESS {
        return 5; // xaritalanmagan manzil rad etilişi kerak
    }
    if zamin::lang() > 2 {
        return 6;
    }

    // 10–12: xotira (heap)
    let mut v: Vec<u64> = Vec::new();
    for i in 0..20_000 {
        v.push(i);
    }
    if v.iter().sum::<u64>() != 199_990_000 {
        return 10;
    }
    let s: String = format!("{}-{}", "Zamin", 42);
    if s != "Zamin-42" {
        return 11;
    }
    let big = zamin::vec![7u8; 300_000]; // bir neçta sahifa
    if big.iter().map(|&x| x as u64).sum::<u64>() != 2_100_000 {
        return 12;
    }
    drop((v, big));

    // 20–26: fayllar
    let path = format!("/uy/sinov-{}.txt", zamin::pid());
    if fs::write(&path, b"Salom, fayl!").is_err() {
        return 20;
    }
    match fs::read_to_string(&path) {
        Ok(t) if t == "Salom, fayl!" => {}
        _ => return 21,
    }
    if !fs::list("/uy").unwrap_or_default().iter().any(|n| path.ends_with(n.as_str())) {
        return 22;
    }
    if fs::remove(&path).is_err() || fs::read(&path).map_err(|e| e.0) != Err(ERR_NOT_FOUND) {
        return 23;
    }
    if fs::remove("/tizim/versiya").map_err(|e| e.0) != Err(ERR_DENIED) {
        return 24; // tizim fayllari himoyalangan
    }
    if fs::mkdir("/uy/sinov-papka").is_err() || fs::remove("/uy/sinov-papka").is_err() {
        return 25;
    }
    match fs::read_to_string("/tizim/nomi") {
        Ok(t) if t.starts_with("Zamin OS") => {}
        _ => return 26,
    }

    // 30–32: Zamin kanallari (özimizga xabar yuborib, qabul qilamiz)
    let Ok(ch) = Channel::open(&format!("sinov-{}", zamin::pid())) else { return 30 };
    if ch.send("birinçi".as_bytes()).is_err() || ch.send("ikkinçi".as_bytes()).is_err() {
        return 31;
    }
    match (ch.recv_string(), ch.recv_string()) {
        (Ok(a), Ok(b)) if a == "birinçi" && b == "ikkinçi" => {}
        _ => return 32,
    }

    // 40–42: dastur işga tuşiriş, argument beriş va kutiş
    let Ok(child) = zamin::spawn("/dasturlar/sinov", "bola") else { return 40 };
    match zamin::wait(child) {
        Ok(7) => {}
        _ => return 41,
    }
    if zamin::wait(child).is_ok() {
        return 42; // ikkinçi marta kutib bölmaydi — u allaqaçon tozalangan
    }

    // 60–62: signallar (özimizga SIG_USER yuboramiz)
    use zamin::abi::SIG_USER;
    if zamin::signal(zamin::pid(), SIG_USER).is_err() {
        return 60;
    }
    if zamin::signals_pending() & (1 << SIG_USER) == 0 {
        return 61;
    }
    if zamin::signals_pending() != 0 {
        return 62; // öqilgaç tozalangan bölişi kerak
    }

    // 70–71: lazy heap — katta maydon ajratib, siyrak tegamiz (demand paging)
    let mut big: zamin::Vec<u8> = zamin::Vec::with_capacity(4 << 20);
    big.resize(4 << 20, 0);
    for i in (0..big.len()).step_by(4096) {
        big[i] = (i / 4096) as u8;
    }
    if big[0] != 0 || big[4096] != 1 || big[8192] != 2 {
        return 70;
    }
    drop(big);

    // 50–52: oqimlar va futex qulfi (4 oqim × 2000 = 8000)
    static COUNTER: zamin::sync::Mutex<u64> = zamin::sync::Mutex::new(0);
    let hs: Vec<_> = (0..4)
        .map(|k| {
            zamin::thread::spawn(move || {
                for _ in 0..2000 {
                    let mut g = COUNTER.lock();
                    *g += 1;
                }
                k
            })
        })
        .collect();
    let codes: i64 = hs.into_iter().map(|h| h.join()).sum();
    if codes != 0 + 1 + 2 + 3 {
        return 50;
    }
    if *COUNTER.lock() != 8000 {
        return 51;
    }
    if zamin::thread::current_tid() != zamin::pid() {
        return 52; // asosiy oqimning TID i — jarayon PID i
    }
    42
}
