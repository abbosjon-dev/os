//! Zamin terminali — yadro ichidagi oddiy buyruqlar qatori.

use crate::memory::{frame, heap};
use crate::{print, println, power, timer};
use alloc::string::String;

const PROMPT: &str = "zamin> ";
const MAX_LINE: usize = 256;

pub struct Shell {
    line: String,
}

impl Shell {
    pub fn new() -> Self {
        Self { line: String::new() }
    }

    pub fn start(&self) {
        println!("Zamin terminali. Buyruqlar ro'yxati uchun: yordam");
        print!("{}", PROMPT);
    }

    /// Klaviaturadan kelgan bitta baytni qayta ishlaydi.
    pub fn feed(&mut self, b: u8) {
        match b {
            b'\r' | b'\n' => {
                println!();
                let line = core::mem::take(&mut self.line);
                self.execute(line.trim());
                print!("{}", PROMPT);
            }
            0x7f | 0x08 => {
                if self.line.pop().is_some() {
                    print!("\x08 \x08");
                }
            }
            0x20..=0x7e if self.line.len() < MAX_LINE => {
                self.line.push(b as char);
                print!("{}", b as char);
            }
            _ => {} // hozircha boshqa belgilar (strelkalar va h.k.) e'tiborsiz
        }
    }

    fn execute(&mut self, line: &str) {
        let (cmd, args) = line.split_once(' ').unwrap_or((line, ""));
        match cmd {
            "" => {}
            "yordam" => {
                println!("  yordam   — shu ro'yxat");
                println!("  haqida   — tizim haqida");
                println!("  vaqt     — ishga tushganidan beri o'tgan vaqt");
                println!("  xotira   — xotira holati");
                println!("  aks      — matnni qaytaradi: aks Salom");
                println!("  tozala   — ekranni tozalash");
                println!("  qayta    — qayta yuklash");
                println!("  ochir    — tizimni o'chirish");
            }
            "haqida" => {
                println!("  Zamin OS v{} (AArch64)", env!("CARGO_PKG_VERSION"));
                println!("  Dasturchi: Abbosjon Dev");
                println!("  Veb-sayt:  zamin.app");
                println!("  Email:     info@zamin.app");
            }
            "vaqt" => {
                let ms = timer::uptime_ms();
                let s = ms / 1000;
                println!(
                    "  {} soat {} daqiqa {}.{:03} soniya (taymer uzilishlari: {})",
                    s / 3600,
                    (s / 60) % 60,
                    s % 60,
                    ms % 1000,
                    timer::ticks()
                );
            }
            "xotira" => {
                let (free, total) = frame::stats();
                let (used, htotal) = heap::stats();
                println!("  RAM:  {} MiB bo'sh / {} MiB jami", free * 4 / 1024, total * 4 / 1024);
                println!("  Heap: {} KiB ishlatilgan / {} KiB", used / 1024, htotal / 1024);
            }
            "aks" => println!("{}", args),
            "tozala" => print!("\x1b[2J\x1b[H"),
            "qayta" => {
                println!("Qayta yuklanmoqda...");
                power::reboot();
            }
            "ochir" => {
                println!("Tizim o'chirilmoqda. Xayr!");
                power::shutdown();
            }
            _ => println!("  noma'lum buyruq: {} (yordam deb yozing)", cmd),
        }
    }
}
