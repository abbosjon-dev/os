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
    pub radius: f32,                  // border-radius
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
            mt: 0.0, mb: 0.0, pad: 0.0, bw: 0.0, bc: None, radius: 0.0, maxw: None, center: false,
            flex: 0, gap: 0.0, gcols: 0, justify: 0, grow: 0.0,
            pos: 0, ins: [None; 4], float: 0, clear: 0, grad: None, shadow: None,
        }
    }
    /// Meros: rang, ölçam, qalinlik, tekislaş, yaşiriş — bolaga ötadi; fon — ötmaydi.
    pub fn inherit(&self) -> Sty {
        Sty {
            bg: None, block: None, mt: 0.0, mb: 0.0, pad: 0.0, bw: 0.0, bc: None, radius: 0.0, maxw: None, center: false,
            flex: 0, gap: 0.0, gcols: 0, justify: 0, grow: 0.0,
            pos: 0, ins: [None; 4], float: 0, clear: 0, grad: None, shadow: None,
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
    pub direct: bool, // keyingi (öngdagi) bilan ">" — bevosita ota
    pub hover: bool,  // :hover — siçqonça ostidagi element (yoki uning ajdodi)
}

#[derive(Clone)]
pub struct Rule {
    pub sel: Vec<Vec<Compound>>, // vergul bilan ajratilgan tanlagiçlar; har biri — avlod zanjiri
    pub decls: Vec<(String, String)>,
    pub media: Vec<Vec<Mq>>,     // @media şartlari: tașqi Vec — VA (ichma-ich), içki — YOKI (vergul)
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
    let me = ElRef { tag, id, classes, hover: true };
    rules.iter().any(|r| {
        r.sel.iter().any(|ch| ch.iter().any(|c| c.hover) && ch.last().is_some_and(|l| compound_match(l, &me)))
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

fn parse_compound(s: &str) -> Option<Compound> {
    let mut c = Compound::default();
    let mut i = 0;
    let b = s.as_bytes();
    let ident = |i: &mut usize| -> String {
        let st = *i;
        while *i < b.len() && (b[*i].is_ascii_alphanumeric() || b[*i] == b'-' || b[*i] == b'_') {
            *i += 1;
        }
        String::from(&s[st..*i])
    };
    while i < b.len() {
        match b[i] {
            b'.' => {
                i += 1;
                c.classes.push(ident(&mut i));
            }
            b'#' => {
                i += 1;
                c.id = Some(ident(&mut i));
            }
            b'*' => i += 1,
            b':' => {
                // Zararsiz psevdo-klasslar e'tiborsiz; holatga bogʻliqlari (:hover…) — qoida taşlanadi
                let rest = &s[i + 1..];
                let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
                if matches!(name.as_str(), "link" | "visited" | "root" | "any-link") {
                    i += 1 + name.len();
                } else if name == "hover" {
                    c.hover = true;
                    i += 1 + name.len();
                } else {
                    return None;
                }
            }
            b'[' => return None, // atribut tanlagiçi — qöllanmaydi
            _ => {
                let t = ident(&mut i);
                if t.is_empty() {
                    return None;
                }
                c.tag = Some(t.to_ascii_lowercase());
            }
        }
    }
    Some(c)
}

/// "a, b c" — tanlagiçlar ro'yxati.
fn parse_selectors(s: &str) -> Vec<Vec<Compound>> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() || part.contains('+') || part.contains('~') {
            continue;
        }
        let spaced = part.replace('>', " > ");
        let mut chain: Option<Vec<Compound>> = Some(Vec::new());
        for tok in spaced.split_whitespace() {
            let Some(ch) = chain.as_mut() else { break };
            if tok == ">" {
                if let Some(last) = ch.last_mut() {
                    last.direct = true;
                }
                continue;
            }
            match parse_compound(tok) {
                Some(c) => ch.push(c),
                None => chain = None,
            }
        }
        if let Some(ch) = chain {
            if !ch.is_empty() {
                out.push(ch);
            }
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
            rules.push(Rule { sel, decls, media: Vec::new() });
        }
        rest = &rest_t[cb + 1..];
    }
}

/// Element: (teg, id, klasslar) — tanlagiçga moslaş uçun.
pub struct ElRef<'a> {
    pub tag: &'a str,
    pub id: &'a str,
    pub classes: &'a [String],
    pub hover: bool, // siçqonça şu element (yoki uning avlodi) ustidami
}

fn compound_match(c: &Compound, e: &ElRef) -> bool {
    (!c.hover || e.hover)
        && c.tag.as_ref().is_none_or(|t| t == e.tag)
        && c.id.as_ref().is_none_or(|i| i == e.id)
        && c.classes.iter().all(|k| e.classes.iter().any(|x| x == k))
}

/// Tanlagiç zanjiri: oxirgisi — elementning özi, oldingilari — ajdodlarida (tartib bilan).
pub fn matches(chain: &[Compound], el: &ElRef, ancestors: &[ElRef]) -> bool {
    let Some((last, prev)) = chain.split_last() else { return false };
    if !compound_match(last, el) {
        return false;
    }
    let mut k = ancestors.len();
    for c in prev.iter().rev() {
        if c.direct {
            // "c > keyingi": bevosita ota bölişi şart
            if k == 0 || !compound_match(c, &ancestors[k - 1]) {
                return false;
            }
            k -= 1;
            continue;
        }
        loop {
            if k == 0 {
                return false;
            }
            k -= 1;
            if compound_match(c, &ancestors[k]) {
                break;
            }
        }
    }
    true
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

/// Bitta e'lonni uslubga qöllaydi. `parent` — ota shrift ölçami.
pub fn apply(s: &mut Sty, prop: &str, val: &str, parent: f32) {
    let low = val.to_ascii_lowercase();
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
                "inline" | "inline-block" | "inline-flex" => Some(false),
                _ => s.block,
            };
        }
        "visibility" => s.hidden = low == "hidden" || low == "collapse",
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
            let top = p.first().and_then(|x| length(x, s.size)).unwrap_or(0.0);
            let bot = p.get(2).or(p.first()).and_then(|x| length(x, s.size)).unwrap_or(top);
            s.mt = top.clamp(0.0, 200.0);
            s.mb = bot.clamp(0.0, 200.0);
            let horiz = if p.len() >= 2 { p[1] } else { p.first().copied().unwrap_or("") };
            s.center = horiz == "auto";
        }
        "margin-top" => s.mt = length(&low, s.size).unwrap_or(0.0).clamp(0.0, 200.0),
        "margin-bottom" => s.mb = length(&low, s.size).unwrap_or(0.0).clamp(0.0, 200.0),
        "margin-left" | "margin-right" | "margin-inline" if low == "auto" => s.center = true,
        "padding" | "padding-top" | "padding-left" | "padding-inline" | "padding-block" => {
            if let Some(v) = low.split_whitespace().next().and_then(|x| length(x, s.size)) {
                s.pad = s.pad.max(v.clamp(0.0, 80.0));
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
        "border-radius" => s.radius = low.split_whitespace().next().and_then(|x| length(x, s.size)).unwrap_or(0.0).clamp(0.0, 60.0),
        "width" | "max-width" => {
            if let Some(p) = low.strip_suffix('%') {
                if let Some(f) = parse_f(p) {
                    s.maxw = Some((f.clamp(1.0, 100.0), true));
                }
            } else if let Some(px) = length(&low, s.size).filter(|v| *v > 0.0) {
                s.maxw = Some((px, false));
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
        "text-decoration" | "text-decoration-line" => {
            s.underline = Some(low.contains("underline"));
            s.strike = low.contains("line-through");
        }
        _ => {}
    }
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
    let mut s = parent.inherit();
    for r in rules {
        if !media_ok(&r.media) {
            continue;
        }
        if r.sel.iter().any(|ch| matches(ch, el, ancestors)) {
            for (k, v) in &r.decls {
                apply(&mut s, k, v, parent.size);
            }
        }
    }
    if let Some(st) = inline {
        for (k, v) in parse_decls(st) {
            apply(&mut s, &k, &v, parent.size);
        }
    }
    s
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
