//! Terminal oynasi uçun sof yordamçi funksiyalar: aylantiriş tarixi (scrollback) çegarasi
//! va uzun qatorlarni oyna eniga qarab vizual qatorlarga öraş.

use alloc::string::String;
use alloc::vec::Vec;

/// Terminal tarixida saqlanadigan eng köp qator soni.
pub const MAX_LINES: usize = 2000;

/// Tarixni `MAX_LINES` gaça kesadi: eng eski qatorlar bitta `drain` bilan olib taşlanadi.
pub fn trim(lines: &mut Vec<String>) {
    if lines.len() > MAX_LINES {
        let extra = lines.len() - MAX_LINES;
        lines.drain(..extra);
    }
}

/// Bitta qatorni `cols` ustunli vizual qatorlarga böladi: (boşlanış, tugaş) bayt oraliqlari.
/// Imkon bölsa oxirgi böşliqdan (söz böyiça), aks holda belgi böyiça böladi.
/// Çegaralar doim UTF-8 belgi çegarasiga tuşadi. Böş qator — bitta böş vizual qator.
pub fn wrap_line(s: &str, cols: usize, out: &mut Vec<(usize, usize)>) {
    let cols = cols.max(1);
    let mut start = 0;
    let mut n = 0; // joriy vizual qatordagi belgilar soni
    let mut brk: Option<usize> = None; // joriy qatordagi oxirgi böşliqdan keyingi bayt
    for (i, c) in s.char_indices() {
        if n == cols {
            let end = match brk {
                Some(b) if b > start && b < i && c != ' ' => b,
                _ => i,
            };
            out.push((start, end));
            n = s[end..i].chars().count();
            start = end;
            brk = None;
        }
        n += 1;
        if c == ' ' {
            brk = Some(i + 1);
        }
    }
    out.push((start, s.len()));
}

/// Barça qatorlarni öraydi: (qator_idx, boşlanış, tugaş) — bayt oraliqlari.
pub fn wrap_lines<S: AsRef<str>>(lines: &[S], cols: usize) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::with_capacity(lines.len());
    let mut tmp = Vec::new();
    for (idx, l) in lines.iter().enumerate() {
        tmp.clear();
        wrap_line(l.as_ref(), cols, &mut tmp);
        out.extend(tmp.iter().map(|&(a, b)| (idx, a, b)));
    }
    out
}

/// Öz-özini tekşiriş: tarix çegarasi va öraş. Natija — sinovlar soni.
pub fn self_test() -> Result<usize, &'static str> {
    use alloc::format;
    let mut n = 0;
    // 1) 3000 qator qöşilganda 2000 qoladi (eng yangilari)
    let mut lines: Vec<String> = Vec::new();
    for k in 0..3000 {
        lines.push(format!("qator {}", k));
        if k % 97 == 0 {
            trim(&mut lines);
        }
    }
    trim(&mut lines);
    if lines.len() != MAX_LINES || lines[0] != "qator 1000" || lines[MAX_LINES - 1] != "qator 2999" {
        return Err("tarix çegarasi");
    }
    n += 1;
    // 2) 250 belgili qator (böşliqsiz) 80 ustunda 4 vizual qator
    let long: String = core::iter::repeat('a').take(250).collect();
    let w = wrap_lines(&[long.as_str()], 80);
    if w.len() != 4 || w[0] != (0, 0, 80) || w[3] != (0, 240, 250) {
        return Err("250 belgi / 80 ustun");
    }
    n += 1;
    // 3) UTF-8: özbek/rus harflari (2 baytli) — çegara belgi örtasiga tuşmaydi
    let uz: String = "ŞÇÖĞşçöğжЖщЩ".chars().cycle().take(250).collect();
    let w = wrap_lines(&[uz.as_str()], 80);
    if w.len() != 4 {
        return Err("UTF-8 qatorlar soni");
    }
    for &(_, a, b) in &w {
        if !uz.is_char_boundary(a) || !uz.is_char_boundary(b) || uz[a..b].chars().count() > 80 {
            return Err("UTF-8 çegarasi");
        }
    }
    if w.last().unwrap().2 != uz.len() {
        return Err("UTF-8 oxiri");
    }
    n += 1;
    // 4) söz böyiça öraş: söz örtasidan kesilmaydi
    let w = wrap_lines(&["salom dunyo ajoyib"], 8);
    let s = "salom dunyo ajoyib";
    let parts: Vec<&str> = w.iter().map(|&(_, a, b)| &s[a..b]).collect();
    if parts != ["salom ", "dunyo ", "ajoyib"] {
        return Err("söz böyiça öraş");
    }
    n += 1;
    // 5) böş qator — bitta vizual qator; qator indekslari
    let w = wrap_lines(&["", "ab"], 10);
    if w != [(0, 0, 0), (1, 0, 2)] {
        return Err("böş qator");
    }
    n += 1;
    Ok(n)
}
