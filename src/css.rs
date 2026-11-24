//! CSS — noldan: uslublar jadvali tahlili (qoidalar, tanlagiçlar, e'lonlar),
//! ranglar, shrift ölçamlari va hisoblangan uslub (meros bilan).
//! Tanlagiçlar: teg, .klass, #id, *, :hover va avlod (bo'şliq) birikmasi;
//! @media (min/max-width, height, orientation) — kö'riş maydoniga qarab; boşqa @-qoidalar taşlanadi.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Hisoblangan uslub (element uçun).
#[derive(Clone, Copy, PartialEq)]
pub struct Sty {
    pub color: Option<u32>,
    pub bg: Option<u32>,
    pub size: f32, // CSS piksel (16 — oddiy)
    pub bold: bool,
    pub italic: bool,
    pub align: u8, // 0 — start (yönalişga qarab), 1 — markaz, 2 — öng, 3 — çap, 4 — end, 5 — justify
    pub rtl: bool, // direction: rtl (meros bölib ötadi)
    pub hidden: bool,
    pub underline: Option<bool>,
    pub strike: bool, // text-decoration: line-through
    pub block: Option<bool>, // display: block / inline
    // Quti modeli (meros bölib ötmaydi)
    pub mt: f32,                      // margin-top (CSS px)
    pub mb: f32,                      // margin-bottom
    pub pad: f32,                     // padding (bir xil)
    pub bw: f32,                      // border kengligi
    pub bc: Option<u32>,              // border rangi
    pub radius: [f32; 4],             // border-*-radius: TL, TR, BR, BL (CSS px; manfiy — foiz, -50 = 50%)
    pub maxw: Option<(f32, bool)>,    // width / max-width: (qiymat, foizmi)
    pub center: bool,                 // margin: auto — markazlaş
    // Joylaşuv (flex / grid)
    pub flex: u8,    // 0 — yöq, 1 — flex qator, 2 — flex ustun, 3 — grid
    pub gap: f32,
    pub gcols: u8,   // grid ustunlari soni
    pub justify: u8, // 0 — boşidan, 1 — markaz, 2 — oraliq (space-between), 3 — oxiridan
    pub grow: f32,   // flex: 1 (ustun kengligi ulushi)
    // Joylaşuv: position / float (meros bölib ötmaydi)
    pub pos: u8,                 // 0 — static, 1 — relative, 2 — absolute, 3 — fixed
    pub ins: [Option<f32>; 4],   // top, right, bottom, left (CSS px)
    pub float: u8,               // 0 — yöq, 1 — çap, 2 — öng
    pub clear: u8,               // 1 — left, 2 — right, 3 — both
    // Bezak (meros bölib ötmaydi)
    pub grad: Option<Grad>,      // background(-image): linear-gradient(...)
    pub shadow: Option<Shadow>,  // box-shadow
    // Balandlik (meros bölib ötmaydi): (qiymat, foizmi) — foiz kö'riş maydoni balandligidan
    pub h: Option<(f32, bool)>,     // height
    pub minh: Option<(f32, bool)>,  // min-height
    // Matn (meros bölib ötadi)
    pub lh: f32, // line-height: 0 — normal, > 0 — kö'paytuvçi (raqam), < 0 — -px (px/em/% hisoblangan)
    pub tt: u8,  // text-transform: 0 — none, 1 — uppercase, 2 — lowercase, 3 — capitalize
    pub ws: u8,  // white-space (meros): 0 — normal, 1 — nowrap, 2 — pre, 3 — pre-wrap, 4 — pre-line
    // Taşib çiqiş va şaffoflik (meros bölib ötmaydi)
    pub ovf: u8,       // overflow: 0 — visible, 1 — hidden (auto / scroll / clip ham şunday)
    pub ellip: bool,   // text-overflow: ellipsis
    pub opacity: f32,  // opacity: 0..=1
    // calc(): aralaş kenglik — foizli `maxw` ga qöşiladigan px bölagi (calc(50% - 20px) → (50, true) + -20)
    pub wpx: f32,
    // Taxminiy konteyner kontent eni (CSS px; 0 — kö'riş maydoni) — margin/padding/width foizlari uçun (meros)
    pub cw: f32,
    // display: inline-block (meros bölib ötmaydi) va vertical-align: 0 — baseline, 1 — middle, 2 — top, 3 — bottom
    pub ib: bool,
    pub valign: u8,
    // visibility: hidden / collapse — joy saqlanadi, özi çizilmaydi (meros bölib ötadi; visible bilan qaytadi)
    pub invis: bool,
}

/// Gradient rang töxtaşlari soni çegarasi.
pub const GRAD_MAX: usize = 6;

/// linear-gradient: burçak (CSS gradus: 0 — yuqoriga, 90 — öngga, 180 — pastga) va
/// rang töxtaşlari (rang, joy 0..=1000 — promille). `n` — haqiqiy töxtaşlar soni (2..=GRAD_MAX).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Grad {
    pub angle: u16,
    pub n: u8,
    pub stops: [(u32, u16); GRAD_MAX],
}

/// box-shadow: siljiş, xiralik, kengayiş (CSS px), rang va uning shaffofligi (0..255).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: u32,
    pub alpha: u8,
}

impl Sty {
    pub const fn root() -> Sty {
        Sty {
            color: None, bg: None, size: 16.0, bold: false, italic: false, align: 0, rtl: false, hidden: false, underline: None, strike: false, block: None,
            mt: 0.0, mb: 0.0, pad: 0.0, bw: 0.0, bc: None, radius: [0.0; 4], maxw: None, center: false,
            flex: 0, gap: 0.0, gcols: 0, justify: 0, grow: 0.0,
            pos: 0, ins: [None; 4], float: 0, clear: 0, grad: None, shadow: None,
            h: None, minh: None, lh: 0.0, tt: 0, ws: 0, ovf: 0, ellip: false, opacity: 1.0, wpx: 0.0, cw: 0.0, ib: false, valign: 0, invis: false,
        }
    }
    /// Meros: rang, ölçam, qalinlik, tekislaş, yaşiriş — bolaga ötadi; fon — ötmaydi.
    pub fn inherit(&self) -> Sty {
        Sty {
            bg: None, block: None, mt: 0.0, mb: 0.0, pad: 0.0, bw: 0.0, bc: None, radius: [0.0; 4], maxw: None, center: false,
            flex: 0, gap: 0.0, gcols: 0, justify: 0, grow: 0.0,
            pos: 0, ins: [None; 4], float: 0, clear: 0, grad: None, shadow: None, h: None, minh: None,
            ovf: 0, ellip: false, opacity: 1.0, wpx: 0.0, ib: false, valign: 0,
            ..*self
        }
    }
    /// Konteynermi (fon, çegara yoki kenglik çeklovi bor)?
    pub fn is_box(&self) -> bool {
        self.bg.is_some() || self.bw > 0.0 || self.maxw.is_some() || self.pad > 0.0 || self.grad.is_some() || self.shadow.is_some()
    }
}

#[derive(Clone, Default)]
pub struct Compound {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub comb: u8,     // keyingi (öngdagi) bilan birikma: 0 — avlod (bo'şliq), 1 — ">" ota, 2 — "+" darhol oldingi qöşni, 3 — "~" oldingi qöşnilardan biri
    pub hover: bool,  // :hover — siçqonça ostidagi element (yoki uning ajdodi)
    pub root: bool,   // :root — hujjat ildizi (html yoki eng yuqori element)
    pub attrs: Vec<AttrSel>,
    pub pseudo: Vec<Pseudo>,
}

/// [nom], [nom=q], [nom~=q], [nom^=q], [nom$=q], [nom*=q], [nom|=q] (i — registrsiz).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct AttrSel {
    pub name: String,
    pub op: u8, // 0 — bor, b'=', b'~', b'^', b'$', b'*', b'|'
    pub val: String,
    pub ci: bool,
}

/// Tuzilma psevdo-klasslari.
#[derive(Clone)]
pub enum Pseudo {
    FirstChild,
    LastChild,
    OnlyChild,
    NthChild(i32, i32),     // an+b
    NthLastChild(i32, i32),
    NthOfType(i32, i32),    // :first-of-type = (0,1)
    Not(Vec<Compound>),
}

#[derive(Clone)]
pub struct Rule {
    pub sel: Vec<Vec<Compound>>, // vergul bilan ajratilgan tanlagiçlar; har biri — avlod zanjiri
    pub decls: Vec<(String, String)>,
    pub media: Vec<Vec<Mq>>,     // @media şartlari: tașqi Vec — VA (ichma-ich), içki — YOKI (vergul)
    pub ua: bool,                // brauzerning öz uslubi (muallif qoidalaridan oldin, specificity'dan qatʼi nazar)
}

/// Bitta media so'rovi: "screen and (max-width: 600px)".
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Mq {
    pub never: bool, // print, nomaʼlum xususiyat va h.k.
    pub neg: bool,   // not …
    pub min_w: f32,
    pub max_w: f32,
    pub min_h: f32,
    pub max_h: f32,
}

// Kö'riş maydoni (viewport) ölçami, CSS pikselda (f32 bitlari) — GUI oyna eniga qarab yangilaydi.
static VIEW_W: AtomicU32 = AtomicU32::new(0x4480_0000); // 1024.0
static VIEW_H: AtomicU32 = AtomicU32::new(0x4440_0000); // 768.0

/// Media so'rovlari baholanadigan kö'riş maydoni (CSS px).
pub fn set_viewport(w: f32, h: f32) {
    VIEW_W.store(w.max(1.0).to_bits(), Ordering::Relaxed);
    VIEW_H.store(h.max(1.0).to_bits(), Ordering::Relaxed);
}

pub fn viewport() -> (f32, f32) {
    (f32::from_bits(VIEW_W.load(Ordering::Relaxed)), f32::from_bits(VIEW_H.load(Ordering::Relaxed)))
}

/// Media uzunligi: px, em/rem (16 px).
fn mq_len(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some(n) = v.strip_suffix("rem").or_else(|| v.strip_suffix("em")) {
        return parse_f(n.trim()).map(|x| x * 16.0);
    }
    parse_f(v.trim_end_matches("px").trim())
}

/// "(max-width: 600px)", "(width >= 40em)", "(600px < width)" — bitta xususiyat.
fn mq_feature(q: &mut Mq, f: &str) {
    let f = f.trim();
    if let Some((k, v)) = f.split_once(':') {
        let k = k.trim();
        let v = v.trim();
        match k {
            "min-width" | "max-width" | "min-height" | "max-height" | "width" | "height" => {
                let Some(x) = mq_len(v) else {
                    q.never = true;
                    return;
                };
                match k {
                    "min-width" => q.min_w = q.min_w.max(x),
                    "max-width" => q.max_w = q.max_w.min(x),
                    "min-height" => q.min_h = q.min_h.max(x),
                    "max-height" => q.max_h = q.max_h.min(x),
                    "width" => {
                        q.min_w = q.min_w.max(x);
                        q.max_w = q.max_w.min(x);
                    }
                    _ => {
                        q.min_h = q.min_h.max(x);
                        q.max_h = q.max_h.min(x);
                    }
                }
            }
            "orientation" => {
                let (w, h) = viewport();
                if (v == "portrait") != (h >= w) {
                    q.never = true;
                }
            }
            "prefers-color-scheme" => q.never |= v != "light",
            "prefers-reduced-motion" => q.never |= v != "no-preference",
            "hover" | "any-hover" => q.never |= v != "hover",
            "pointer" | "any-pointer" => q.never |= v != "fine",
            "color" | "min-color" => {}
            "min-resolution" | "max-resolution" | "-webkit-min-device-pixel-ratio" | "min-device-pixel-ratio" => {
                let r = v.trim_end_matches("dppx").trim_end_matches('x');
                let lim = if v.ends_with("dpi") { parse_f(v.trim_end_matches("dpi")).map(|d| d / 96.0) } else { parse_f(r) };
                match lim {
                    Some(l) if k.starts_with("max") => q.never |= l < 1.0,
                    Some(l) => q.never |= l > 1.0,
                    None => q.never = true,
                }
            }
            _ => q.never = true,
        }
        return;
    }
    // Diapazon sintaksisi: "width <= 600px", "400px < width < 800px"
    let mut toks: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in f.chars() {
        if c == '<' || c == '>' || c == '=' {
            if !cur.trim().is_empty() {
                toks.push(String::from(cur.trim()));
            }
            cur.clear();
            if let Some(l) = toks.last_mut().filter(|t| t.chars().all(|x| x == '<' || x == '>' || x == '=')) {
                l.push(c);
            } else {
                toks.push(String::from(c));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        toks.push(String::from(cur.trim()));
    }
    if toks.len() < 3 || toks.len() % 2 == 0 {
        // Yalang'oç xususiyat: "(color)", "(hover)" — bor; boşqasi nomaʼlum
        q.never |= !matches!(f, "color" | "hover" | "pointer" | "any-hover" | "any-pointer");
        return;
    }
    let mut i = 0;
    while i + 2 < toks.len() {
        let (a, op, b) = (toks[i].as_str(), toks[i + 1].as_str(), toks[i + 2].as_str());
        // feature op value  yoki  value op feature
        let (feat, val, op) = if a == "width" || a == "height" {
            (a, b, String::from(op))
        } else {
            let flip: String = op.chars().map(|c| if c == '<' { '>' } else if c == '>' { '<' } else { c }).collect();
            (b, a, flip)
        };
        let Some(x) = mq_len(val) else {
            q.never = true;
            return;
        };
        let eps = if op.contains('=') { 0.0 } else { 0.01 };
        let (mn, mx) = if feat == "width" { (&mut q.min_w, &mut q.max_w) } else if feat == "height" { (&mut q.min_h, &mut q.max_h) } else {
            q.never = true;
            return;
        };
        match op.as_str() {
            "<" | "<=" => *mx = mx.min(x - eps),
            ">" | ">=" => *mn = mn.max(x + eps),
            "=" => {
                *mn = mn.max(x);
                *mx = mx.min(x);
            }
            _ => q.never = true,
        }
        i += 2;
    }
}

/// "screen and (max-width: 600px), print" — media so'rovlari ro'yxati (YOKI).
pub fn parse_media(s: &str) -> Vec<Mq> {
    let low = s.to_ascii_lowercase();
    let mut out = Vec::new();
    for part in low.split(',') {
        let mut q = Mq { never: false, neg: false, min_w: 0.0, max_w: f32::INFINITY, min_h: 0.0, max_h: f32::INFINITY };
        let mut rest = part.trim();
        if rest.is_empty() {
            continue;
        }
        if let Some(r) = rest.strip_prefix("not ") {
            q.neg = true;
            rest = r.trim_start();
        } else if let Some(r) = rest.strip_prefix("only ") {
            rest = r.trim_start();
        }
        // Media turi (qavsdan oldingi söz)
        if !rest.starts_with('(') {
            let ty: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
            rest = rest[ty.len()..].trim_start();
            if !matches!(ty.as_str(), "screen" | "all") {
                q.never = true;
            }
        }
        // Xususiyatlar: (…) and (…)
        while let Some(o) = rest.find('(') {
            let Some(c) = rest[o..].find(')').map(|j| o + j) else { break };
            mq_feature(&mut q, &rest[o + 1..c]);
            rest = &rest[c + 1..];
        }
        if rest.contains(" or ") {
            q.never = true; // murakkab mantiq — qöllanmaydi
        }
        out.push(q);
    }
    out
}

fn mq_ok(q: &Mq, w: f32, h: f32) -> bool {
    let hit = !q.never && w >= q.min_w && w <= q.max_w && h >= q.min_h && h <= q.max_h;
    hit != q.neg
}

/// Qoidaning @media şartlari joriy kö'riş maydonida bajariladimi?
pub fn media_ok(media: &[Vec<Mq>]) -> bool {
    if media.is_empty() {
        return true;
    }
    let (w, h) = viewport();
    media.iter().all(|alt| alt.iter().any(|q| mq_ok(q, w, h)))
}

/// Har bir şartli (@media) qoidaning joriy holati — ölçam özgarganda qayta tahlil kerakligini aniqlaş uçun.
pub fn media_state(rules: &[Rule]) -> Vec<bool> {
    rules.iter().filter(|r| !r.media.is_empty()).map(|r| media_ok(&r.media)).collect()
}

/// Elementga :hover li qoida tegişli bölişi mumkinmi (oxirgi bölagi mos keladi)?
pub fn hover_hits(rules: &[Rule], tag: &str, id: &str, classes: &[String]) -> bool {
    // tuzilma / atribut şartlari bu yerda e'tiborsiz — aniq natijani qayta uslublaş beradi
    let me = ElRef { tag, id, classes, hover: true, ..ElRef::default() };
    rules.iter().any(|r| {
        r.sel.iter().any(|ch| ch.iter().any(|c| c.hover) && ch.last().is_some_and(|l| simple_match(l, &me, false)))
    })
}

/// Qoidada :hover bormi (va u oxirgi bölakdami)?
pub fn hover_kind(r: &Rule) -> (bool, bool) {
    let mut last = false;
    let mut inner = false;
    for ch in &r.sel {
        for (k, c) in ch.iter().enumerate() {
            if c.hover {
                if k + 1 == ch.len() {
                    last = true;
                } else {
                    inner = true;
                }
            }
        }
    }
    (last, inner)
}

fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = match rest[i + 2..].find("*/") {
            Some(j) => &rest[i + 2 + j + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// CSS nomi (identifikator) oxiri: harf, raqam, '-', '_' va ASCII bölmagan belgilar.
fn ident_end(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'_' || b[i] >= 0x80) {
        i += 1;
    }
    i
}

/// `open` dagi '(' yoki '[' ning juftini topadi (qötirnoq içi hisobga olinmaydi).
fn sel_close(b: &[u8], open: usize) -> Option<usize> {
    let (o, c) = if b[open] == b'(' { (b'(', b')') } else { (b'[', b']') };
    let (mut depth, mut q, mut i) = (0i32, 0u8, open);
    while i < b.len() {
        let ch = b[i];
        if q != 0 {
            if ch == b'\\' {
                i += 1;
            } else if ch == q {
                q = 0;
            }
        } else if ch == b'"' || ch == b'\'' {
            q = ch;
        } else if ch == o {
            depth += 1;
        } else if ch == c {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Yuqori darajadagi `sep` böyiça bölaş (qavslar, [ ] va qötirnoqlar içida emas).
fn split_sel(s: &str, sep: u8) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let (mut st, mut i) = (0usize, 0usize);
    while i < b.len() {
        match b[i] {
            b'(' | b'[' => {
                i = sel_close(b, i).map(|e| e + 1).unwrap_or(b.len());
                continue;
            }
            b'"' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
                continue;
            }
            c if c == sep => {
                out.push(&s[st..i]);
                st = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&s[st.min(s.len())..]);
    out
}

/// "ul > li + li a" → [("ul", >), ("li", +), ("li", bo'şliq), ("a", 0)]: bölak va undan keyingi birikma.
fn split_chain(s: &str) -> Option<Vec<(&str, u8)>> {
    let b = s.as_bytes();
    let mut out: Vec<(&str, u8)> = Vec::new();
    let mut pending: Option<u8> = None;
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if ch.is_ascii_whitespace() {
            if pending.is_none() && !out.is_empty() {
                pending = Some(0);
            }
            i += 1;
            continue;
        }
        if matches!(ch, b'>' | b'+' | b'~') {
            if out.is_empty() || matches!(pending, Some(x) if x != 0) {
                return None;
            }
            pending = Some(match ch {
                b'>' => 1,
                b'+' => 2,
                _ => 3,
            });
            i += 1;
            continue;
        }
        let st = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'>' | b'+' | b'~') {
            match b[i] {
                b'(' | b'[' => i = sel_close(b, i)? + 1,
                b'"' | b'\'' | b'\\' | b'{' | b'}' => return None,
                _ => i += 1,
            }
        }
        if let Some(last) = out.last_mut() {
            last.1 = pending.take().unwrap_or(0);
        }
        out.push((&s[st..i], 0));
    }
    if matches!(pending, Some(x) if x != 0) || out.is_empty() {
        return None;
    }
    Some(out)
}

/// "[type=text i]" içi → atribut şarti.
fn parse_attr(s: &str) -> Option<AttrSel> {
    let s = s.trim();
    let e = ident_end(s.as_bytes(), 0);
    if e == 0 {
        return None;
    }
    let name = s[..e].to_ascii_lowercase();
    let rest = s[e..].trim_start();
    if rest.is_empty() {
        return Some(AttrSel { name, ..AttrSel::default() });
    }
    let rb = rest.as_bytes();
    let (op, skip) = match rb[0] {
        b'=' => (b'=', 1),
        c @ (b'~' | b'^' | b'$' | b'*' | b'|') if rb.get(1) == Some(&b'=') => (c, 2),
        _ => return None,
    };
    let v = rest[skip..].trim_start();
    let (val, tail) = match v.as_bytes().first() {
        Some(&q) if q == b'"' || q == b'\'' => {
            let body = &v[1..];
            let end = body.find(q as char)?;
            (String::from(&body[..end]), body[end + 1..].trim())
        }
        _ => {
            let e = ident_end(v.as_bytes(), 0);
            if e == 0 {
                return None;
            }
            (String::from(&v[..e]), v[e..].trim())
        }
    };
    let ci = match tail {
        "" | "s" | "S" => false,
        "i" | "I" => true,
        _ => return None,
    };
    Some(AttrSel { name, op, val: if ci { val.to_lowercase() } else { val }, ci })
}

/// an+b: "odd", "even", "2n+1", "-n + 3", "n", "4".
fn parse_nth(s: &str) -> Option<(i32, i32)> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    match t.as_str() {
        "odd" => return Some((2, 1)),
        "even" => return Some((2, 0)),
        _ => {}
    }
    let num = |x: &str| -> Option<i32> {
        if x.is_empty() || x.len() > 6 || !x.bytes().all(|c| c.is_ascii_digit()) {
            None
        } else {
            x.parse().ok()
        }
    };
    let signed = |x: &str| -> Option<i32> {
        match x.strip_prefix('-') {
            Some(d) => num(d).map(|v| -v),
            None => num(x.strip_prefix('+').unwrap_or(x)),
        }
    };
    match t.find('n') {
        Some(np) => {
            let a = match &t[..np] {
                "" | "+" => 1,
                "-" => -1,
                x => signed(x)?,
            };
            let r = &t[np + 1..];
            let b = if r.is_empty() {
                0
            } else if r.starts_with('+') || r.starts_with('-') {
                signed(r)?
            } else {
                return None;
            };
            Some((a, b))
        }
        None => Some((0, signed(&t)?)),
    }
}

/// `idx` (1 dan) an+b ko'rinişida (n ≥ 0) ifodalanadimi?
fn nth_ok(a: i32, b: i32, idx: i32) -> bool {
    if a == 0 {
        idx == b
    } else {
        let d = idx - b;
        d % a == 0 && d / a >= 0
    }
}

fn parse_compound(s: &str, depth: u8) -> Option<Compound> {
    if depth > 4 || s.is_empty() {
        return None;
    }
    let mut c = Compound::default();
    let b = s.as_bytes();
    let mut i = 0;
    let ident = |i: &mut usize| -> Option<String> {
        let e = ident_end(b, *i);
        if e == *i {
            return None;
        }
        let r = String::from(&s[*i..e]);
        *i = e;
        Some(r)
    };
    while i < b.len() {
        match b[i] {
            b'.' => {
                i += 1;
                c.classes.push(ident(&mut i)?);
            }
            b'#' => {
                i += 1;
                c.id = Some(ident(&mut i)?);
            }
            b'*' => i += 1,
            b'[' => {
                let e = sel_close(b, i)?;
                c.attrs.push(parse_attr(&s[i + 1..e])?);
                i = e + 1;
            }
            b':' => {
                // Holatga bogʻliq / nomaʼlum psevdo-klass (:focus, ::before …) — faqat şu tanlagiç taşlanadi
                i += 1;
                let name = ident(&mut i)?.to_ascii_lowercase();
                let arg = if b.get(i) == Some(&b'(') {
                    let e = sel_close(b, i)?;
                    let a = &s[i + 1..e];
                    i = e + 1;
                    Some(a)
                } else {
                    None
                };
                match (name.as_str(), arg) {
                    ("root", None) => c.root = true,
                    ("hover", None) => c.hover = true,
                    ("link" | "visited" | "any-link", None) => {}
                    ("first-child", None) => c.pseudo.push(Pseudo::FirstChild),
                    ("last-child", None) => c.pseudo.push(Pseudo::LastChild),
                    ("only-child", None) => c.pseudo.push(Pseudo::OnlyChild),
                    ("first-of-type", None) => c.pseudo.push(Pseudo::NthOfType(0, 1)),
                    ("nth-child", Some(a)) => {
                        let (x, y) = parse_nth(a)?;
                        c.pseudo.push(Pseudo::NthChild(x, y));
                    }
                    ("nth-last-child", Some(a)) => {
                        let (x, y) = parse_nth(a)?;
                        c.pseudo.push(Pseudo::NthLastChild(x, y));
                    }
                    ("nth-of-type", Some(a)) => {
                        let (x, y) = parse_nth(a)?;
                        c.pseudo.push(Pseudo::NthOfType(x, y));
                    }
                    ("not", Some(a)) => {
                        let mut inner = Vec::new();
                        for part in split_sel(a, b',') {
                            let toks = split_chain(part.trim())?;
                            if toks.len() != 1 {
                                return None; // :not() içida birikmalar qöllanmaydi
                            }
                            inner.push(parse_compound(toks[0].0, depth + 1)?);
                        }
                        c.pseudo.push(Pseudo::Not(inner));
                    }
                    _ => return None,
                }
            }
            _ => {
                let t = ident(&mut i)?;
                c.tag = Some(t.to_ascii_lowercase());
            }
        }
    }
    Some(c)
}

/// "a, b c, ul > li + li" — tanlagiçlar ro'yxati. Noto'g'ri tanlagiç faqat özi taşlanadi.
fn parse_selectors(s: &str) -> Vec<Vec<Compound>> {
    let mut out = Vec::new();
    for part in split_sel(s, b',') {
        let Some(toks) = split_chain(part.trim()) else { continue };
        if toks.len() > 32 {
            continue;
        }
        let mut ch = Vec::new();
        for (t, comb) in toks {
            match parse_compound(t, 0) {
                Some(mut c) => {
                    c.comb = comb;
                    ch.push(c);
                }
                None => {
                    ch.clear();
                    break;
                }
            }
        }
        if !ch.is_empty() {
            out.push(ch);
        }
    }
    out
}

/// "color: red; font-size: 12px" — e'lonlar.
pub fn parse_decls(s: &str) -> Vec<(String, String)> {
    s.split(';')
        .filter_map(|d| d.split_once(':'))
        .map(|(k, v)| (String::from(k.trim()).to_ascii_lowercase(), String::from(v.trim().trim_end_matches("!important").trim())))
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect()
}

/// Uslublar jadvali. @media bloklari ichidagi qoidalar şart bilan saqlanadi
/// (kö'riş maydoni eniga qarab `compute` da baholanadi); boşqa @-qoidalar taşlanadi.
pub fn parse_sheet(src: &str) -> Vec<Rule> {
    let src = strip_comments(src);
    let mut rules = Vec::new();
    parse_block(&src, &mut rules, 0);
    rules
}

fn parse_block(src: &str, rules: &mut Vec<Rule>, depth: u8) {
    let mut rest = src;
    loop {
        let rest_t = rest.trim_start();
        if rest_t.is_empty() {
            break;
        }
        if rest_t.starts_with('}') {
            rest = &rest_t[1..];
            continue;
        }
        if rest_t.starts_with('@') {
            // @import ...; yoki @media {...} (ichma-ich qavslar bilan)
            let semi = rest_t.find(';');
            let brace = rest_t.find('{');
            match (semi, brace) {
                (Some(s), Some(b)) if s < b => {
                    rest = &rest_t[s + 1..];
                    continue;
                }
                (_, Some(b)) => {
                    let mut depth_b = 0;
                    let mut end = rest_t.len();
                    for (k, ch) in rest_t[b..].char_indices() {
                        if ch == '{' {
                            depth_b += 1;
                        } else if ch == '}' {
                            depth_b -= 1;
                            if depth_b == 0 {
                                end = b + k + 1;
                                break;
                            }
                        }
                    }
                    let head = &rest_t[1..b];
                    let name: String = head.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>().to_ascii_lowercase();
                    if name == "media" && depth < 4 {
                        let cond = parse_media(&head[name.len()..]);
                        let inner_end = if end > b + 1 && rest_t.as_bytes()[end - 1] == b'}' { end - 1 } else { end };
                        let first = rules.len();
                        parse_block(&rest_t[b + 1..inner_end], rules, depth + 1);
                        for r in &mut rules[first..] {
                            r.media.insert(0, cond.clone());
                        }
                    }
                    rest = &rest_t[end..];
                    continue;
                }
                (Some(s), None) => {
                    rest = &rest_t[s + 1..];
                    continue;
                }
                _ => break,
            }
        }
        let Some(ob) = rest_t.find('{') else { break };
        let Some(cb) = rest_t[ob..].find('}').map(|j| ob + j) else { break };
        let sel = parse_selectors(&rest_t[..ob]);
        let decls = parse_decls(&rest_t[ob + 1..cb]);
        if !sel.is_empty() && !decls.is_empty() {
            rules.push(Rule { sel, decls, media: Vec::new(), ua: false });
        }
        rest = &rest_t[cb + 1..];
    }
}

/// Element: (teg, id, klasslar, atributlar, aka-ukalar orasidagi örni) — tanlagiçga moslaş uçun.
#[derive(Clone, Copy)]
pub struct ElRef<'a> {
    pub tag: &'a str,
    pub id: &'a str,
    pub classes: &'a [String],
    pub hover: bool, // siçqonça şu element (yoki uning avlodi) ustidami
    pub attrs: &'a [(String, String)], // atributlar (nomi kiçik harfda); class / id bölmasa ham ular hisobga olinadi
    pub nth: u32,   // aka-ukalar (element) orasidagi tartib raqami, 1 dan; 0 — nomaʼlum
    pub after: u32, // keyingi aka-ukalar soni; u32::MAX — nomaʼlum
    pub prev: &'a [ElRef<'a>], // oldingi aka-ukalar (eng yaqini — oxirida); to'liq bölmasligi mumkin
}

impl Default for ElRef<'_> {
    fn default() -> Self {
        ElRef { tag: "", id: "", classes: &[], hover: false, attrs: &[], nth: 0, after: u32::MAX, prev: &[] }
    }
}

/// Teg / id / klass / :hover / :root — tuzilma va atributlarsiz.
fn simple_match(c: &Compound, e: &ElRef, top: bool) -> bool {
    (!c.hover || e.hover)
        && (!c.root || top || e.tag == "html")
        && c.tag.as_ref().is_none_or(|t| t == e.tag)
        && c.id.as_ref().is_none_or(|i| i == e.id)
        && c.classes.iter().all(|k| e.classes.iter().any(|x| x == k))
}

fn attr_ok(a: &AttrSel, e: &ElRef) -> bool {
    let joined;
    let v: &str = match e.attrs.iter().find(|(k, _)| *k == a.name) {
        Some((_, v)) => v,
        None if a.name == "id" && !e.id.is_empty() => e.id,
        None if a.name == "class" && !e.classes.is_empty() => {
            joined = e.classes.join(" ");
            &joined
        }
        None => return false,
    };
    let low;
    let v = if a.ci {
        low = v.to_lowercase();
        low.as_str()
    } else {
        v
    };
    let q = a.val.as_str();
    match a.op {
        0 => true,
        b'=' => v == q,
        b'~' => !q.is_empty() && v.split_whitespace().any(|w| w == q),
        b'^' => !q.is_empty() && v.starts_with(q),
        b'$' => !q.is_empty() && v.ends_with(q),
        b'*' => !q.is_empty() && v.contains(q),
        b'|' => v == q || v.strip_prefix(q).is_some_and(|r| r.starts_with('-')),
        _ => false,
    }
}

fn pseudo_ok(p: &Pseudo, e: &ElRef, top: bool) -> bool {
    let last = e.after != u32::MAX && e.after == 0;
    match p {
        Pseudo::FirstChild => e.nth == 1,
        Pseudo::LastChild => last,
        Pseudo::OnlyChild => e.nth == 1 && last,
        Pseudo::NthChild(a, b) => e.nth > 0 && nth_ok(*a, *b, e.nth as i32),
        Pseudo::NthLastChild(a, b) => e.after != u32::MAX && nth_ok(*a, *b, e.after as i32 + 1),
        Pseudo::NthOfType(a, b) => {
            // oldingi aka-ukalar ro'yxati to'liq bölsagina
            e.nth > 0 && e.prev.len() + 1 == e.nth as usize && nth_ok(*a, *b, 1 + e.prev.iter().filter(|s| s.tag == e.tag).count() as i32)
        }
        Pseudo::Not(list) => !list.iter().any(|x| compound_match(x, e, top)),
    }
}

fn compound_match(c: &Compound, e: &ElRef, top: bool) -> bool {
    simple_match(c, e, top) && c.attrs.iter().all(|a| attr_ok(a, e)) && c.pseudo.iter().all(|p| pseudo_ok(p, e, top))
}

/// `e.prev[j]` — örni (nth, after) va öz oldingi aka-ukalari bilan.
fn sib<'a>(e: &ElRef<'a>, j: usize) -> ElRef<'a> {
    let mut s = e.prev[j];
    let back = (e.prev.len() - j) as u32;
    s.nth = if e.nth > back { e.nth - back } else { 0 };
    s.after = if e.after == u32::MAX { u32::MAX } else { e.after + back };
    s.prev = &e.prev[..j];
    s
}

/// `chain[ci]` `e` ga mos keldi (`anc` — uning ajdodlari, ota oxirida); chapdagi bölaklar orqaga qaytiş bilan tekşiriladi.
fn match_left(chain: &[Compound], ci: usize, e: ElRef, anc: &[ElRef], budget: &mut u32) -> bool {
    if ci == 0 {
        return true;
    }
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let c = &chain[ci - 1];
    match c.comb {
        1 => {
            let Some((p, up)) = anc.split_last() else { return false };
            compound_match(c, p, up.is_empty()) && match_left(chain, ci - 1, *p, up, budget)
        }
        2 | 3 => {
            for j in (0..e.prev.len()).rev() {
                let s = sib(&e, j);
                if compound_match(c, &s, false) && match_left(chain, ci - 1, s, anc, budget) {
                    return true;
                }
                if c.comb == 2 {
                    break;
                }
            }
            false
        }
        _ => {
            for k in (0..anc.len()).rev() {
                if compound_match(c, &anc[k], k == 0) && match_left(chain, ci - 1, anc[k], &anc[..k], budget) {
                    return true;
                }
            }
            false
        }
    }
}

/// Tanlagiç zanjiri: oxirgisi — elementning özi; oldingilari birikma turiga qarab
/// ajdodlarda (bo'şliq, >) yoki oldingi aka-ukalarda (+, ~). Moslaş öngdan çapga.
pub fn matches(chain: &[Compound], el: &ElRef, ancestors: &[ElRef]) -> bool {
    let Some(last) = chain.last() else { return false };
    if !compound_match(last, el, ancestors.is_empty()) {
        return false;
    }
    let mut budget = 4096;
    match_left(chain, chain.len() - 1, *el, ancestors, &mut budget)
}

fn spec_c(c: &Compound) -> u32 {
    let ids = c.id.is_some() as u32;
    let cls = (c.classes.len() + c.attrs.len()) as u32 + c.hover as u32 + c.root as u32;
    let mut s = (ids << 20) + (cls << 10) + c.tag.is_some() as u32;
    for p in &c.pseudo {
        s += match p {
            Pseudo::Not(l) => l.iter().map(spec_c).max().unwrap_or(0),
            _ => 1 << 10,
        };
    }
    s
}

/// Specificity (a, b, c) bir sonda: a·2²⁰ + b·2¹⁰ + c. :not(X) — X ning specificity'si.
pub fn specificity(chain: &[Compound]) -> u32 {
    chain.iter().map(spec_c).sum()
}

/// Qoidalar qanday kontekst talab qiladi: (atributlar, oldingi aka-ukalar ro'yxati, keyingi aka-ukalar soni).
pub fn needs(rules: &[Rule]) -> (bool, bool, bool) {
    fn walk(c: &Compound, n: &mut (bool, bool, bool)) {
        n.0 |= !c.attrs.is_empty();
        n.1 |= c.comb == 2 || c.comb == 3;
        for p in &c.pseudo {
            match p {
                Pseudo::NthOfType(..) => n.1 = true,
                Pseudo::LastChild | Pseudo::OnlyChild | Pseudo::NthLastChild(..) => n.2 = true,
                Pseudo::Not(l) => l.iter().for_each(|x| walk(x, n)),
                _ => {}
            }
        }
    }
    let mut n = (false, false, false);
    for r in rules {
        for ch in &r.sel {
            ch.iter().for_each(|c| walk(c, &mut n));
        }
    }
    n
}

const NAMED: [(&str, u32); 30] = [
    ("black", 0x000000), ("white", 0xffffff), ("red", 0xff0000), ("green", 0x008000), ("blue", 0x0000ff),
    ("yellow", 0xffff00), ("orange", 0xffa500), ("purple", 0x800080), ("gray", 0x808080), ("grey", 0x808080),
    ("silver", 0xc0c0c0), ("maroon", 0x800000), ("navy", 0x000080), ("teal", 0x008080), ("lime", 0x00ff00),
    ("aqua", 0x00ffff), ("cyan", 0x00ffff), ("fuchsia", 0xff00ff), ("magenta", 0xff00ff), ("olive", 0x808000),
    ("pink", 0xffc0cb), ("brown", 0xa52a2a), ("gold", 0xffd700), ("crimson", 0xdc143c), ("tomato", 0xff6347),
    ("coral", 0xff7f50), ("indigo", 0x4b0082), ("violet", 0xee82ee), ("darkgray", 0xa9a9a9), ("lightgray", 0xd3d3d3),
];

/// Rang: #rgb, #rrggbb, rgb(r,g,b), rgba(...), nom bilan.
/// Qavslarni hisobga olib, yuqori darajadagi `sep` belgisi böyiça bölaydi.
fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if depth == 0 && (c == sep || sep == ' ' && c.is_whitespace()) => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out.into_iter().map(|x| x.trim()).filter(|x| !x.is_empty()).collect()
}

/// Rang + shaffoflik (rgba/hsla 4-qiymat, #rrggbbaa, #rgba, transparent).
pub fn parse_color_a(v: &str) -> Option<(u32, u8)> {
    let v = v.trim();
    let low = v.to_ascii_lowercase();
    if low == "transparent" {
        return Some((0, 0));
    }
    let c = parse_color(v)?;
    let mut a = 255u8;
    if let Some(h) = low.strip_prefix('#') {
        let h: String = h.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if h.len() == 8 {
            a = u8::from_str_radix(&h[6..8], 16).unwrap_or(255);
        } else if h.len() == 4 {
            a = (h[3..4].chars().next().and_then(|c| c.to_digit(16)).unwrap_or(15) * 17) as u8;
        }
    } else if let Some(r) = low.strip_prefix("rgba(").or_else(|| low.strip_prefix("rgb(")) {
        let parts: Vec<&str> = r.trim_end_matches(')').split([',', ' ', '/']).filter(|x| !x.is_empty()).collect();
        if let Some(x) = parts.get(3) {
            let f = if let Some(p) = x.strip_suffix('%') { parse_f(p).map(|f| f / 100.0) } else { parse_f(x) };
            a = (f.unwrap_or(1.0).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    Some((c, a))
}

/// `linear-gradient(to bottom|to right|<N>deg|<N>turn, rang [joy%], ...)` — kamida 2 rang.
/// Yaroqsiz (yopilmagan qavs, rang yöq, nomaʼlum yönaliş) — None (panic qilmaydi).
pub fn parse_gradient(v: &str) -> Option<Grad> {
    let low = v.trim().to_ascii_lowercase();
    let i = low.find("linear-gradient(")?;
    let body = &low[i + "linear-gradient(".len()..];
    // mos yopuvçi qavs
    let mut depth = 1i32;
    let mut end = None;
    for (k, c) in body.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(k);
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &body[..end?];
    let mut args = split_top(body, ',');
    if args.is_empty() {
        return None;
    }
    let mut angle = 180u16;
    let first = args[0];
    if let Some(dir) = first.strip_prefix("to ") {
        let (mut x, mut y) = (0i32, 0i32);
        for w in dir.split_whitespace() {
            match w {
                "top" => y = -1,
                "bottom" => y = 1,
                "left" => x = -1,
                "right" => x = 1,
                _ => return None,
            }
        }
        angle = match (x, y) {
            (0, -1) => 0,
            (1, -1) => 45,
            (1, 0) => 90,
            (1, 1) => 135,
            (0, 1) => 180,
            (-1, 1) => 225,
            (-1, 0) => 270,
            (-1, -1) => 315,
            _ => return None,
        };
        args.remove(0);
    } else if first.ends_with("deg") || first.ends_with("turn") || first.ends_with("rad") {
        let n = parse_f(first)?;
        let deg = if first.ends_with("turn") { n * 360.0 } else if first.ends_with("rad") { n * 57.29578 } else { n };
        let d = deg % 360.0;
        angle = (if d < 0.0 { d + 360.0 } else { d } + 0.5) as u16 % 360;
        args.remove(0);
    }
    if args.len() < 2 {
        return None;
    }
    let mut stops: Vec<(u32, Option<u16>)> = Vec::new();
    for a in args.iter().take(GRAD_MAX) {
        let toks = split_top(a, ' ');
        let (c, _) = parse_color_a(toks.first()?)?;
        let pos = toks.get(1).and_then(|p| p.strip_suffix('%')).and_then(parse_f).map(|f| (f.clamp(0.0, 100.0) * 10.0) as u16);
        stops.push((c, pos));
    }
    // joyi berilmaganlar teng taqsimlanadi; kamaymaydigan qilib tuzatiladi
    let n = stops.len();
    let mut out = [(0u32, 0u16); GRAD_MAX];
    let mut prev = 0u16;
    for (k, (c, p)) in stops.iter().enumerate() {
        let auto = (k * 1000 / (n - 1)) as u16;
        let p = p.unwrap_or(auto).max(prev);
        out[k] = (*c, p);
        prev = p;
    }
    Some(Grad { angle, n: n as u8, stops: out })
}

/// `box-shadow: [inset] dx dy [blur [spread]] rang` (birinçi soya; `none` — None).
pub fn parse_shadow(v: &str) -> Option<Shadow> {
    let low = v.trim().to_ascii_lowercase();
    if low.is_empty() || low == "none" {
        return None;
    }
    let first = *split_top(&low, ',').first()?;
    let mut nums: Vec<f32> = Vec::new();
    let mut col = None;
    for t in split_top(first, ' ') {
        if t == "inset" {
            return None; // içki soya qöllab-quvvatlanmaydi
        }
        if let Some(c) = parse_color_a(t) {
            col = Some(c);
        } else if let Some(n) = length(t, 16.0) {
            nums.push(n);
        } else {
            return None;
        }
    }
    if nums.len() < 2 || nums.len() > 4 {
        return None;
    }
    let (color, alpha) = col.unwrap_or((0, 255));
    if alpha == 0 {
        return None;
    }
    Some(Shadow {
        dx: nums[0].clamp(-60.0, 60.0),
        dy: nums[1].clamp(-60.0, 60.0),
        blur: nums.get(2).copied().unwrap_or(0.0).clamp(0.0, 60.0),
        spread: nums.get(3).copied().unwrap_or(0.0).clamp(-30.0, 30.0),
        color,
        alpha,
    })
}

pub fn parse_color(v: &str) -> Option<u32> {
    let v = v.trim().to_ascii_lowercase();
    if let Some(h) = v.strip_prefix('#') {
        let h: String = h.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        return match h.len() {
            3 | 4 => {
                let d: Vec<u32> = h.chars().take(3).map(|c| c.to_digit(16).unwrap() * 17).collect();
                Some(d[0] << 16 | d[1] << 8 | d[2])
            }
            6 | 8 => u32::from_str_radix(&h[..6], 16).ok(),
            _ => None,
        };
    }
    if let Some(r) = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb(")) {
        let nums: Vec<u32> = r
            .trim_end_matches(')')
            .split([',', ' ', '/'])
            .filter(|x| !x.is_empty())
            .take(3)
            .filter_map(|x| {
                if let Some(p) = x.strip_suffix('%') {
                    p.parse::<f32>().ok().map(|f| (f * 2.55) as u32)
                } else {
                    x.parse::<f32>().ok().map(|f| f as u32)
                }
            })
            .collect();
        if nums.len() == 3 {
            return Some(nums[0].min(255) << 16 | nums[1].min(255) << 8 | nums[2].min(255));
        }
        return None;
    }
    NAMED.iter().find(|(n, _)| *n == v).map(|(_, c)| *c)
}

fn parse_f(s: &str) -> Option<f32> {
    let s = s.trim();
    let end = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(s.len());
    let (num, _) = s.split_at(end);
    let mut v = 0f32;
    let mut neg = false;
    let mut frac = 0f32;
    let mut seen = false;
    for c in num.chars() {
        match c {
            '-' => neg = true,
            '.' => frac = 0.1,
            d => {
                let dv = d.to_digit(10)? as f32;
                seen = true;
                if frac > 0.0 {
                    v += dv * frac;
                    frac *= 0.1;
                } else {
                    v = v * 10.0 + dv;
                }
            }
        }
    }
    if !seen {
        return None;
    }
    Some(if neg { -v } else { v })
}

/// Shrift ölçami: px, pt, em, rem, %, kalit sözlar.
fn font_size(v: &str, parent: f32) -> Option<f32> {
    let v = v.trim().to_ascii_lowercase();
    Some(match v.as_str() {
        "xx-small" => 9.0,
        "x-small" => 10.0,
        "small" => 13.0,
        "medium" => 16.0,
        "large" => 18.0,
        "x-large" => 24.0,
        "xx-large" => 32.0,
        "smaller" => parent * 0.83,
        "larger" => parent * 1.2,
        _ => {
            let n = parse_f(&v)?;
            if v.ends_with("rem") {
                n * 16.0
            } else if v.ends_with("em") {
                n * parent
            } else if v.ends_with('%') {
                n * parent / 100.0
            } else if v.ends_with("pt") {
                n * 4.0 / 3.0
            } else {
                n
            }
        }
    })
}

/// Uzunlik: px, em, rem, pt (foiz — None; kenglik uçun alohida).
fn length(v: &str, fsize: f32) -> Option<f32> {
    let v = v.trim();
    if v == "0" || v == "auto" {
        return Some(0.0);
    }
    let n = parse_f(v)?;
    Some(if v.ends_with("rem") {
        n * 16.0
    } else if v.ends_with("em") {
        n * fsize
    } else if v.ends_with("pt") {
        n * 4.0 / 3.0
    } else if v.ends_with('%') || v.ends_with("vw") || v.ends_with("vh") {
        return None;
    } else {
        n
    })
}

/// Bitta burçak radiusi: uzunlik (CSS px, 0..4000) yoki foiz (manfiy son: 50% → -50.0).
fn radius_val(x: &str, fsize: f32) -> Option<f32> {
    match x.trim().strip_suffix('%') {
        Some(p) => parse_f(p).map(|f| -f.clamp(0.0, 100.0)).map(|f| if f == 0.0 { 0.0 } else { f }),
        None => length(x, fsize).map(|f| f.clamp(0.0, 4000.0)),
    }
}

/// Burçak radiuslari (TL, TR, BR, BL; musbat — piksel, manfiy — foiz) → w×h qutidagi butun
/// piksel radiuslar [gorizontal, vertikal]. Foiz CSS kabi: gorizontal — qutining enidan,
/// vertikal — böyidan (kvadratda 50% — doira, nokvadratda — ellips). Bir tomondagi ikki
/// radius yiğindisi tomondan oşsa, CSS qoidasi böyiça hamma radius bir xil
/// f = min(tomon / yiğindi) koeffitsiyentga kiçraytiriladi (gfx::fit_radii).
pub fn radius_px(r: [f32; 4], w: usize, h: usize) -> crate::gfx::Radii {
    let v = r.map(|x| if x < 0.0 { [-x * w as f32 / 100.0, -x * h as f32 / 100.0] } else { [x, x] });
    crate::gfx::fit_radii(v, w, h)
}

/// border-radius tekşiruvi: 1/2/3/4 qiymatli yozuv, alohida burçak xossalari, foiz va
/// yiğindi tomondan oşganda proporsional kiçraytiriş.
pub fn self_test_radius() -> Result<usize, &'static str> {
    let root = Sty::root();
    let st = |d: &str| compute(&root, &[], &ElRef { tag: "div", ..ElRef::default() }, &[], Some(d)).radius;
    let mut n = 0;
    for (d, want) in [
        ("border-radius: 8px", [8.0, 8.0, 8.0, 8.0]),
        ("border-radius: 10px 4px", [10.0, 4.0, 10.0, 4.0]),
        ("border-radius: 10px 4px 2px", [10.0, 4.0, 2.0, 4.0]),
        ("border-radius: 10px 0 20px 5px", [10.0, 0.0, 20.0, 5.0]),
        ("border-radius: 1em 2px 3px 4px / 9px", [16.0, 2.0, 3.0, 4.0]),
        ("border-radius: 50%", [-50.0, -50.0, -50.0, -50.0]),
        ("border-radius: 120px", [120.0, 120.0, 120.0, 120.0]),
        ("border-radius: 6px; border-top-right-radius: 0; border-bottom-left-radius: 25%", [6.0, 0.0, 6.0, -25.0]),
        ("border-top-left-radius: 12px 30px", [12.0, 0.0, 0.0, 0.0]),
    ] {
        if st(d) != want {
            return Err("border-radius: qiymatlar tahlili xato");
        }
        n += 1;
    }
    for (r, w, h, want) in [
        ([8.0, 8.0, 8.0, 8.0], 100, 50, [[8, 8]; 4]),
        ([-50.0; 4], 64, 64, [[32, 32]; 4]),         // 50% kvadratda — doira
        ([-50.0; 4], 200, 40, [[100, 20]; 4]),       // nokvadratda — ellips (CSS kabi)
        ([-10.0, 0.0, -10.0, 0.0], 200, 100, [[20, 10], [0, 0], [20, 10], [0, 0]]),
        ([200.0; 4], 100, 60, [[30, 30]; 4]),        // yiğindi oşdi: f = 60/400
        ([60.0, 20.0, 0.0, 0.0], 40, 100, [[30, 30], [10, 10], [0, 0], [0, 0]]), // yuqori: f = 40/80
        ([90.0, 0.0, 90.0, 0.0], 100, 100, [[90, 90], [0, 0], [90, 90], [0, 0]]), // qarama-qarşi kiçraymaydi
        ([999.0; 4], 300, 40, [[20, 20]; 4]),        // "tabletka": f = 40/1998
    ] {
        if radius_px(r, w, h) != want {
            return Err("border-radius: piksel / foiz / kiçraytiriş xato");
        }
        n += 1;
    }
    // aniq "padding: 0" — -0.0 (doira avatar uçun maket çekiniş qöşmaydi), boşqalari — oddiy
    let el = ElRef { tag: "div", ..ElRef::default() };
    let pd = |d: &str| compute(&root, &[], &el, &[], Some(d)).pad;
    if !pd("padding: 0").is_sign_negative() || !pd("padding: 0 0").is_sign_negative() || pd("padding: 0 4px").is_sign_negative() || pd("padding: 0 4px") != 0.0
        || pd("padding-left: 0").is_sign_negative() || pd("").is_sign_negative() || pd("padding: 6px") != 6.0 {
        return Err("padding: 0 (aniq nol) belgisi xato");
    }
    n += 1;
    Ok(n)
}

/// Bitta e'lonni uslubga qöllaydi. `parent` — ota shrift ölçami.
pub fn apply(s: &mut Sty, prop: &str, val: &str, parent: f32) {
    let low = val.to_ascii_lowercase();
    if low.contains("calc(") {
        apply_calc(s, prop, &low, parent);
        return;
    }
    let cwb = if s.cw > 0.0 { s.cw } else { viewport().0 };
    let lenp = |x: &str, fs: f32| -> Option<f32> {
        match x.trim().strip_suffix('%') {
            Some(p) => parse_f(p).map(|f| f * cwb / 100.0),
            None => length(x, fs),
        }
    };
    match prop {
        "color" => {
            if let Some(c) = parse_color(val) {
                s.color = Some(c);
            }
        }
        "background-image" => {
            if low.trim() == "none" {
                s.grad = None;
            } else if let Some(g) = parse_gradient(&low) {
                s.grad = Some(g);
                s.bg = Some(g.stops[0].0);
            }
        }
        "box-shadow" => s.shadow = parse_shadow(&low),
        "background-color" | "background" => {
            if prop == "background" && low.contains("gradient(") {
                // yaroqsiz gradient — e'tiborsiz (fon özgarmaydi)
                if let Some(g) = parse_gradient(&low) {
                    s.grad = Some(g);
                    s.bg = Some(g.stops[0].0);
                }
            } else if low == "none" || low == "transparent" {
                s.bg = None;
                if prop == "background" {
                    s.grad = None;
                }
            } else if let Some(c) = parse_color(val).or_else(|| val.split_whitespace().find_map(parse_color)) {
                s.bg = Some(c);
                if prop == "background" {
                    s.grad = None;
                }
            }
        }
        "font-size" => {
            if let Some(f) = font_size(val, parent) {
                s.size = f.clamp(6.0, 96.0);
            }
        }
        "font-weight" => s.bold = low == "bold" || low == "bolder" || parse_f(&low).is_some_and(|n| n >= 600.0),
        "font-style" => s.italic = low == "italic" || low == "oblique",
        "font" => {
            s.bold |= low.contains("bold");
            if let Some(f) = low.split_whitespace().find_map(|t| if t.ends_with("px") || t.ends_with("em") || t.ends_with("pt") { font_size(t.split('/').next().unwrap_or(t), parent) } else { None }) {
                s.size = f.clamp(6.0, 96.0);
            }
        }
        "text-align" => {
            s.align = match low.as_str() {
                "center" | "-webkit-center" | "-moz-center" => 1,
                "right" | "-webkit-right" => 2,
                "left" | "-webkit-left" => 3,
                "end" => 4,
                "justify" => 5,
                "start" => 0,
                _ => s.align, // inherit / match-parent va h.k. — meros qiymati qoladi
            }
        }
        "direction" => {
            match low.as_str() {
                "rtl" => s.rtl = true,
                "ltr" => s.rtl = false,
                _ => {}
            }
        }
        "display" => {
            s.hidden = low == "none";
            s.flex = match low.as_str() {
                "flex" | "inline-flex" => 1,
                "grid" | "inline-grid" => 3,
                _ => 0,
            };
            s.block = match low.as_str() {
                "block" | "flex" | "grid" | "list-item" | "table" => Some(true),
                "inline-block" => Some(true),
                "inline" | "inline-flex" => Some(false),
                _ => s.block,
            };
            s.ib = low == "inline-block";
        }
        "visibility" => {
            if matches!(low.as_str(), "hidden" | "collapse") {
                s.invis = true;
            } else if low == "visible" {
                s.invis = false;
            }
        }
        "vertical-align" => {
            s.valign = match low.as_str() {
                "middle" => 1,
                "top" | "text-top" => 2,
                "bottom" | "text-bottom" => 3,
                _ => 0,
            }
        }
        "flex-direction" => {
            if s.flex == 1 && low.starts_with("column") {
                s.flex = 2;
            } else if s.flex == 2 && low.starts_with("row") {
                s.flex = 1;
            }
        }
        "gap" | "column-gap" | "grid-gap" | "grid-column-gap" => {
            if let Some(v) = low.split_whitespace().last().and_then(|x| length(x, s.size)) {
                s.gap = v.clamp(0.0, 100.0);
            }
        }
        "grid-template-columns" => {
            // repeat(3, 1fr) yoki "1fr 2fr 1fr" — ustunlar soni
            let n = if let Some(r) = low.strip_prefix("repeat(") {
                r.split(',').next().and_then(|x| x.trim().parse::<u8>().ok()).unwrap_or(2)
            } else {
                low.split_whitespace().count() as u8
            };
            s.gcols = n.clamp(1, 12);
        }
        "justify-content" => {
            s.justify = match low.as_str() {
                "center" => 1,
                "space-between" | "space-around" | "space-evenly" => 2,
                "flex-end" | "end" | "right" => 3,
                _ => 0,
            }
        }
        "flex" | "flex-grow" => {
            if let Some(f) = low.split_whitespace().next().and_then(parse_f) {
                s.grow = f.clamp(0.0, 12.0);
            }
        }
        "margin" => {
            let p: Vec<&str> = low.split_whitespace().collect();
            let top = p.first().and_then(|x| lenp(x, s.size)).unwrap_or(0.0);
            let bot = p.get(2).or(p.first()).and_then(|x| lenp(x, s.size)).unwrap_or(top);
            s.mt = top.clamp(0.0, 200.0);
            s.mb = bot.clamp(0.0, 200.0);
            let horiz = if p.len() >= 2 { p[1] } else { p.first().copied().unwrap_or("") };
            s.center = horiz == "auto";
        }
        "margin-top" => s.mt = lenp(&low, s.size).unwrap_or(0.0).clamp(0.0, 200.0),
        "margin-bottom" => s.mb = lenp(&low, s.size).unwrap_or(0.0).clamp(0.0, 200.0),
        "margin-left" | "margin-right" | "margin-inline" if low == "auto" => s.center = true,
        "padding" | "padding-top" | "padding-left" | "padding-inline" | "padding-block" => {
            if let Some(v) = low.split_whitespace().next().and_then(|x| lenp(x, s.size)) {
                s.pad = s.pad.max(v.clamp(0.0, 80.0));
            }
            // aniq "padding: 0" (hamma qiymat 0) — -0.0 (arifmetikada 0 ga teng): maket fonli
            // qutiga qöşadigan eng kiçik ichki çekinişni qöşmaydi (masalan, 50% doira avatar)
            if prop == "padding" && s.pad == 0.0 && low.split_whitespace().all(|x| lenp(x, s.size) == Some(0.0)) {
                s.pad = -0.0;
            }
        }
        "border" | "border-top" | "border-bottom" => {
            if low == "none" || low == "0" {
                s.bw = 0.0;
            } else {
                for t in low.split_whitespace() {
                    if let Some(c) = parse_color(t) {
                        s.bc = Some(c);
                    } else if let Some(w) = length(t, s.size) {
                        s.bw = w.clamp(0.0, 12.0);
                    } else if t == "solid" || t == "dashed" || t == "dotted" || t == "double" {
                        if s.bw == 0.0 {
                            s.bw = 1.0;
                        }
                    }
                }
                if s.bw > 0.0 && s.bc.is_none() {
                    s.bc = Some(0x808080);
                }
            }
        }
        "border-width" => s.bw = length(&low, s.size).unwrap_or(1.0).clamp(0.0, 12.0),
        "border-color" => s.bc = parse_color(val),
        "border-radius" => {
            // 1–4 qiymat (TL TR BR BL tartibida, CSS kabi); "/" dan keyingi (elliptik
            // vertikal) qism e'tiborga olinmaydi — burçaklar doiraviy çiziladi
            let h = low.split('/').next().unwrap_or("");
            let v: Vec<f32> = h.split_whitespace().map(|x| radius_val(x, s.size).unwrap_or(0.0)).take(4).collect();
            s.radius = match v.len() {
                0 => [0.0; 4],
                1 => [v[0]; 4],
                2 => [v[0], v[1], v[0], v[1]],
                3 => [v[0], v[1], v[2], v[1]],
                _ => [v[0], v[1], v[2], v[3]],
            };
        }
        "border-top-left-radius" | "border-top-right-radius" | "border-bottom-right-radius" | "border-bottom-left-radius" => {
            let i = match prop {
                "border-top-left-radius" => 0,
                "border-top-right-radius" => 1,
                "border-bottom-right-radius" => 2,
                _ => 3,
            };
            s.radius[i] = low.split(|c: char| c.is_whitespace() || c == '/').find(|x| !x.is_empty()).and_then(|x| radius_val(x, s.size)).unwrap_or(0.0);
        }
        "width" | "max-width" => {
            if let Some(p) = low.strip_suffix('%') {
                if let Some(f) = parse_f(p) {
                    s.maxw = Some((f.clamp(1.0, 100.0), true));
                    s.wpx = 0.0;
                }
            } else if let Some(px) = length(&low, s.size).filter(|v| *v > 0.0) {
                s.maxw = Some((px, false));
                s.wpx = 0.0;
            }
        }
        "height" | "min-height" => {
            let v = if let Some(p) = low.strip_suffix('%') {
                parse_f(p).map(|f| (f.clamp(0.0, 1000.0), true))
            } else if let Some(p) = low.strip_suffix("vh") {
                parse_f(p).map(|f| (f.clamp(0.0, 1000.0), true))
            } else {
                length(&low, s.size).map(|px| (px.clamp(0.0, 20000.0), false))
            };
            // auto / none / yaroqsiz — belgilanmagan
            let v = v.filter(|x| x.0 > 0.0);
            if prop == "height" {
                s.h = v;
            } else {
                s.minh = v;
            }
        }
        "line-height" => {
            if low == "normal" || low == "initial" {
                s.lh = 0.0;
            } else if let Some(f) = Some(low.as_str()).filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')).and_then(parse_f).filter(|f| *f > 0.0) {
                s.lh = f.clamp(0.3, 10.0); // raqam — bolalar ham kö'paytuvçini oladi
            } else if let Some(f) = low.strip_suffix('%').and_then(parse_f).filter(|f| *f > 0.0) {
                s.lh = -(s.size * f / 100.0).clamp(1.0, 1000.0);
            } else if let Some(px) = length(&low, s.size).filter(|v| *v > 0.0) {
                s.lh = -px.clamp(1.0, 1000.0);
            }
        }
        "text-transform" => {
            s.tt = match low.as_str() {
                "uppercase" => 1,
                "lowercase" => 2,
                "capitalize" => 3,
                "none" => 0,
                _ => s.tt,
            }
        }
        "position" => {
            s.pos = match low.as_str() {
                "relative" | "sticky" => 1,
                "absolute" => 2,
                "fixed" => 3,
                _ => 0,
            }
        }
        "top" | "right" | "bottom" | "left" | "inset" => {
            let v = if low == "auto" { None } else { low.split_whitespace().next().and_then(|x| length(x, s.size)).map(|v| v.clamp(-4000.0, 4000.0)) };
            match prop {
                "top" => s.ins[0] = v,
                "right" => s.ins[1] = v,
                "bottom" => s.ins[2] = v,
                "left" => s.ins[3] = v,
                _ => s.ins = [v; 4],
            }
        }
        "float" => {
            s.float = match low.as_str() {
                "left" | "inline-start" => 1,
                "right" | "inline-end" => 2,
                _ => 0,
            }
        }
        "clear" => {
            s.clear = match low.as_str() {
                "left" | "inline-start" => 1,
                "right" | "inline-end" => 2,
                "both" => 3,
                _ => 0,
            }
        }
        "overflow" | "overflow-x" | "overflow-y" => {
            // overflow: hidden auto — ikki qiymatdan birortasi visible emas bölsa qirqiladi
            s.ovf = if low.split_whitespace().any(|t| matches!(t, "hidden" | "auto" | "scroll" | "clip" | "overlay")) {
                1
            } else if low.split_whitespace().all(|t| t == "visible") && !low.is_empty() {
                0
            } else {
                s.ovf
            };
        }
        "white-space" | "white-space-collapse" | "text-wrap" | "text-wrap-mode" => {
            s.ws = match (prop, low.trim()) {
                ("text-wrap" | "text-wrap-mode", "nowrap") => 1,
                ("text-wrap" | "text-wrap-mode", "wrap") => if s.ws == 1 { 0 } else { s.ws },
                (_, "normal") => 0,
                (_, "nowrap") => 1,
                (_, "pre") => 2,
                (_, "pre-wrap" | "break-spaces" | "preserve") => 3,
                (_, "pre-line" | "preserve-breaks") => 4,
                _ => s.ws,
            }
        }
        "text-overflow" => s.ellip = low.contains("ellipsis") || low.contains('…'),
        "opacity" => {
            let v = if let Some(p) = low.trim().strip_suffix('%') { parse_f(p).map(|f| f / 100.0) } else { parse_f(low.trim()) };
            if let Some(v) = v {
                s.opacity = v.clamp(0.0, 1.0);
            }
        }
        "text-decoration" | "text-decoration-line" => {
            s.underline = Some(low.contains("underline"));
            s.strike = low.contains("line-through");
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// calc() va maxsus xossalar (--nom / var())
// ---------------------------------------------------------------------------

/// calc() natijasi: `px + pct%` (foiz — konteyner eniga nisbatan) yoki toza raqam (`num`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Calc {
    pub px: f32,
    pub pct: f32,
    pub num: bool,
}

struct CalcP<'a> {
    b: &'a [u8],
    i: usize,
    fs: f32,
}

impl CalcP<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&self) -> u8 {
        self.b.get(self.i).copied().unwrap_or(0)
    }
    fn expr(&mut self, d: u8) -> Option<Calc> {
        if d > 16 {
            return None;
        }
        let mut a = self.term(d)?;
        loop {
            self.ws();
            let op = self.peek();
            if op != b'+' && op != b'-' {
                return Some(a);
            }
            self.i += 1;
            let t = self.term(d)?;
            if t.num != a.num {
                return None; // raqam va uzunlik qöşilmaydi
            }
            let k = if op == b'-' { -1.0 } else { 1.0 };
            a.px += k * t.px;
            a.pct += k * t.pct;
        }
    }
    fn term(&mut self, d: u8) -> Option<Calc> {
        let mut a = self.factor(d)?;
        loop {
            self.ws();
            match self.peek() {
                b'*' => {
                    self.i += 1;
                    let t = self.factor(d)?;
                    let (n, mut v) = if a.num { (a.px, t) } else if t.num { (t.px, a) } else { return None };
                    v.px *= n;
                    v.pct *= n;
                    a = v;
                }
                b'/' => {
                    self.i += 1;
                    let t = self.factor(d)?;
                    if !t.num || t.px == 0.0 {
                        return None; // faqat noldan farqli raqamga bölinadi
                    }
                    a.px /= t.px;
                    a.pct /= t.px;
                }
                _ => return Some(a),
            }
        }
    }
    fn factor(&mut self, d: u8) -> Option<Calc> {
        self.ws();
        let rest = &self.b[self.i..];
        if rest.first() == Some(&b'(') || rest.starts_with(b"calc(") {
            self.i += if rest[0] == b'(' { 1 } else { 5 };
            let v = self.expr(d + 1)?;
            self.ws();
            if self.peek() != b')' {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        match self.peek() {
            b'-' | b'+' => {
                let neg = self.peek() == b'-';
                self.i += 1;
                let mut v = self.factor(d + 1)?;
                if neg {
                    v.px = -v.px;
                    v.pct = -v.pct;
                }
                return Some(v);
            }
            _ => {}
        }
        // raqam
        let st = self.i;
        while self.i < self.b.len() && (self.b[self.i].is_ascii_digit() || self.b[self.i] == b'.') {
            self.i += 1;
        }
        if self.i == st {
            return None;
        }
        let n = parse_f(core::str::from_utf8(&self.b[st..self.i]).ok()?)?;
        // birlik
        let us = self.i;
        while self.i < self.b.len() && (self.b[self.i].is_ascii_alphabetic() || self.b[self.i] == b'%') {
            self.i += 1;
        }
        let (vw, vh) = viewport();
        let l = |px: f32| Some(Calc { px, pct: 0.0, num: false });
        match &self.b[us..self.i] {
            b"" => Some(Calc { px: n, pct: 0.0, num: true }),
            b"px" => l(n),
            b"em" => l(n * self.fs),
            b"rem" => l(n * 16.0),
            b"pt" => l(n * 4.0 / 3.0),
            b"vw" => l(n * vw / 100.0),
            b"vh" => l(n * vh / 100.0),
            b"%" => Some(Calc { px: 0.0, pct: n, num: false }),
            _ => None,
        }
    }
}

/// `calc(...)` ifodasini hisoblaydi (`fs` — em uçun shrift ölçami). Noto'g'ri ifoda — None.
pub fn calc(v: &str, fs: f32) -> Option<Calc> {
    let low = v.trim().to_ascii_lowercase();
    if !low.starts_with("calc(") {
        return None;
    }
    let mut p = CalcP { b: low.as_bytes(), i: 0, fs };
    let r = p.factor(0)?;
    p.ws();
    if p.i != p.b.len() || !r.px.is_finite() || !r.pct.is_finite() {
        return None;
    }
    Some(r)
}

/// Qavs mosini topadi: `s[open]` — '(' ; qaytaradi — mos ')' indeksi.
fn close_paren(s: &str, open: usize) -> Option<usize> {
    let mut d = 0i32;
    for (i, c) in s.bytes().enumerate().skip(open) {
        match c {
            b'(' => d += 1,
            b')' => {
                d -= 1;
                if d == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Qiymat içidagi har bir calc(...) ni hisoblangan sonli qiymatga almaştiradi
/// (foiz `base` ga nisbatan). Biror ifoda noto'g'ri bölsa — None.
pub fn subst_calc(v: &str, fs: f32, base: f32) -> Option<String> {
    let mut out = String::new();
    let mut rest = v;
    while let Some(k) = rest.find("calc(") {
        let e = close_paren(rest, k + 4)?;
        let c = calc(&rest[k..=e], fs)?;
        out.push_str(&rest[..k]);
        let n = c.px + c.pct * base / 100.0;
        out.push_str(&if c.num { alloc::format!("{}", n) } else { alloc::format!("{}px", n) });
        rest = &rest[e + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// calc() li e'lon: width / max-width — (foiz, px) juftligi maket bosqiçida yeçiladi;
/// font-size — ota shriftga nisbatan; qolganlari — sonli qiymatga almaştirilib qöllanadi.
fn apply_calc(s: &mut Sty, prop: &str, low: &str, parent: f32) {
    let t = low.trim();
    let whole = t.starts_with("calc(") && close_paren(t, 4) == Some(t.len() - 1);
    match prop {
        "width" | "max-width" if whole => {
            if let Some(c) = calc(t, s.size).filter(|c| !c.num) {
                if c.pct != 0.0 {
                    s.maxw = Some((c.pct.clamp(0.1, 1000.0), true));
                    s.wpx = c.px;
                } else if c.px > 0.0 {
                    s.maxw = Some((c.px, false));
                    s.wpx = 0.0;
                }
            }
        }
        "font-size" if whole => {
            if let Some(c) = calc(t, parent).filter(|c| !c.num) {
                s.size = (c.px + c.pct * parent / 100.0).clamp(6.0, 96.0);
            }
        }
        _ => {
            let base = match prop {
                "height" | "min-height" | "top" | "bottom" => viewport().1,
                "line-height" => s.size,
                _ if s.cw > 0.0 => s.cw,
                _ => viewport().0,
            };
            if let Some(v) = subst_calc(low, s.size, base) {
                apply(s, prop, &v, parent);
            }
        }
    }
}

/// Foizli kenglik (`maxw`) + calc() px bölagi → CSS px (`base` — konteyner eni).
pub fn resolve_w(w: (f32, bool), wpx: f32, base: f32) -> f32 {
    if w.1 { base * w.0 / 100.0 + wpx } else { w.0 }
}

/// Maxsus xossalar (`--nom: qiymat`) — elementda saqlanadi va bolalarga meros bo'ladi.
/// Qiymatlar içidagi var() lar e'lon qilingan elementda yeçilgan holda saqlanadi.
pub type Vars = alloc::rc::Rc<Vec<(String, String)>>;

/// Böş o'zgaruvçilar tóplami (ildiz).
pub fn no_vars() -> Vars {
    alloc::rc::Rc::new(Vec::new())
}

/// var() ichma-ichligi çegarasi (sikl — `--a: var(--b); --b: var(--a)` — şu yerda töxtaydi).
const VAR_DEPTH: u8 = 8;

/// Qiymat içidagi var(--nom[, zaxira]) larni almaştiradi. Topilmasa va zaxira yöq bölsa,
/// yoki sikl / ichma-ichlik çegarasi oşsa — None (xossa e'tiborsiz qoladi).
pub fn subst_var(v: &str, vars: &[(String, String)], depth: u8) -> Option<String> {
    if depth > VAR_DEPTH {
        return None;
    }
    let mut out = String::new();
    let mut rest = v;
    loop {
        let Some(k) = rest.find("var(").or_else(|| rest.find("VAR(")) else { break };
        let e = close_paren(rest, k + 3)?;
        let inner = &rest[k + 4..e];
        // birinçi yuqori darajadagi vergul — nom va zaxira qiymat orasida
        let mut d = 0i32;
        let comma = inner.bytes().position(|c| {
            match c {
                b'(' => d += 1,
                b')' => d -= 1,
                _ => {}
            }
            c == b',' && d == 0
        });
        let (name, fb) = match comma {
            Some(i) => (inner[..i].trim(), Some(inner[i + 1..].trim())),
            None => (inner.trim(), None),
        };
        let name = name.to_ascii_lowercase();
        if !name.starts_with("--") {
            return None;
        }
        let val = match vars.iter().rev().find(|(n, _)| *n == name) {
            Some((_, x)) => subst_var(x, vars, depth + 1)?,
            None => subst_var(fb?, vars, depth + 1)?,
        };
        out.push_str(&rest[..k]);
        out.push_str(&val);
        rest = &rest[e + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Elementning yangi maxsus xossalari (tartib böyiça, oxirgisi ustun) → meros bölgan tóplam bilan birlaşma.
fn merge_vars(pvars: &Vars, new: Vec<(String, String)>) -> Vars {
    if new.is_empty() {
        return pvars.clone();
    }
    let mut m: Vec<(String, String)> = (**pvars).clone();
    let mut keys: Vec<String> = Vec::new();
    for (k, v) in new {
        match m.iter_mut().find(|(n, _)| *n == k) {
            Some(x) => x.1 = v,
            None => m.push((k.clone(), v)),
        }
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    // şu elementda e'lon qilinganlarni yeçamiz (bir-biriga va otadan kelganlarga tayanişi mumkin)
    let res: Vec<(String, Option<String>)> = keys
        .into_iter()
        .map(|k| {
            let raw = m.iter().find(|(n, _)| *n == k).map(|x| x.1.clone()).unwrap_or_default();
            let r = if raw.contains("var(") || raw.contains("VAR(") { subst_var(&raw, &m, 0) } else { Some(raw) };
            (k, r)
        })
        .collect();
    for (k, r) in res {
        match r {
            Some(v) => {
                if let Some(x) = m.iter_mut().find(|(n, _)| *n == k) {
                    x.1 = v;
                }
            }
            None => m.retain(|(n, _)| *n != k), // yaroqsiz (sikl) — e'lon qilinmagandek
        }
    }
    alloc::rc::Rc::new(m)
}

/// Blok uçun yakuniy tekislaş: 0 — çap, 1 — markaz, 2 — öng, 3 — justify
/// (start/end yönalişga — `direction` / `dir` ga qarab aniqlanadi).
pub fn resolve_align(align: u8, rtl: bool) -> u8 {
    match (align, rtl) {
        (1, _) => 1,
        (2, _) => 2,
        (3, _) => 0,
        (5, _) => 3,
        (4, false) | (0, true) => 2,
        _ => 0,
    }
}

/// Uslublar jadvali va inline uslubdan elementning hisoblangan uslubi.
pub fn compute(parent: &Sty, rules: &[Rule], el: &ElRef, ancestors: &[ElRef], inline: Option<&str>) -> Sty {
    compute_v(parent, &no_vars(), rules, el, ancestors, inline).0
}

/// `compute` + maxsus xossalar: ota o'zgaruvçilaridan (`pvars`) meros oladi, öz `--nom` larini
/// qöşadi va var() larni qiymat qöllanişidan oldin almaştiradi. Qaytaradi: uslub va bolalar uçun tóplam.
pub fn compute_v(parent: &Sty, pvars: &Vars, rules: &[Rule], el: &ElRef, ancestors: &[ElRef], inline: Option<&str>) -> (Sty, Vars) {
    let mut s = parent.inherit();
    let inl = inline.map(parse_decls).unwrap_or_default();
    let mut decls: Vec<(&str, &str)> = Vec::new();
    // Kaskad: brauzer uslublari, keyin muallif qoidalari specificity böyiça (teng bölsa — manba tartibi)
    let mut hits: Vec<(bool, u32, usize)> = Vec::new();
    for (ri, r) in rules.iter().enumerate() {
        if !media_ok(&r.media) {
            continue;
        }
        if let Some(sp) = r.sel.iter().filter(|ch| matches(ch, el, ancestors)).map(|ch| specificity(ch)).max() {
            hits.push((!r.ua, sp, ri));
        }
    }
    hits.sort_by_key(|h| (h.0, h.1));
    for &(_, _, ri) in &hits {
        decls.extend(rules[ri].decls.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }
    decls.extend(inl.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let custom: Vec<(String, String)> = decls.iter().filter(|(k, _)| k.starts_with("--")).map(|(k, v)| (String::from(*k), String::from(*v))).collect();
    let vars = merge_vars(pvars, custom);
    for (k, v) in decls {
        if k.starts_with("--") {
            continue;
        }
        if v.contains("var(") || v.contains("VAR(") {
            if let Some(x) = subst_var(v, &vars, 0) {
                apply(&mut s, k, &x, parent.size);
            }
        } else {
            apply(&mut s, k, v, parent.size);
        }
    }
    // bolalar uçun taxminiy konteyner eni
    let base = if parent.cw > 0.0 { parent.cw } else { viewport().0 };
    let w = match s.maxw {
        Some(m) => resolve_w(m, s.wpx, base).min(base),
        None => base,
    };
    s.cw = (w - 2.0 * (s.pad + s.bw)).max(1.0);
    (s, vars)
}

/// camelCase (JS: `style.backgroundColor`) → kebab-case (`background-color`).
pub fn kebab(prop: &str) -> String {
    let mut out = String::new();
    for c in prop.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// O'z-özini tekşiruv: maxsus xossalar (var(), zaxira, meros, sikl) va calc().
pub fn self_test_vars() -> Result<usize, &'static str> {
    let mut n = 0;
    let rules = parse_sheet(
        ":root { --c: #ff0000; --pad: 8px; --a: var(--b); --b: var(--a); --x: var(--c) }
         p { color: var(--c); padding: var(--pad) }
         .fb { color: var(--yoq, #00ff00) }
         .fb2 { color: var(--yoq, var(--c2, rgb(0, 0, 255))) }
         .ota { --c: #123456 }
         .cyc { color: var(--a, #00ff00); background-color: var(--b) }
         .nx { color: var(--x) }
         .calc { width: calc(50% - 20px); margin-top: calc(10px + 2em); font-size: 16px }
         .bad { width: calc(50% - ); padding: calc(10px * 2px) }",
    );
    let none: [String; 0] = [];
    let cls = |c: &'static str| alloc::vec![String::from(c)];
    fn el<'a>(tag: &'a str, classes: &'a [String]) -> ElRef<'a> {
        ElRef { tag, id: "", classes, ..ElRef::default() }
    }
    let root = Sty::root();
    let (hs, hv) = compute_v(&root, &no_vars(), &rules, &el("html", &none), &[], None);
    let html = el("html", &none);
    // 1) var(--c) rangi
    let (p, _) = compute_v(&hs, &hv, &rules, &el("p", &none), core::slice::from_ref(&html), None);
    if p.color != Some(0xff0000) || (p.pad - 8.0).abs() > 0.01 {
        return Err("var(--c) rangi / var(--pad) öqilmadi");
    }
    n += 1;
    // 2) zaxira qiymat (oddiy va ichma-ich)
    let fb = cls("fb");
    let (s, _) = compute_v(&hs, &hv, &rules, &el("div", &fb), core::slice::from_ref(&html), None);
    if s.color != Some(0x00ff00) {
        return Err("var() zaxira qiymati ishlamadi");
    }
    let fb2 = cls("fb2");
    let (s, _) = compute_v(&hs, &hv, &rules, &el("div", &fb2), core::slice::from_ref(&html), None);
    if s.color != Some(0x0000ff) {
        return Err("ichma-ich var() zaxirasi ishlamadi");
    }
    n += 2;
    // 3) meros va qayta belgilaş: .ota --c ni özgartiradi, uning bolasi p yangi qiymatni oladi
    let ota = cls("ota");
    let (os, ov) = compute_v(&hs, &hv, &rules, &el("div", &ota), core::slice::from_ref(&html), None);
    let anc = [el("html", &none), el("div", &ota)];
    let (p2, _) = compute_v(&os, &ov, &rules, &el("p", &none), &anc, None);
    if p2.color != Some(0x123456) {
        return Err("bola ota o'zgaruvçisini qayta belgilay olmadi");
    }
    // inline uslubdagi o'zgaruvçi va :root dan meros (avlodda --x hali ham :root dagi --c ga teng)
    let (p3, _) = compute_v(&hs, &hv, &rules, &el("p", &none), core::slice::from_ref(&html), Some("--c: blue; color: var(--c)"));
    let nx = cls("nx");
    let (p4, _) = compute_v(&os, &ov, &rules, &el("span", &nx), &anc, None);
    if p3.color != Some(0x0000ff) || p4.color != Some(0xff0000) {
        return Err("inline --nom yoki ildizda yeçilgan var() noto'g'ri");
    }
    n += 2;
    // 4) sikl — panic yöq, zaxira ishlaydi, zaxirasiz xossa e'tiborsiz
    let cyc = cls("cyc");
    let (s, _) = compute_v(&hs, &hv, &rules, &el("div", &cyc), core::slice::from_ref(&html), None);
    if s.color != Some(0x00ff00) || s.bg.is_some() || hv.iter().any(|(k, _)| k == "--a" || k == "--b") {
        return Err("var() sikli noto'g'ri yeçildi");
    }
    if subst_var("var(--q)", &[(String::from("--q"), String::from("var(--q)"))], 0).is_some() {
        return Err("öz-özini kö'rsatuvçi var() töxtamadi");
    }
    n += 2;
    // 5) calc()
    let c = calc("calc(10px + 2em)", 16.0).ok_or("calc(10px + 2em) tahlil qilinmadi")?;
    if (c.px - 42.0).abs() > 0.01 || c.pct != 0.0 {
        return Err("calc(10px + 2em) ≠ 42");
    }
    let c = calc("calc(50% - 20px)", 16.0).ok_or("calc(50% - 20px) tahlil qilinmadi")?;
    if (c.px + c.pct * 600.0 / 100.0 - 280.0).abs() > 0.01 {
        return Err("calc(50% - 20px) 600 da ≠ 280");
    }
    for (e, v) in [("calc((10px + 5px) * 2)", 30.0), ("calc(100px / 4 - -5px)", 30.0), ("calc(2 * (1rem + 4px))", 40.0), ("calc(calc(1em) + 1pt * 3)", 20.0)] {
        if calc(e, 16.0).is_none_or(|c| (c.px - v).abs() > 0.01) {
            return Err("calc(): qavslar / * / ichma-ich hisob xato");
        }
    }
    for e in ["calc(10px + 2)", "calc(10px * 2px)", "calc(1px / 0)", "calc(50% - )", "calc(1px", "calc(3foo)"] {
        if calc(e, 16.0).is_some() {
            return Err("noto'g'ri calc() ifodasi qabul qilindi");
        }
    }
    n += 7;
    // 6) uslub orqali: 600 enli konteynerda calc(50% - 20px) → 280; margin-top → 42
    let parent = Sty { cw: 600.0, ..Sty::root() };
    let cc = cls("calc");
    let s = compute(&parent, &rules, &el("div", &cc), &[], None);
    match s.maxw {
        Some(m) if (resolve_w(m, s.wpx, 600.0) - 280.0).abs() < 0.01 => {}
        _ => return Err("width: calc(50% - 20px) uslubda 280 bölmadi"),
    }
    if (s.mt - 42.0).abs() > 0.01 || (s.cw - 280.0).abs() > 0.01 {
        return Err("margin-top: calc(10px + 2em) ≠ 42 yoki konteyner eni xato");
    }
    // foizli padding konteyner eniga nisbatan; noto'g'ri calc — xossa e'tiborsiz
    let s = compute(&parent, &rules, &el("div", &none), &[], Some("padding: calc(5% + 2px)"));
    let bad = cls("bad");
    let b = compute(&parent, &rules, &el("div", &bad), &[], None);
    if (s.pad - 32.0).abs() > 0.01 || b.maxw.is_some() || b.pad != 0.0 {
        return Err("padding: calc(%) yoki noto'g'ri calc() xato qöllandi");
    }
    n += 3;
    Ok(n)
}

/// O'z-özini tekşiruv: birikmalar (>, +, ~), [atribut], :first/last/nth-child, :not(), specificity.
pub fn self_test_selectors() -> Result<usize, &'static str> {
    let mut n = 0;
    let none: [String; 0] = [];
    let at = |k: &str, v: &str| alloc::vec![(String::from(k), String::from(v))];
    fn e<'a>(tag: &'a str, classes: &'a [String]) -> ElRef<'a> {
        ElRef { tag, id: "", classes, ..ElRef::default() }
    }
    let sel = |s: &str| -> Vec<Compound> { parse_selectors(s).into_iter().next().unwrap_or_default() };
    let top = alloc::vec![String::from("top")];
    // 1) "ul > li": bevosita bola mos, nevara (ul > li > div > li) mos emas
    let (ul, li, div) = (e("ul", &top), e("li", &none), e("div", &none));
    let c = sel("ul > li");
    if !matches(&c, &li, &[ul]) || matches(&c, &li, &[ul, li, div]) {
        return Err("'ul > li' nevara li ga mos keldi yoki bolaga mos kelmadi");
    }
    let c2 = sel("ul.top>li");
    let inner_ul = e("ul", &none);
    if !matches(&c2, &li, &[ul]) || matches(&c2, &li, &[ul, li, inner_ul]) {
        return Err("'ul.top>li' ichma-ich ro'yxatda xato");
    }
    if !matches(&sel("ul li"), &li, &[ul, li, div]) {
        return Err("avlod tanlagiçi buzildi");
    }
    n += 3;
    // 2) "h1 + p" — faqat darhol keyingi p; "h1 ~ p" — hammasi
    let body = e("body", &none);
    let kids = [e("h1", &none), e("p", &none), e("p", &none), e("div", &none), e("p", &none)];
    let at_k = |k: usize| ElRef { nth: k as u32 + 1, after: (kids.len() - k - 1) as u32, prev: &kids[..k], ..kids[k] };
    let adj = sel("h1 + p");
    let sib_all = sel("h1 ~ p");
    let hits = |c: &[Compound]| -> Vec<usize> { (0..kids.len()).filter(|&k| matches(c, &at_k(k), core::slice::from_ref(&body))).collect() };
    if hits(&adj) != [1] || hits(&sib_all) != [1, 2, 4] || hits(&sel("h1+p+p")) != [2] || hits(&sel("body > h1 ~ div + p")) != [4] {
        return Err("'+' / '~' birikmalari notöğri");
    }
    n += 4;
    // 3) :nth-child, :first-child, :last-child, :only-child, :nth-last-child, :nth-of-type
    let row = |k: u32| ElRef { nth: k, after: 6 - k, ..e("tr", &none) };
    let nth = |s: &str| -> Vec<u32> { let c = sel(s); (1..=6).filter(|&k| matches(&c, &row(k), &[])).collect() };
    if nth("tr:nth-child(2n+1)") != [1, 3, 5] || nth("tr:nth-child(odd)") != [1, 3, 5] || nth(":nth-child(even)") != [2, 4, 6] {
        return Err(":nth-child(2n+1) / odd / even notöğri");
    }
    if nth(":nth-child(-n+3)") != [1, 2, 3] || nth(":nth-child( 3n - 1 )") != [2, 5] || nth(":nth-child(4)") != [4] || nth(":nth-child(n+5)") != [5, 6] {
        return Err(":nth-child(-n+3) / an-b notöğri");
    }
    if nth(":first-child") != [1] || nth(":last-child") != [6] || nth(":nth-last-child(2)") != [5] || !nth(":only-child").is_empty() {
        return Err(":first-child / :last-child / :nth-last-child notöğri");
    }
    if hits(&sel("p:first-of-type")) != [1] || hits(&sel("p:nth-of-type(3)")) != [4] {
        return Err(":nth-of-type notöğri");
    }
    // nomaʼlum örin (nth = 0, after = MAX) — tuzilma psevdo-klasslari mos kelmaydi
    if matches(&sel("li:first-child"), &li, &[]) || matches(&sel("li:last-child"), &li, &[]) {
        return Err("nomaʼlum örinli element :first-child ga mos keldi");
    }
    n += 5;
    // 4) atributlar
    let h1 = at("href", "https://zamin.app/docs.pdf");
    let a = ElRef { attrs: &h1, ..e("a", &none) };
    let t1 = at("type", "TEXT");
    let inp = ElRef { attrs: &t1, ..e("input", &none) };
    let ok = |s: &str, el: &ElRef| matches(&sel(s), el, &[]);
    if !ok("[href^=\"https\"]", &a) || ok("[href^='http:']", &a) || !ok("a[href$=\".pdf\"]", &a) || !ok("[href*=zamin]", &a) || !ok("[href]", &a) || ok("[title]", &a) {
        return Err("[href^=…] / $= / *= / [atr] notöğri");
    }
    if !ok("input[type=text i]", &inp) || ok("input[type=text]", &inp) || !ok("[type=\"TEXT\" s]", &inp) {
        return Err("[type=text i] registrsiz moslaş notöğri");
    }
    let l1 = at("lang", "uz-Latn");
    let cl = alloc::vec![String::from("btn"), String::from("katta")];
    let x = ElRef { attrs: &l1, ..e("p", &cl) };
    if !ok("[lang|=uz]", &x) || ok("[lang|=u]", &x) || !ok("[class~=katta]", &x) || ok("[class~=kat]", &x) {
        return Err("[lang|=…] / [class~=…] notöğri");
    }
    n += 4;
    // 5) :not() va specificity
    let fa = alloc::vec![String::from("karta"), String::from("faol")];
    let k1 = alloc::vec![String::from("karta")];
    let (cf, ck) = (e("div", &fa), e("div", &k1));
    let nt = sel(".karta:not(.faol)");
    if matches(&nt, &cf, &[]) || !matches(&nt, &ck, &[]) || !matches(&sel("div:not(.a, .b, [x])"), &ck, &[]) || matches(&sel(":not(div)"), &ck, &[]) {
        return Err(":not() notöğri");
    }
    if specificity(&sel(":not(.a)")) != specificity(&sel(".a")) || specificity(&sel("li:not(#x)")) != specificity(&sel("li#x")) {
        return Err(":not(.a) specificity'si .a nikiga teng emas");
    }
    if !(specificity(&sel("#a")) > specificity(&sel(".a.b.c")) && specificity(&sel(".a")) > specificity(&sel("ul li p")) && specificity(&sel("[x]")) == specificity(&sel(":first-child"))) {
        return Err("specificity tartibi notöğri");
    }
    n += 3;
    // 6) noto'g'ri tanlagiçlar — panic yöq, faqat öşa qoida taşlanadi
    let rules = parse_sheet(
        "p { color: #010101 } a[ { color: red } li:nth-child(2n+) { color: red } .x::before { content: 'y' }
         > p { color: red } p + { color: red } :not(p > a) { color: red } :nth-child(foo) { color: red }
         [=x] { color: red } a[href=\"x { color: red } .ok, p ~ ~ p { color: #020202 } div:hover:not(.q) { color: #030303 }",
    );
    if rules.iter().any(|r| r.decls.iter().any(|(_, v)| v == "red") || r.sel.iter().any(|c| c.is_empty())) {
        return Err("noto'g'ri tanlagiç qoidasi qabul qilindi yoki to'g'risi taşlandi");
    }
    let good = rules.iter().filter(|r| r.decls.iter().any(|(_, v)| v.starts_with("#0"))).count();
    if good != 3 || rules.iter().find(|r| r.decls[0].1 == "#020202").is_none_or(|r| r.sel.len() != 1) {
        return Err("yaroqli qoidalar (yoki vergulli ro'yxatning to'g'ri qismi) yöqoldi");
    }
    for bad in ["::", ":not(", "[a", "a > > b", ":nth-child()", "*:not(:not(:not(:not(:not(:not(a))))))", "a,,b", "[a=\"b]", "\u{ff}:\u{ff}"] {
        let _ = parse_selectors(bad);
    }
    n += 2;
    // 7) kaskad: yuqori specificity keyinroq e'lon qilingandan ustun; teng bölsa — tartib
    let rules = parse_sheet("ul li { color: #111111 } li { color: #222222 } .z { font-size: 12px } li.z { font-size: 14px } .z { font-size: 16px }");
    let z = alloc::vec![String::from("z")];
    let s = compute(&Sty::root(), &rules, &e("li", &z), &[e("ul", &none)], None);
    if s.color != Some(0x111111) {
        return Err("kaskad: 'ul li' 'li' dan ustun bölmadi");
    }
    if (s.size - 14.0).abs() > 0.01 {
        return Err("kaskad: 'li.z' '.z' dan ustun bölmadi");
    }
    n += 1;
    // 8) HTML bilan: zebra jadval, 'li + li', :last-child (oldindan körish), input[type=password]
    let d = crate::html::parse(
        "<style>tr:nth-child(odd) td{color:#aa0000} tr:nth-child(even) td{color:#00aa00} li+li{color:#0000aa}
         li:last-child{font-weight:bold} input[type=password]{background:#fff0f0} .k:not(.f){color:#123456}</style>
         <table><tr><td>R1</td></tr><tr><td>R2</td></tr><tr><td>R3</td></tr></table>
         <ul><li>L1<ul><li>N1</li><li>N2</li></ul></li><li>L2</li><li>L3</ul>
         <p class=k>K1</p><p class='k f'>K2</p><input type=text><input type=password>",
    );
    let sp = |t: &str| d.blocks.iter().flat_map(|b| b.spans.iter()).find(|s| s.text.trim() == t).map(|s| (s.color, s.bold));
    if sp("R1").map(|s| s.0) != Some(Some(0xaa0000)) || sp("R2").map(|s| s.0) != Some(Some(0x00aa00)) || sp("R3").map(|s| s.0) != Some(Some(0xaa0000)) {
        return Err("HTML: zebra jadval (tr:nth-child) qöllanmadi");
    }
    if sp("L1").map(|s| s.0) == Some(Some(0x0000aa)) {
        return Err("HTML: 'li + li' birinçi li ga qöllandi");
    }
    if sp("L2").map(|s| s.0) != Some(Some(0x0000aa)) {
        return Err("HTML: 'li + li' ikkinçi li ga qöllanmadi");
    }
    if sp("N1").map(|s| s.0) == Some(Some(0x0000aa)) || sp("N2").map(|s| s.0) != Some(Some(0x0000aa)) {
        return Err("HTML: 'li + li' ichma-ich birinçi li ga qöllandi");
    }
    if sp("L3").map(|s| s.1) != Some(true) || sp("L1").map(|s| s.1) != Some(false) || sp("N1").map(|s| s.1) != Some(false) || sp("N2").map(|s| s.1) != Some(true) {
        return Err("HTML: li:last-child (oldindan körish) notöğri");
    }
    if sp("K1").map(|s| s.0) != Some(Some(0x123456)) || sp("K2").map(|s| s.0) == Some(Some(0x123456)) {
        return Err("HTML: .k:not(.f) notöğri");
    }
    if d.inputs.len() != 2 || d.inputs[0].bg.is_some() || d.inputs[1].bg != Some(0xfff0f0) {
        return Err("HTML: input[type=password] uslubi qöllanmadi");
    }
    n += 5;
    Ok(n)
}

/// O'z-özini tekşiruv: display: inline-block, vertical-align va visibility (joy saqlanadi, meros).
pub fn self_test_inline() -> Result<usize, &'static str> {
    let mut n = 0;
    let rules = parse_sheet(".nav a { display: inline-block; width: 120px; padding: 6px; vertical-align: middle } .yoq { visibility: hidden }");
    let none: [String; 0] = [];
    let nav = [String::from("nav")];
    let yoq = [String::from("yoq")];
    fn mk<'a>(tag: &'a str, c: &'a [String]) -> ElRef<'a> {
        ElRef { tag, id: "", classes: c, ..ElRef::default() }
    }
    let root = Sty::root();
    // inline-block: blok kabi quti (o'lçamlari bilan), lekin qatorda
    let a = compute(&root, &rules, &mk("a", &none), &[mk("div", &nav)], None);
    if !a.ib || a.block != Some(true) || a.maxw != Some((120.0, false)) || a.pad != 6.0 || a.valign != 1 || a.hidden {
        return Err("display: inline-block / width / padding / vertical-align xato");
    }
    n += 1;
    // keyingi display qiymati inline-block ni bekor qiladi; inline-block meros bölmaydi
    let b = compute(&root, &[], &mk("span", &none), &[], Some("display: inline-block; display: inline"));
    let kid = compute(&a, &[], &mk("b", &none), &[], None);
    if b.ib || b.block != Some(false) || kid.ib || kid.valign != 0 {
        return Err("inline-block bekor qiliş yoki meros xato");
    }
    n += 1;
    // visibility: hidden — display: none emas (joy qoladi), bolaga meros, visible bilan qaytadi
    let h = compute(&root, &rules, &mk("span", &yoq), &[], None);
    let hk = compute(&h, &[], &mk("i", &none), &[], None);
    let hv = compute(&h, &[], &mk("i", &none), &[], Some("visibility: visible"));
    let c = compute(&root, &[], &mk("span", &none), &[], Some("visibility: collapse"));
    if !h.invis || h.hidden || !hk.invis || hv.invis || !c.invis || c.hidden {
        return Err("visibility: hidden / collapse / visible xato");
    }
    n += 1;
    Ok(n)
}
