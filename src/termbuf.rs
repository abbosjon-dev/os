//! Terminal oynasi uçun sof yordamçi funksiyalar: aylantiriş tarixi (scrollback) çegarasi
//! va uzun qatorlarni oyna eniga qarab vizual qatorlarga öraş; ANSI SGR (ranglar) atributlari.

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

// --- ANSI SGR atributlari -------------------------------------------------------------

/// Katak atributi: `fg`/`bg` — 0 standart, 1..=16 — 16 rangli palitra indeksi + 1.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Attr {
    pub fg: u8,
    pub bg: u8,
    pub bold: bool,
}

/// Qator atributlari: (bayt ofseti, atribut) — şu ofsetdan keyingi run boşlanguniça amal qiladi.
/// Ofsetlar matn baytlariga bog'liq, şuning uçun öraş (wrap) va tarix kesilişi ularni buzmaydi.
pub type Runs = Vec<(usize, Attr)>;

/// 16 rangli palitra (töq fon 0x0e121a uçun moslangan): 0–7 oddiy, 8–15 yorqin.
pub const PALETTE: [u32; 16] = [
    0x3b4252, 0xf0616d, 0x5be37d, 0xf2c55c, 0x63a8ff, 0xc77dff, 0x4fd6d6, 0xd3dae5,
    0x6b7689, 0xff8a94, 0x8af5a4, 0xffe08a, 0x9cc8ff, 0xe0aaff, 0x8af0f0, 0xffffff,
];

/// `off` baytidagi atribut.
pub fn attr_at(runs: &[(usize, Attr)], off: usize) -> Attr {
    runs.iter().take_while(|r| r.0 <= off).last().map_or(Attr::default(), |r| r.1)
}

/// SGR parametrlari ("1;31", "", "0") ni atributga qöllaydi (sof funksiya).
/// Qöllanadi: 0, 1, 22, 30–37, 39, 40–47, 49, 90–97, 100–107; 38/48 (5;n / 2;r;g;b) — taşlab ketiladi.
pub fn apply_sgr(attr: &mut Attr, params: &str) {
    let mut it = params.split(';').map(|p| if p.is_empty() { Some(0u32) } else { p.parse::<u32>().ok() });
    while let Some(p) = it.next() {
        let Some(p) = p else { continue };
        match p {
            0 => *attr = Attr::default(),
            1 => attr.bold = true,
            22 => attr.bold = false,
            30..=37 => attr.fg = (p - 30 + 1) as u8,
            39 => attr.fg = 0,
            40..=47 => attr.bg = (p - 40 + 1) as u8,
            49 => attr.bg = 0,
            90..=97 => attr.fg = (p - 90 + 9) as u8,
            100..=107 => attr.bg = (p - 100 + 9) as u8,
            38 | 48 => match it.next() {
                Some(Some(5)) => {
                    it.next();
                }
                Some(Some(2)) => {
                    it.next();
                    it.next();
                    it.next();
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// CSI ketma-ketligi: ESC dan keyingi matn ("[1;31m") → (parametrlar, yakuniy belgi).
/// Xususiy (`?25l`) yoki notöğri parametrlar — `None` (eʼtiborsiz qoldiriladi).
pub fn parse_csi(seq: &str) -> Option<(&str, char)> {
    let body = seq.strip_prefix('[')?;
    let fin = body.chars().last()?;
    let params = &body[..body.len() - fin.len_utf8()];
    params.bytes().all(|b| b.is_ascii_digit() || b == b';').then_some((params, fin))
}

/// Terminal emulyatori holati: tugallanmagan ESC ketma-ketligi (keyingi öqişda davom etadi),
/// joriy atribut va joriy (tugamagan) qator atributlari.
#[derive(Default)]
pub struct Vt {
    pub esc: Option<String>,
    pub attr: Attr,
    pub runs: Runs,
}

/// Joriy qatorni yakunlaydi: matn va uning atributlari tarixga qöşiladi.
pub fn end_line(vt: &mut Vt, lines: &mut Vec<String>, attrs: &mut Vec<Runs>, cur: &mut Vec<u8>) {
    let line = String::from_utf8_lossy(&core::mem::take(cur)).into_owned();
    attrs.resize(lines.len(), Vec::new());
    let runs = core::mem::take(&mut vt.runs);
    attrs.push(if runs.iter().all(|r| r.1 == Attr::default()) { Vec::new() } else { runs });
    lines.push(line);
}

/// Foydalanuvçi kiritgan matnni (aks ettiriş) standart atribut bilan joriy qatorga qöşadi.
pub fn push_plain(vt: &mut Vt, cur: &mut Vec<u8>, bytes: &[u8]) {
    if !bytes.is_empty() && attr_at(&vt.runs, cur.len()) != Attr::default() {
        let n = cur.len();
        vt.runs.retain(|r| r.0 < n);
        vt.runs.push((n, Attr::default()));
    }
    cur.extend_from_slice(bytes);
}

/// Tarix çegarasi: matn va atributlar birga kesiladi. Qaytaradi — olib taşlangan qatorlar soni.
pub fn trim_attrs(lines: &mut Vec<String>, attrs: &mut Vec<Runs>) -> usize {
    let before = lines.len();
    trim(lines);
    let removed = before - lines.len();
    attrs.drain(..removed.min(attrs.len()));
    removed
}

/// Bayt oqimini qayta işlaydi: `\n`, `\r`, orqaga öçiriş, ESC/CSI (SGR ranglar, `J` tozalaş).
/// Noma'lum ketma-ketliklar eʼtiborsiz. Qaytaradi: ekran tozalandimi (`\x1b[2J`).
pub fn feed(vt: &mut Vt, lines: &mut Vec<String>, attrs: &mut Vec<Runs>, cur: &mut Vec<u8>, bytes: &[u8]) -> bool {
    let mut cleared = false;
    for &b in bytes {
        if let Some(esc) = vt.esc.as_mut() {
            esc.push(b as char);
            let done = match esc.as_bytes()[0] {
                b'[' => esc.len() >= 2 && (0x40..=0x7e).contains(&b),
                b']' => b == 0x07 || esc.ends_with("\u{1b}\\"),
                _ => true, // ikki baytli ESC (ESC 7, ESC c ...) — eʼtiborsiz
            };
            if done {
                let seq = vt.esc.take().unwrap();
                match parse_csi(&seq) {
                    Some((p, 'm')) => apply_sgr(&mut vt.attr, p),
                    Some((_, 'J')) => {
                        lines.clear();
                        attrs.clear();
                        cur.clear();
                        vt.runs.clear();
                        cleared = true;
                    }
                    // \x1b[K: \r allaqaçon qatorni böşatadi; kursor, \x1b[H va boşqalar — eʼtiborsiz
                    _ => {}
                }
            } else if esc.len() > 64 {
                vt.esc = None;
            }
            continue;
        }
        match b {
            0x1b => vt.esc = Some(String::new()),
            b'\n' => end_line(vt, lines, attrs, cur),
            b'\r' => {
                cur.clear();
                vt.runs.clear();
            }
            0x08 => {
                cur.pop();
                let n = cur.len();
                vt.runs.retain(|r| r.0 < n || r.0 == 0);
            }
            0..=0x1f => {}
            _ => {
                if attr_at(&vt.runs, cur.len()) != vt.attr {
                    let n = cur.len();
                    vt.runs.retain(|r| r.0 < n);
                    vt.runs.push((n, vt.attr));
                }
                cur.push(b);
            }
        }
    }
    cleared
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

/// SGR öz-özini tekşiriş: ranglar, bölingan ketma-ketlik, noma'lum CSI, tarix kesiliş.
pub fn self_test_sgr() -> Result<usize, &'static str> {
    let mut n = 0;
    // SGR: \x1b[1;31m, \x1b[0m, \x1b[92m, ikki öqişga bölingan \x1b[3 + 4m, noma'lum CSI
    let (mut vt, mut ls, mut at, mut cur) = (Vt::default(), Vec::new(), Vec::new(), Vec::new());
    feed(&mut vt, &mut ls, &mut at, &mut cur, "\x1b[1;31mXato\x1b[0m ok \x1b[92myaşil\x1b[3".as_bytes());
    if vt.esc.as_deref() != Some("[3") {
        return Err("SGR: bölingan ketma-ketlik saqlanmadi");
    }
    n += 1;
    feed(&mut vt, &mut ls, &mut at, &mut cur, "4mkök\x1b[5C\x1b[?25l\x1b[2Kz\x1b]0;nom\x07\x1b[0m.\n".as_bytes());
    let red = Attr { fg: 2, bg: 0, bold: true };
    let def = Attr::default();
    let (g, bl) = (Attr { fg: 11, ..def }, Attr { fg: 5, ..def });
    let l0 = ls.first().map_or("", |l: &String| l.as_str());
    if ls.len() != 1 || l0 != "Xato ok yaşilkökz." || vt.esc.is_some() {
        return Err("SGR: matn (ketma-ketliklar ekranga çiqmasligi kerak)");
    }
    let r = &at[0];
    let ka = l0.find("kök").unwrap();
    if attr_at(r, 0) != red || attr_at(r, 3) != red || attr_at(r, 4) != def || attr_at(r, 8) != g
        || attr_at(r, ka) != bl || attr_at(r, l0.find('z').unwrap()) != bl || attr_at(r, l0.len() - 1) != def
    {
        return Err("SGR: atributlar");
    }
    n += 1;
    // fon, 39/49, 22, 38;5;n
    let mut a = Attr::default();
    apply_sgr(&mut a, "1;44;97");
    apply_sgr(&mut a, "38;5;200;22");
    if a != (Attr { fg: 16, bg: 5, bold: false }) {
        return Err("SGR: fon / 38;5");
    }
    apply_sgr(&mut a, "39;49");
    if a != def {
        return Err("SGR: 39/49");
    }
    n += 1;
    // tarix kesilganda atributlar qatoriga mos qoladi; öraşda ofsetlar özgarmaydi
    for k in 0..MAX_LINES + 50 {
        let t = if k % 2 == 0 { alloc::format!("\x1b[34m{:03}\x1b[0m\n", k % 1000) } else { alloc::format!("{}\n", k) };
        feed(&mut vt, &mut ls, &mut at, &mut cur, t.as_bytes());
        trim_attrs(&mut ls, &mut at);
    }
    let even = |i: usize| ls[i].parse::<usize>().map_or(false, |v| v % 2 == 0);
    if ls.len() != MAX_LINES || at.len() != MAX_LINES
        || (0..MAX_LINES).any(|i| (attr_at(&at[i], 0) == bl) != even(i))
    {
        return Err("SGR: tarix kesiliş");
    }
    n += 1;
    // \x1b[2J — tozalaş
    if !feed(&mut vt, &mut ls, &mut at, &mut cur, b"\x1b[2J") || !ls.is_empty() || !at.is_empty() {
        return Err("SGR: 2J");
    }
    n += 1;
    Ok(n)
}
