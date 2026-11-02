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
    let mut first = it.next().unwrap_or("");
    let https = first == "-s";
    if https {
        first = it.next().unwrap_or("");
    }
    if first.is_empty() {
        println!("{}", tr(["Foydalaniş: yukla [-s] <host> [/yöl]  (-s: HTTPS)", "Использование: yukla [-s] <хост> [/путь]", "Usage: yukla [-s] <host> [/path]"]));
        return 2;
    }
    let host = first;
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
    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: ZaminOS\r\n\r\n", path, host);
    let mut body = zamin::Vec::new();
    if https {
        let Ok(conn) = zamin::net::Tls::connect(ip, 443, host) else {
            zamin::eprintln!("{}", tr(["TLS ulanib bölmadi", "не удалось установить TLS", "TLS connect failed"]));
            return 1;
        };
        if conn.send(req.as_bytes()).is_err() {
            return 1;
        }
        loop {
            let chunk = conn.recv(8192);
            if chunk.is_empty() {
                break;
            }
            body.extend_from_slice(&chunk);
        }
    } else {
        let Ok(conn) = Tcp::connect(ip, 80) else {
            zamin::eprintln!("{}", tr(["ulanib bölmadi", "не удалось подключиться", "could not connect"]));
            return 1;
        };
        if conn.send(req.as_bytes()).is_err() {
            return 1;
        }
        loop {
            let chunk = conn.recv(4096, 5000);
            if chunk.is_empty() {
                break;
            }
            body.extend_from_slice(&chunk);
        }
    }
    // Sarlavhalardan keyin (bos qator) — faqat tanani çiqaramiz
    let text = String::from_utf8_lossy(&body);
    let out = text.split_once("\r\n\r\n").map_or(text.as_ref(), |(_, b)| b);
    zamin::write(out.as_bytes());
    0
}
