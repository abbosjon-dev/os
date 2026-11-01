//! Qator tahrirlovçisi: klaviaturadan bitta qatorni yiğadi.
//! UTF-8 (ö, ş, кириллица), öçiriş tugmasi va yaşirin rejim (parol uçun: `*`).

use crate::{print, println, task, uart};
use alloc::string::String;

const MAX_LINE: usize = 256;

pub struct LineEditor {
    line: String,
    utf8: [u8; 4],
    utf8_len: usize,
    utf8_need: usize,
    pub hidden: bool,
    /// Ctrl+D bosildi
    pub eof: bool,
}

impl LineEditor {
    pub const fn new() -> Self {
        Self { line: String::new(), utf8: [0; 4], utf8_len: 0, utf8_need: 0, hidden: false, eof: false }
    }

    /// Bitta baytni qabul qiladi. Enter bosilganda töliq qatorni qaytaradi.
    pub fn feed(&mut self, b: u8) -> Option<String> {
        if self.utf8_need > 0 {
            if b & 0xc0 == 0x80 {
                self.utf8[self.utf8_len] = b;
                self.utf8_len += 1;
                if self.utf8_len == self.utf8_need {
                    self.utf8_need = 0;
                    let (buf, len) = (self.utf8, self.utf8_len);
                    if let Ok(s) = core::str::from_utf8(&buf[..len]) {
                        self.insert(s);
                    }
                }
                return None;
            }
            self.utf8_need = 0; // buzilgan ketma-ketlik — taşlab yuboramiz
        }
        match b {
            b'\r' | b'\n' => {
                println!();
                return Some(core::mem::take(&mut self.line));
            }
            0x04 if self.line.is_empty() => {
                // Ctrl+D böş qatorda — kiriş oxiri
                println!();
                self.eof = true;
                return Some(String::new());
            }
            0x7f | 0x08 => {
                if self.line.pop().is_some() {
                    print!("\x08 \x08");
                }
            }
            0x20..=0x7e => {
                let buf = [b];
                self.insert(unsafe { core::str::from_utf8_unchecked(&buf) });
            }
            0xc0..=0xf7 => {
                self.utf8[0] = b;
                self.utf8_len = 1;
                self.utf8_need = match b {
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    _ => 4,
                };
            }
            _ => {} // boşqaruv belgilari (strelkalar va h.k.) hozirça eʼtiborsiz
        }
        None
    }

    fn insert(&mut self, s: &str) {
        if self.line.len() + s.len() <= MAX_LINE {
            self.line.push_str(s);
            print!("{}", if self.hidden { "*" } else { s });
        }
    }
}

/// Bitta qator yoki `None` — Ctrl+D (kiriş oxiri).
pub fn read_line_eof(hidden: bool) -> Option<String> {
    let mut ed = LineEditor::new();
    ed.hidden = hidden;
    loop {
        while let Some(b) = uart::read_byte() {
            if let Some(line) = ed.feed(b) {
                return if ed.eof { None } else { Some(line) };
            }
        }
        if task::current_killed() {
            return None;
        }
        task::wait_input(uart::has_input);
    }
}

/// Bitta qatorni öqiydi (kelgunça uxlaydi).
pub fn read_line(hidden: bool) -> String {
    let mut ed = LineEditor::new();
    ed.hidden = hidden;
    loop {
        while let Some(b) = uart::read_byte() {
            if let Some(line) = ed.feed(b) {
                return line;
            }
        }
        task::wait_input(uart::has_input);
    }
}
