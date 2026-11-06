//! CSS — noldan: uslublar jadvali tahlili (qoidalar, tanlagiçlar, e'lonlar),
//! ranglar, shrift ölçamlari va hisoblangan uslub (meros bilan).
//! Tanlagiçlar: teg, .klass, #id, * va avlod (bo'şliq) birikmasi; @-qoidalar taşlanadi.

use alloc::string::String;
use alloc::vec::Vec;

/// Hisoblangan uslub (element uçun).
#[derive(Clone, Copy, PartialEq)]
pub struct Sty {
    pub color: Option<u32>,
    pub bg: Option<u32>,
    pub size: f32, // CSS piksel (16 — oddiy)
    pub bold: bool,
    pub italic: bool,
    pub align: u8, // 0 — çap, 1 — markaz, 2 — öng
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
}

impl Sty {
    pub const fn root() -> Sty {
        Sty {
            color: None, bg: None, size: 16.0, bold: false, italic: false, align: 0, hidden: false, underline: None, strike: false, block: None,
            mt: 0.0, mb: 0.0, pad: 0.0, bw: 0.0, bc: None, radius: 0.0, maxw: None, center: false,
            flex: 0, gap: 0.0, gcols: 0, justify: 0, grow: 0.0,
            pos: 0, ins: [None; 4], float: 0, clear: 0,
        }
    }
    /// Meros: rang, ölçam, qalinlik, tekislaş, yaşiriş — bolaga ötadi; fon — ötmaydi.
    pub fn inherit(&self) -> Sty {
        Sty {
            bg: None, block: None, mt: 0.0, mb: 0.0, pad: 0.0, bw: 0.0, bc: None, radius: 0.0, maxw: None, center: false,
            flex: 0, gap: 0.0, gcols: 0, justify: 0, grow: 0.0,
            pos: 0, ins: [None; 4], float: 0, clear: 0,
            ..*self
        }
    }
    /// Konteynermi (fon, çegara yoki kenglik çeklovi bor)?
    pub fn is_box(&self) -> bool {
        self.bg.is_some() || self.bw > 0.0 || self.maxw.is_some() || self.pad > 0.0
    }
}

#[derive(Clone, Default)]
pub struct Compound {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub direct: bool, // keyingi (öngdagi) bilan ">" — bevosita ota
}

#[derive(Clone)]
pub struct Rule {
    pub sel: Vec<Vec<Compound>>, // vergul bilan ajratilgan tanlagiçlar; har biri — avlod zanjiri
    pub decls: Vec<(String, String)>,
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
                if matches!(name.as_str(), "link" | "visited" | "root") {
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

/// Uslublar jadvali (@-qoidalar va ularning bloklari taşlanadi).
pub fn parse_sheet(src: &str) -> Vec<Rule> {
    let src = strip_comments(src);
    let mut rules = Vec::new();
    let mut rest = src.as_str();
    loop {
        let rest_t = rest.trim_start();
        if rest_t.is_empty() {
            break;
        }
        if rest_t.starts_with('@') {
            // @import ...; yoki @media {...} — taşlaymiz (ichma-ich qavslar bilan)
            let semi = rest_t.find(';');
            let brace = rest_t.find('{');
            match (semi, brace) {
                (Some(s), Some(b)) if s < b => {
                    rest = &rest_t[s + 1..];
                    continue;
                }
                (_, Some(b)) => {
                    let mut depth = 0;
                    let mut end = rest_t.len();
                    for (k, ch) in rest_t[b..].char_indices() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                end = b + k + 1;
                                break;
                            }
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
            rules.push(Rule { sel, decls });
        }
        rest = &rest_t[cb + 1..];
    }
    rules
}

/// Element: (teg, id, klasslar) — tanlagiçga moslaş uçun.
pub struct ElRef<'a> {
    pub tag: &'a str,
    pub id: &'a str,
    pub classes: &'a [String],
}

fn compound_match(c: &Compound, e: &ElRef) -> bool {
    c.tag.as_ref().is_none_or(|t| t == e.tag)
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
        "background-color" | "background" => {
            if low == "none" || low == "transparent" {
                s.bg = None;
            } else if let Some(c) = parse_color(val).or_else(|| val.split_whitespace().find_map(parse_color)) {
                s.bg = Some(c);
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
                "center" => 1,
                "right" | "end" => 2,
                _ => 0,
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

/// Uslublar jadvali va inline uslubdan elementning hisoblangan uslubi.
pub fn compute(parent: &Sty, rules: &[Rule], el: &ElRef, ancestors: &[ElRef], inline: Option<&str>) -> Sty {
    let mut s = parent.inherit();
    for r in rules {
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
