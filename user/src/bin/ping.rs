//! ping — manzilga 4 ta paket yuboradi. `ping 10.0.2.2` yoki `ping zamin.app`
#![no_std]
#![no_main]

use zamin::net;
use zamin::{println, tr};

zamin::entry!(main);

fn main() -> i64 {
    let host = zamin::args().trim();
    if host.is_empty() {
        println!("{}", tr(["Foydalaniş: ping <manzil>", "Использование: ping <адрес>", "Usage: ping <address>"]));
        return 2;
    }
    if !net::info().configured {
        println!("{}", tr(["Tarmoq sozlanmagan.", "Сеть не настроена.", "Network is not configured."]));
        return 1;
    }
    let ip = match net::resolve(host) {
        Ok(ip) => ip,
        Err(_) => {
            println!("{}: {}", tr(["topilmadi", "не найдено", "not found"]), host);
            return 1;
        }
    };
    println!("PING {} ({})", host, net::ip_string(ip));
    let mut ok = 0;
    for seq in 0..4 {
        match net::ping(ip, seq) {
            Ok(ms) => {
                println!("  {} seq={} {}={} ms", net::ip_string(ip), seq, tr(["vaqt", "время", "time"]), ms);
                ok += 1;
            }
            Err(_) => println!("  seq={} {}", seq, tr(["javob yöq", "нет ответа", "no reply"])),
        }
        zamin::sleep_ms(300);
    }
    println!("  {}/4 {}", ok, tr(["javob", "ответов", "replies"]));
    if ok > 0 { 0 } else { 1 }
}
