//! Brauzer xotirasini (localStorage, cookie) diskka saqlaş: `~/.zamin/<nom>`.
//!
//! Har bir ombor — (kalit1, kalit2, qiymat) uçliklar röyxati. Fayl formati —
//! satrlar, maydonlar TAB bilan ajratilgan; `\` , TAB va yangi satr qoçirilgan
//! (`\\`, `\t`, `\n`). Ombor joriy foydalanuvçiga bog'langan: foydalanuvçi
//! almaşsa (kiriş/çiqiş) — xotiradagi nusxa tozalanib, uning faylidan qayta öqiladi.
//! Tizimga hali heç kim kirmagan bölsa (`~` yöq) — faqat xotirada işlaydi.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::SpinLock;

pub type Entries = Vec<(String, String, String)>;

/// Bitta saqlanadigan ombor.
pub struct Persist {
    name: &'static str,
    /// Qaysi foydalanuvçi uçun öqilgan (`NONE` — hali öqilmagan).
    uid: AtomicU32,
}

const NONE: u32 = u32::MAX;
const DIR: &str = "~/.zamin";

impl Persist {
    pub const fn new(name: &'static str) -> Self {
        Persist { name, uid: AtomicU32::new(NONE) }
    }

    /// Joriy foydalanuvçi almaşgan bölsa — omborni uning faylidan qayta yuklaydi.
    /// Ombor qulfi uşlab turilmagan holda çaqiriladi.
    pub fn sync(&self, store: &SpinLock<Entries>) {
        let uid = crate::accounts::current_uid();
        if self.uid.load(Ordering::Acquire) == uid {
            return;
        }
        let loaded = if uid == 0 { Vec::new() } else { load(self.name) };
        *store.lock() = loaded;
        self.uid.store(uid, Ordering::Release);
    }

    /// Omborni diskka yozadi (foydalanuvçi kirgan bölsa). Qulf uşlanmagan holda.
    pub fn save(&self, store: &SpinLock<Entries>) {
        if crate::accounts::current_uid() == 0 {
            return;
        }
        let data = encode(&store.lock());
        let _ = write_file(self.name, &data);
    }
}

fn path(name: &str) -> String {
    alloc::format!("{}/{}", DIR, name)
}

fn write_file(name: &str, data: &[u8]) -> Result<(), crate::fs::FsError> {
    use crate::abi::{OPEN_CREATE, OPEN_TRUNCATE, OPEN_WRITE};
    if crate::fs::node_of(DIR).is_err() {
        crate::fs::mkdir(DIR)?;
    }
    let id = crate::fs::open(&path(name), OPEN_CREATE | OPEN_TRUNCATE | OPEN_WRITE)?;
    if !data.is_empty() {
        crate::fs::write(id, Some(0), data)?;
    }
    Ok(())
}

fn load(name: &str) -> Entries {
    crate::fs::read_all(&path(name)).map(|b| decode(&b)).unwrap_or_default()
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

pub fn encode(entries: &[(String, String, String)]) -> Vec<u8> {
    let mut s = String::new();
    for (a, b, c) in entries {
        escape(a, &mut s);
        s.push('\t');
        escape(b, &mut s);
        s.push('\t');
        escape(c, &mut s);
        s.push('\n');
    }
    s.into_bytes()
}

pub fn decode(bytes: &[u8]) -> Entries {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for line in text.split('\n') {
        let mut f = line.split('\t');
        if let (Some(a), Some(b), Some(c), None) = (f.next(), f.next(), f.next(), f.next()) {
            out.push((unescape(a), unescape(b), unescape(c)));
        }
    }
    out
}

/// Self-test: kodlaş/yeçiş aylanmasi (maxsus belgilar bilan).
pub fn self_test() -> Result<usize, &'static str> {
    let s = |x: &str| String::from(x);
    let src: Entries = alloc::vec![
        (s("https://zamin.app"), s("tashrif"), s("3")),
        (s("zamin"), s("a\tb"), s("q\\a\ny\r")),
        (s("misol.uz"), s("til"), s("öz ʼ")),
        (s("x"), s(""), s("")),
    ];
    let back = decode(&encode(&src));
    if back != src {
        return Err("encode/decode");
    }
    if !decode(b"buzuq satr\n\t\n").is_empty() {
        return Err("buzuq satr");
    }
    Ok(src.len())
}
