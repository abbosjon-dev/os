//! katta — katta fayl bilan işlaşni tekşiradi (ZaminFS 2).
//!   işlat katta 8        — ~/katta.bin ga 8 MB yozadi va qayta öqib tekşiradi
//!   işlat katta tekşir   — mavjud faylni öqib, har bir baytini tekşiradi
#![no_std]
#![no_main]

use zamin::fs::File;
use zamin::{println, tr, vec};

zamin::entry!(main);

const PATH: &str = "~/katta.bin";
const PIECE: usize = 64 * 1024;

fn pattern(i: usize) -> u8 {
    (i.wrapping_mul(31) ^ (i >> 12)) as u8
}

fn verify() -> i64 {
    let t0 = zamin::uptime_ms();
    let Ok(mut f) = File::open(PATH, 0) else {
        println!("  {}: {}", PATH, tr(["topilmadi", "не найден", "not found"]));
        return 1;
    };
    let mut buf = vec![0u8; PIECE];
    let mut pos = 0usize;
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                println!("  {}: {}", tr(["öqiş xatosi", "ошибка чтения", "read error"]), e);
                return 2;
            }
        };
        for (k, &b) in buf[..n].iter().enumerate() {
            if b != pattern(pos + k) {
                println!("  {} {}", tr(["NOTÖĞRI bayt, joyi:", "НЕВЕРНЫЙ байт, позиция:", "WRONG byte at"]), pos + k);
                return 3;
            }
        }
        pos += n;
    }
    println!(
        "  {} {} MB ({} ms)",
        tr(["Tekşirildi, hammasi töğri:", "Проверено, всё верно:", "Verified, all correct:"]),
        pos >> 20,
        zamin::uptime_ms() - t0
    );
    0
}

fn main() -> i64 {
    let arg = zamin::args().trim();
    if arg == "tekşir" || arg == "tekshir" || arg == "verify" {
        return verify();
    }
    let mb: usize = arg.parse().unwrap_or(4).clamp(1, 64);
    let t0 = zamin::uptime_ms();
    let Ok(mut f) = File::create(PATH) else { return 1 };
    let mut buf = vec![0u8; PIECE];
    let mut pos = 0;
    while pos < mb << 20 {
        for (k, b) in buf.iter_mut().enumerate() {
            *b = pattern(pos + k);
        }
        if let Err(e) = f.write(&buf) {
            println!("  {}: {}", tr(["yoziş xatosi", "ошибка записи", "write error"]), e);
            return 2;
        }
        pos += PIECE;
    }
    drop(f);
    println!("  {} {} MB ({} ms)", tr(["Yozildi:", "Записано:", "Written:"]), mb, zamin::uptime_ms() - t0);
    verify()
}
