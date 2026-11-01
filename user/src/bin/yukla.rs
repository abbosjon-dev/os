//! yukla — HTTP orqali sahifani yuklab, ekranga (yoki faylga) çiqaradi.
//!   yukla example.com [/yöl]        — ekranga
//!   yukla example.com /yöl > fayl   — faylga (terminal yönaltirişi)
#![no_std]
#![no_main]

use zamin::net::{self, Tcp};
use zamin::{String, format, println, tr};

zamin::entry!(main);

fn main() -> i64 {
    let mut it = zamin::args().trim().split_whitespace();
    let Some(host) = it.next() else {
        println!("{}", tr(["Foydalaniş: yukla <host> [/yöl]", "Использование: yukla <хост> [/путь]", "Usage: yukla <host> [/path]"]));
        return 2;
    };
    let path = it.next().unwrap_or("/");
    if !net::info().configured {
        println!("{}", tr(["Tarmoq sozlanmagan.", "Сеть не настроена.", "Network is not configured."]));
        return 1;
    }
    let ip = match net::resolve(host) {
        Ok(ip) => ip,
        Err(e) => {
            zamin::eprintln!("{}: {}", host, e);
            return 1;
        }
    };
    let Ok(conn) = Tcp::connect(ip, 80) else {
        zamin::eprintln!("{}", tr(["ulanib bölmadi", "не удалось подключиться", "could not connect"]));
        return 1;
    };
    let req = format!("GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n", path, host);
    if conn.send(req.as_bytes()).is_err() {
        return 1;
    }
    // Javobni oxirigaça öqiymiz
    let mut body = zamin::Vec::new();
    loop {
        let chunk = conn.recv(4096, 5000);
        if chunk.is_empty() {
            break;
        }
        body.extend_from_slice(&chunk);
    }
    // Sarlavhalardan keyin (bos qator) — faqat tanani çiqaramiz
    let text = String::from_utf8_lossy(&body);
    let out = text.split_once("\r\n\r\n").map_or(text.as_ref(), |(_, b)| b);
    zamin::write(out.as_bytes());
    0
}
