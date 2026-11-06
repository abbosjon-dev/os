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

/// Manba matndagi örin: (qator indeksi, bayt ofseti). Ofset doim UTF-8 belgi çegarasida.
pub type Pos = (usize, usize);

/// Vizual örin (vizual qator, ustun — belgilarda) → manba örni.
/// Qator `rows` dan taşqarida bölsa — oxirgi qatorga, ustun qator uzunligiga keltiriladi.
pub fn vis_to_src<S: AsRef<str>>(lines: &[S], rows: &[(usize, usize, usize)], row: usize, col: usize) -> Pos {
    let Some(&(li, a, b)) = rows.get(row.min(rows.len().saturating_sub(1))) else {
        return (0, 0);
    };
    let s = lines.get(li).map_or("", |l| l.as_ref());
    let b = b.min(s.len());
    let off = s[a..b].char_indices().nth(col).map_or(b, |(i, _)| a + i);
    (li, off)
}

/// (langar, kursor) ni tartiblaydi: (boşi, oxiri).
pub fn order(a: Pos, b: Pos) -> (Pos, Pos) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Tanlangan matn: [boşi, oxiri) manba oraligʻi. Öralgan (wrap) qatorlar bitta qator
/// bölib qoladi — manba qatorlari orasiga `\n` qöyiladi.
pub fn range_text<S: AsRef<str>>(lines: &[S], a: Pos, b: Pos) -> String {
    let ((la, oa), (lb, ob)) = order(a, b);
    let mut out = String::new();
    for li in la..=lb.min(lines.len().saturating_sub(1)) {
        let s = lines[li].as_ref();
        let from = if li == la { oa.min(s.len()) } else { 0 };
        let to = if li == lb { ob.min(s.len()) } else { s.len() };
        if li > la {
            out.push('\n');
        }
        if from < to {
            out.push_str(&s[from..to]);
        }
    }
    out
}

/// Vizual qator (li, a, b) ning tanlovga tuşgan bayt oraligʻi (bölsa).
pub fn row_span(row: (usize, usize, usize), a: Pos, b: Pos) -> Option<(usize, usize)> {
    let ((la, oa), (lb, ob)) = order(a, b);
    let (li, ra, rb) = row;
    if li < la || li > lb {
        return None;
    }
    let from = if li == la { oa.max(ra) } else { ra };
    let to = if li == lb { ob.min(rb) } else { rb };
    (from < to).then_some((from, to))
}

/// Söz belgisi: harf, raqam va yöl/nom ichidagi belgilar.
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '~' | '\u{2bc}')
}

/// `off` baytidagi söz çegaralari (boşi, oxiri). Söz belgisi bölmasa — şu bitta belgi;
/// qator oxirida — böş oraliq.
pub fn word_at(s: &str, off: usize) -> (usize, usize) {
    let mut off = off.min(s.len());
    while !s.is_char_boundary(off) {
        off -= 1;
    }
    let Some(c) = s[off..].chars().next() else {
        return (off, off);
    };
    if !is_word(c) {
        return (off, off + c.len_utf8());
    }
    let start = s[..off].char_indices().rev().take_while(|&(_, c)| is_word(c)).last().map_or(off, |(i, _)| i);
    let end = s[off..].char_indices().find(|&(_, c)| !is_word(c)).map_or(s.len(), |(i, _)| off + i);
    (start, end)
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
    // 6) 80 ustunda öralgan 250 belgilik qatorda (vizual 1:10)–(3:5) tanlov
    let mix: String = "aŞbÇ→ğжЩ1ʼ".chars().cycle().take(250).collect();
    let src = [mix.as_str()];
    let w = wrap_lines(&src, 80);
    let chars: Vec<char> = mix.chars().collect();
    let want: String = chars[90..245].iter().collect();
    let a = vis_to_src(&src, &w, 1, 10);
    let b = vis_to_src(&src, &w, 3, 5);
    if !mix.is_char_boundary(a.1) || !mix.is_char_boundary(b.1) {
        return Err("tanlov: UTF-8 çegarasi");
    }
    // teskari yönalişda (pastdan yuqoriga sudraş) ham bir xil
    if range_text(&src, a, b) != want || range_text(&src, b, a) != want {
        return Err("tanlov (1:10)-(3:5)");
    }
    // vizual qatorlar bölaklari birga manba bölagini beradi, oraliqda `\n` yöq
    let mut joined = String::new();
    for &r in &w {
        if let Some((x, y)) = row_span(r, a, b) {
            joined.push_str(&mix[x..y]);
        }
    }
    if joined != want {
        return Err("tanlov: qator bölaklari");
    }
    n += 1;
    // 7) bir neça manba qatori: \n bilan; ustun qatordan uzun — qator oxiri
    let src2 = ["salom dunyo", "ikkinçi"];
    let w2 = wrap_lines(&src2, 80);
    let a = vis_to_src(&src2, &w2, 0, 6);
    let b = vis_to_src(&src2, &w2, 1, 99);
    if range_text(&src2, a, b) != "dunyo\nikkinçi" || b != (1, "ikkinçi".len()) {
        return Err("köp qatorli tanlov");
    }
    n += 1;
    // 8) söz çegarasi
    let t = "ls ~/Hujjatlar/reja.txt  oʼzbek, 42!";
    let wd = |k: usize| {
        let (x, y) = word_at(t, k);
        &t[x..y]
    };
    let ozb = t.find("oʼz").unwrap();
    if wd(5) != "~/Hujjatlar/reja.txt" || wd(0) != "ls" || wd(ozb + 2) != "oʼzbek" || wd(t.find(',').unwrap()) != "," || wd(t.len()) != "" {
        return Err("söz çegarasi");
    }
    // UTF-8 belgi örtasidagi ofset ham to'g'ri çegaraga tortiladi
    let (x, y) = word_at("çaqmoq", 1);
    if (x, y) != (0, "çaqmoq".len()) {
        return Err("söz çegarasi UTF-8");
    }
    n += 1;
    Ok(n)
}
