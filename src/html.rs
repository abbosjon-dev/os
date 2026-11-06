//! Kiçik HTML tahlilçisi — noldan. Hujjatni bloklar (sarlavha, xatboşi, ro'yxat,
//! kod, iqtibos, chiziq) va ularning ichidagi matn bölaklariga (havola, qalin,
//! kod) aylantiradi. <script>/<style> taşlanadi, belgilar (&amp; &#NN; …) yeçiladi.

use alloc::string::String;
use alloc::vec::Vec;

use crate::css::{self, ElRef, Rule, Sty};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    H(u8),
    P,
    Li(u8, u16), // çuqurlik, tartib raqami (0 — nuqta)
    Pre,
    Quote,
    Hr,
    Img,
}

#[derive(Clone)]
pub struct Span {
    pub text: String,
    pub link: Option<String>,
    pub bold: bool,
    pub code: bool,
    pub br: bool, // qator uziliş
    pub color: Option<u32>,
    pub size: f32, // CSS piksel
    pub el: u16,   // eng yaqin ro'yxatdagi element (id / onclick) — JS uçun
    pub button: bool,
    pub underline: bool,
    pub input: u16, // forma maydoni (Doc::inputs indeksi) yoki NO_EL
}

#[derive(Clone)]
pub struct Block {
    pub kind: Kind,
    pub spans: Vec<Span>,
    pub bg: Option<u32>,
    pub align: u8,
    pub size: f32,
    pub color: Option<u32>,
    pub el: u16,
    pub src: String, // rasm manzili (Img)
    pub hidden: bool,
    pub bgk: u32,    // fonni bergan element (qöşni bloklar bitta fon bilan birlaşadi)
    // Quti modeli: konteynerdan (pad, çegara, radius, kenglik) va blokning özidan (chekinişlar)
    pub pad: f32,
    pub bw: f32,
    pub bc: Option<u32>,
    pub radius: f32,
    pub maxw: Option<(f32, bool)>,
    pub center: bool,
    pub mt: f32,
    pub mb: f32,
    pub bmt: f32, // konteynerning öz chekinişlari
    pub bmb: f32,
    // Flex / grid / jadval: konteyner va ustun (bevosita bola) raqamlari
    pub flex: u32,
    pub fcol: u32,
    pub fmode: u8, // 1 — qator (flex/jadval), 3 — grid
    pub fgap: f32,
    pub gcols: u8,
    pub fjust: u8,
    pub fw: Option<(f32, bool)>, // ustun kengligi (bolaning width)
    pub fgrow: f32,
}

/// Forma maydoni.
#[derive(Clone)]
pub struct Input {
    pub kind: u8, // 0 — matn, 1 — yuboriş tugmasi, 2 — belgilaş (checkbox), 3 — parol, 4 — yaşirin, 5 — <button>
    pub name: String,
    pub value: String,
    pub placeholder: String,
    pub form: u16,
    pub el: u16,
    pub size: u16,
    pub checked: bool,
    pub options: Vec<(String, String)>, // <select>: (qiymat, yozuv)
}

#[derive(Clone)]
pub struct Form {
    pub action: String,
    pub method: String,
    pub el: u16, // JS (onsubmit / addEventListener('submit')) uçun
}

/// JS körinadigan element (id yoki onclick bölganlar).
#[derive(Clone)]
pub struct ElInfo {
    pub id: String,
    pub tag: String,
    pub onclick: Option<String>,
    pub classes: Vec<String>,
    pub parent: u16,
}

#[derive(Clone)]
pub enum Script {
    Inline(String),
    Src(String),
}

pub struct Doc {
    pub title: String,
    pub blocks: Vec<Block>,
    pub elems: Vec<ElInfo>,
    pub scripts: Vec<Script>,
    pub bg: Option<u32>,
    pub color: Option<u32>,
    pub inputs: Vec<Input>,
    pub forms: Vec<Form>,
    pub rules: Vec<Rule>, // classList özgarişida qayta uslublaş uçun
}

pub const NO_EL: u16 = u16::MAX;

impl Doc {
    pub fn empty() -> Doc {
        Doc { title: String::new(), blocks: Vec::new(), elems: Vec::new(), scripts: Vec::new(), bg: None, color: None, inputs: Vec::new(), forms: Vec::new(), rules: Vec::new() }
    }
    /// Elementga tegişli forma maydoni (input / textarea).
    pub fn input_of(&self, el: usize) -> Option<usize> {
        self.inputs.iter().position(|i| i.el as usize == el)
    }
    pub fn find_id(&self, id: &str) -> Option<usize> {
        self.elems.iter().position(|e| e.id == id)
    }
    /// `a` element `e` içidami (yoki özimi)?
    pub fn within(&self, a: u16, e: u16) -> bool {
        let mut cur = a;
        let mut n = 0;
        while cur != NO_EL && n < 512 {
            if cur == e {
                return true;
            }
            cur = self.elems.get(cur as usize).map(|x| x.parent).unwrap_or(NO_EL);
            n += 1;
        }
        false
    }
    /// Elementning matni (barça avlodlari bilan).
    pub fn text_of(&self, el: usize) -> String {
        let e = el as u16;
        let mut out = String::new();
        for b in &self.blocks {
            for s in b.spans.iter().filter(|s| self.within(s.el, e)) {
                out.push_str(&s.text);
            }
        }
        out
    }
    /// Element tarkibini almaştiradi: matn (textContent) yoki HTML (innerHTML).
    pub fn set_content(&mut self, el: usize, text: &str, html: bool) {
        let e = el as u16;
        // 1) Blok darajasidagi element: içidagi barça bloklar → yangisi
        let range: Vec<usize> = (0..self.blocks.len()).filter(|&k| self.within(self.blocks[k].el, e)).collect();
        if let (Some(&first), Some(&last)) = (range.first(), range.last()) {
            let is_block = self.blocks[first].el == e || range.len() > 1 || self.blocks[first].spans.iter().all(|s| self.within(s.el, e));
            if is_block {
                let tmpl = self.blocks[first].clone();
                let base_el = tmpl.el;
                let mut new_blocks: Vec<Block> = if html {
                    let mut d = parse_frag(text, &self.rules);
                    let off = self.elems.len() as u16;
                    for x in d.elems.iter_mut() {
                        x.parent = if x.parent == NO_EL { e } else { x.parent + off };
                    }
                    let remap = |v: u16| if v == NO_EL { e } else { v + off };
                    for b in d.blocks.iter_mut() {
                        b.el = remap(b.el);
                        for sp in b.spans.iter_mut() {
                            sp.el = remap(sp.el);
                        }
                        if b.bg.is_none() {
                            b.bg = tmpl.bg;
                            b.bgk = tmpl.bgk;
                        }
                        b.flex = tmpl.flex;
                        b.fcol = tmpl.fcol;
                        b.fmode = tmpl.fmode;
                        b.fgap = tmpl.fgap;
                        b.gcols = tmpl.gcols;
                        b.fw = tmpl.fw;
                        b.pad = tmpl.pad;
                        b.bw = tmpl.bw;
                        b.bc = tmpl.bc;
                        b.radius = tmpl.radius;
                        b.maxw = tmpl.maxw;
                        b.center = tmpl.center;
                        if b.color.is_none() {
                            b.color = tmpl.color;
                        }
                    }
                    self.elems.extend(d.elems);
                    d.blocks
                } else {
                    Vec::new()
                };
                if new_blocks.is_empty() {
                    let mut b = tmpl.clone();
                    let sp = tmpl.spans.iter().find(|s| !s.br && s.input == NO_EL).cloned().unwrap_or(Span {
                        text: String::new(), link: None, bold: false, code: false, br: false, color: None, size: tmpl.size, el: base_el, button: false, underline: false, input: NO_EL,
                    });
                    let lines: Vec<&str> = text.split('\n').collect();
                    b.spans.clear();
                    for (k, l) in lines.iter().enumerate() {
                        if k > 0 {
                            b.spans.push(Span { text: String::new(), br: true, ..sp.clone() });
                        }
                        b.spans.push(Span { text: String::from(*l), br: false, el: base_el, input: NO_EL, ..sp.clone() });
                    }
                    new_blocks.push(b);
                }
                // Faqat ketma-ket oraliqni almaştiramiz
                let last = if range.windows(2).all(|w| w[1] == w[0] + 1) { last } else { first };
                self.blocks.splice(first..=last, new_blocks);
                return;
            }
        }
        // 2) Qator içidagi element (span, b, a…)
        let plain = if html { strip_tags(text) } else { String::from(text) };
        for bi in 0..self.blocks.len() {
            let pos: Vec<usize> = (0..self.blocks[bi].spans.len()).filter(|&k| self.within(self.blocks[bi].spans[k].el, e)).collect();
            if let Some(&first) = pos.first() {
                let b = &mut self.blocks[bi];
                b.spans[first].text = plain.clone();
                b.spans[first].el = e;
                let mut k = 0;
                b.spans.retain(|_| {
                    let keep = !pos.contains(&k) || k == first;
                    k += 1;
                    keep
                });
                return;
            }
        }
    }
    /// JS: element.style.X = v — elementning blok/bölaklariga qöllanadi.
    pub fn set_style(&mut self, el: usize, prop: &str, val: &str) {
        let e = el as u16;
        let prop = css::kebab(prop);
        let low = val.to_ascii_lowercase();
        for bi in 0..self.blocks.len() {
            let whole = self.within(self.blocks[bi].el, e);
            let owner = self.blocks[bi].el == e;
            let inside: Vec<bool> = self.blocks[bi].spans.iter().map(|sp| self.within(sp.el, e)).collect();
            let b = &mut self.blocks[bi];
            if whole {
                match prop.as_str() {
                    "display" => b.hidden = low == "none",
                    "background" | "background-color" if owner => b.bg = css::parse_color(val),
                    "text-align" if owner => b.align = if low == "center" { 1 } else if low == "right" { 2 } else { 0 },
                    "color" => b.color = css::parse_color(val),
                    "font-size" => {
                        let mut st = Sty::root();
                        css::apply(&mut st, "font-size", val, b.size);
                        b.size = st.size;
                    }
                    _ => {}
                }
            }
            for (k, sp) in b.spans.iter_mut().enumerate() {
                if !(whole || inside[k]) {
                    continue;
                }
                match prop.as_str() {
                    "color" => sp.color = css::parse_color(val),
                    "font-weight" => sp.bold = low == "bold" || low.parse::<u32>().is_ok_and(|n| n >= 600),
                    "font-size" => {
                        let mut st = Sty::root();
                        css::apply(&mut st, "font-size", val, sp.size);
                        sp.size = st.size;
                    }
                    _ => {}
                }
            }
        }
    }
    /// JS: appendChild / insertAdjacentHTML — HTML ni element oxiriga qöşadi.
    pub fn append_into(&mut self, el: usize, src: &str) {
        let e = el as u16;
        let last = (0..self.blocks.len()).rev().find(|&k| self.within(self.blocks[k].el, e));
        let mut d = parse_frag(src, &self.rules);
        let off = self.elems.len() as u16;
        for x in d.elems.iter_mut() {
            x.parent = if x.parent == NO_EL { e } else { x.parent + off };
        }
        let tmpl = last.map(|k| self.blocks[k].clone());
        for b in d.blocks.iter_mut() {
            b.el = if b.el == NO_EL { e } else { b.el + off };
            for sp in b.spans.iter_mut() {
                sp.el = if sp.el == NO_EL { e } else { sp.el + off };
            }
            if let Some(t) = &tmpl {
                if b.bg.is_none() {
                    b.bg = t.bg;
                    b.bgk = t.bgk;
                }
                b.pad = t.pad;
                b.bw = t.bw;
                b.bc = t.bc;
                b.radius = t.radius;
                b.maxw = t.maxw;
                b.center = t.center;
                b.flex = t.flex;
                b.fcol = t.fcol;
                b.fmode = t.fmode;
                b.fgap = t.fgap;
                b.gcols = t.gcols;
                b.fw = t.fw;
            }
        }
        self.elems.extend(d.elems);
        let at = last.map(|k| k + 1).unwrap_or(self.blocks.len());
        self.blocks.splice(at..at, d.blocks);
    }
    /// classList özgardi: elementning öz qoidalari (rang, fon, ko'rinişi) qayta qöllanadi.
    pub fn restyle(&mut self, el: usize) {
        let Some(info) = self.elems.get(el).cloned() else { return };
        let me = ElRef { tag: &info.tag, id: &info.id, classes: &info.classes };
        let st = css::compute(&Sty::root(), &self.rules, &me, &[], None);
        let e = el as u16;
        let base = Sty::root();
        for bi in 0..self.blocks.len() {
            let whole = self.blocks[bi].el == e;
            if whole {
                self.blocks[bi].hidden = st.hidden;
                if st.bg.is_some() || self.blocks[bi].bgk == 0 {
                    self.blocks[bi].bg = st.bg;
                }
            }
            for k in 0..self.blocks[bi].spans.len() {
                if whole || self.blocks[bi].spans[k].el == e {
                    let sp = &mut self.blocks[bi].spans[k];
                    sp.color = st.color;
                    sp.bold = st.bold;
                    if st.size != base.size {
                        sp.size = st.size;
                    }
                }
            }
        }
        // Yaşiriş butun avlod bloklariga
        for bi in 0..self.blocks.len() {
            if self.blocks[bi].el != e && self.within(self.blocks[bi].el, e) {
                self.blocks[bi].hidden = st.hidden;
            }
        }
    }
    /// Oddiy tanlagiç böyiça elementlar: "#id", ".klass", "teg", "teg.klass".
    pub fn query_all(&self, sel: &str) -> Vec<usize> {
        let sel = sel.trim();
        let last = sel.split_whitespace().last().unwrap_or(sel);
        let (tag, rest) = match last.find(['.', '#']) {
            Some(0) => ("", last),
            Some(i) => (&last[..i], &last[i..]),
            None => (last, ""),
        };
        let mut id = String::new();
        let mut classes: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut kind = ' ';
        for c in rest.chars().chain(core::iter::once('.')) {
            if c == '.' || c == '#' {
                if !cur.is_empty() {
                    if kind == '#' {
                        id = core::mem::take(&mut cur);
                    } else {
                        classes.push(core::mem::take(&mut cur));
                    }
                }
                cur.clear();
                kind = c;
            } else {
                cur.push(c);
            }
        }
        (0..self.elems.len())
            .filter(|&k| {
                let e = &self.elems[k];
                (tag.is_empty() || tag == "*" || e.tag == tag.to_ascii_lowercase())
                    && (id.is_empty() || e.id == id)
                    && classes.iter().all(|c| e.classes.iter().any(|x| x == c))
            })
            .collect()
    }
    /// JS: document.write — HTML bölagini oxiriga qöşadi.
    pub fn append_html(&mut self, src: &str) {
        let d = parse(src);
        self.blocks.extend(d.blocks);
    }
}

fn entity(name: &str) -> Option<char> {
    if let Some(n) = name.strip_prefix('#') {
        let v = if let Some(h) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
            u32::from_str_radix(h, 16).ok()?
        } else {
            n.parse().ok()?
        };
        return char::from_u32(v);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "mdash" => '—',
        "ndash" => '-',
        "hellip" => '…',
        "copy" => 'c',
        "laquo" => '«',
        "raquo" => '»',
        "middot" => '·',
        "deg" => '°',
        "rsquo" | "lsquo" => '\'',
        "rdquo" | "ldquo" => '"',
        _ => return None,
    })
}

pub fn decode_entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.find(';').filter(|&j| j <= 10).and_then(|j| entity(&tail[..j]).map(|c| (c, j))) {
            Some((c, j)) => {
                out.push(c);
                rest = &tail[j + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let low = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(p) = low[from..].find(name) {
        let i = from + p;
        let before_ok = i == 0 || low.as_bytes()[i - 1].is_ascii_whitespace();
        let after = low[i + name.len()..].trim_start();
        if before_ok && after.starts_with('=') {
            let off = tag.len() - after.len() + 1;
            let v = tag[off..].trim_start();
            let val = if let Some(r) = v.strip_prefix('"') {
                r.split('"').next().unwrap_or("")
            } else if let Some(r) = v.strip_prefix('\'') {
                r.split('\'').next().unwrap_or("")
            } else {
                v.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("")
            };
            return Some(decode_entities(val));
        }
        from = i + name.len();
    }
    None
}

/// Brauzerning öz uslublari (UA stylesheet).
const UA_CSS: &str = "h1{font-size:2em;font-weight:bold}h2{font-size:1.5em;font-weight:bold}h3{font-size:1.17em;font-weight:bold}\
h4,h5,h6{font-weight:bold}b,strong,th{font-weight:bold}small{font-size:.83em}big{font-size:1.2em}center{text-align:center}\
a{text-decoration:underline}pre,code,kbd,samp,tt{font-size:.9em}button{font-weight:bold}\
tr{display:flex}td,th{padding:3px}th{font-weight:bold}";

const VOID: [&str; 12] = ["br", "hr", "img", "meta", "link", "input", "area", "base", "col", "embed", "source", "wbr"];

struct Open {
    tag: String,
    id: String,
    classes: Vec<String>,
    sty: Sty,
    el: u16,
    link: Option<String>,
    serial: u32,
    input: u16,
}

struct P {
    doc: Doc,
    rules: Vec<Rule>,
    stack: Vec<Open>,
    cur: Option<Block>,
    pre: u32,
    lists: Vec<(bool, u16)>,
    in_title: bool,
    serial: u32,
    form: u16,
}

impl P {
    fn top(&self) -> Sty {
        self.stack.last().map(|o| o.sty).unwrap_or(Sty::root())
    }
    fn el(&self) -> u16 {
        self.stack.last().map(|o| o.el).unwrap_or(NO_EL)
    }
    fn hidden(&self) -> bool {
        self.top().hidden
    }
    fn flush(&mut self) {
        if let Some(b) = self.cur.take() {
            let has = b.spans.iter().any(|s| !s.text.trim().is_empty() || s.input != NO_EL);
            let target = b.el != NO_EL && self.doc.elems.get(b.el as usize).is_some_and(|e| !e.id.is_empty());
            if has || target || b.kind == Kind::Hr || b.kind == Kind::Img {
                self.doc.blocks.push(b);
            }
        }
    }
    fn new_block(&self, k: Kind) -> Block {
        let st = self.top();
        // Fon: eng yaqin fonli ajdod (body/html dan taşqari) — butun konteyner bölib bo'yaladi
        // Eng yaqin flex/grid konteyner va uning bevosita bolasi (ustun)
        let fl = match self.stack.iter().rposition(|o| o.sty.flex == 1 || o.sty.flex == 3) {
            Some(k) => {
                let c = &self.stack[k];
                let (col, fw, grow) = match self.stack.get(k + 1) {
                    Some(ch) => (ch.serial, ch.sty.maxw, ch.sty.grow),
                    None => (c.serial, None, 0.0),
                };
                (c.serial, col, c.sty.flex, c.sty.gap, c.sty.gcols, c.sty.justify, fw, grow)
            }
            None => (0, 0, 0, 0.0, 0, 0, None, 0.0),
        };
        // Quti: flex konteynerdan içkaridagisi (aks holda butun qator bitta qutiga tuşib qoladi)
        let fk = self.stack.iter().rposition(|o| o.sty.flex == 1 || o.sty.flex == 3).map(|k| k + 1).unwrap_or(0);
        let cont = self.stack[fk.min(self.stack.len())..].iter().rev().take_while(|o| o.tag != "body" && o.tag != "html").find(|o| o.sty.is_box());
        let (bg, bgk, cs) = match cont {
            Some(o) => (o.sty.bg, o.serial, o.sty),
            None => (None, 0, Sty::root()),
        };
        Block {
            kind: k, spans: Vec::new(), bg, align: st.align, size: st.size, color: st.color, el: self.el(), src: String::new(), hidden: st.hidden, bgk,
            pad: cs.pad, bw: cs.bw, bc: cs.bc, radius: cs.radius, maxw: cs.maxw, center: cs.center, mt: st.mt, mb: st.mb,
            bmt: cs.mt, bmb: cs.mb,
            flex: fl.0, fcol: fl.1, fmode: fl.2, fgap: fl.3, gcols: fl.4, fjust: fl.5, fw: fl.6, fgrow: fl.7,
        }
    }
    fn start(&mut self, k: Kind) {
        self.flush();
        // display:none bölsa ham blok yaratiladi (hidden) — JS keyin körsatişi mumkin
        self.cur = Some(self.new_block(k));
    }
    fn text(&mut self, t: &str) {
        if self.in_title {
            self.doc.title.push_str(&decode_entities(t));
            return;
        }
        // Körinadigan blok içidagi yaşirin qator elementi — taşlanadi
        if self.hidden() && self.cur.as_ref().is_some_and(|b| !b.hidden) {
            return;
        }
        let mut s = decode_entities(t);
        if self.pre == 0 {
            let mut c = String::new();
            let mut sp = false;
            for ch in s.chars() {
                if ch.is_whitespace() {
                    if !sp {
                        c.push(' ');
                    }
                    sp = true;
                } else {
                    c.push(ch);
                    sp = false;
                }
            }
            s = c;
            if s.trim().is_empty() && self.cur.as_ref().is_none_or(|b| b.spans.is_empty()) {
                return;
            }
        }
        if self.cur.is_none() {
            let k = if self.pre > 0 { Kind::Pre } else { Kind::P };
            self.cur = Some(self.new_block(k));
        }
        let st = self.top();
        let link = self.stack.iter().rev().find_map(|o| o.link.clone());
        let code = self.pre > 0 || self.stack.iter().any(|o| matches!(o.tag.as_str(), "code" | "kbd" | "samp" | "tt"));
        let button = self.stack.iter().any(|o| o.tag == "button");
        let input = self.stack.iter().rev().find(|o| o.input != NO_EL).map(|o| o.input).unwrap_or(NO_EL);
        let el = self.el();
        let b = self.cur.as_mut().unwrap();
        if b.spans.is_empty() && self.pre == 0 {
            s = String::from(s.trim_start());
        }
        let underline = st.underline.unwrap_or(false) && link.is_some();
        b.spans.push(Span { text: s, link, bold: st.bold, code, br: false, color: st.color, size: st.size, el, button, underline, input });
    }
    /// Forma maydonini ro'yxatga oladi. `el` — tayyor element (yöq bölsa id/onclick dan yaratiladi).
    fn add_input(&mut self, kind: u8, tag: &str, value: String, el: Option<u16>) -> u16 {
        let el = el.unwrap_or_else(|| {
            let id = attr(tag, "id").unwrap_or_default();
            let onclick = attr(tag, "onclick");
            let classes: Vec<String> = attr(tag, "class").map(|c| c.split_whitespace().map(String::from).collect()).unwrap_or_default();
            let parent = self.el();
            self.doc.elems.push(ElInfo { id, tag: String::from("input"), onclick, classes, parent });
            (self.doc.elems.len() - 1) as u16
        });
        let size = attr(tag, "size").and_then(|v| v.parse().ok()).unwrap_or(if kind == 0 || kind == 3 { 20 } else { 0 });
        self.doc.inputs.push(Input {
            kind,
            name: attr(tag, "name").unwrap_or_default(),
            value,
            placeholder: attr(tag, "placeholder").unwrap_or_default(),
            form: self.form,
            el,
            size,
            checked: attr(tag, "checked").is_some() || tag.split_whitespace().any(|w| w.eq_ignore_ascii_case("checked")),
            options: Vec::new(),
        });
        (self.doc.inputs.len() - 1) as u16
    }
    /// Maydonni joriy blokka qöyadi.
    fn input_span(&mut self, idx: u16) {
        if self.doc.inputs[idx as usize].kind == 4 || (self.hidden() && self.cur.as_ref().is_some_and(|b| !b.hidden)) {
            return;
        }
        if self.cur.is_none() {
            self.cur = Some(self.new_block(Kind::P));
        }
        let st = self.top();
        let el = self.doc.inputs[idx as usize].el;
        let b = self.cur.as_mut().unwrap();
        b.spans.push(Span { text: String::from("\u{2007}"), link: None, bold: false, code: false, br: false, color: st.color, size: st.size, el, button: false, underline: false, input: idx });
    }
    fn br(&mut self) {
        if let Some(b) = self.cur.as_mut() {
            let size = b.size;
            b.spans.push(Span { text: String::new(), link: None, bold: false, code: false, br: true, color: None, size, el: NO_EL, button: false, underline: false, input: NO_EL });
        }
    }
    fn open(&mut self, name: &str, tag: &str) {
        let id = attr(tag, "id").unwrap_or_default();
        let classes: Vec<String> = attr(tag, "class").map(|c| c.split_whitespace().map(String::from).collect()).unwrap_or_default();
        let parent = self.top();
        let mut st = {
            let anc: Vec<ElRef> = self.stack.iter().map(|o| ElRef { tag: &o.tag, id: &o.id, classes: &o.classes }).collect();
            let me = ElRef { tag: name, id: &id, classes: &classes };
            let style = attr(tag, "style");
            css::compute(&parent, &self.rules, &me, &anc, style.as_deref())
        };
        // Eski HTML atributlari
        if let Some(c) = attr(tag, "bgcolor").and_then(|c| css::parse_color(&c)) {
            st.bg = Some(c);
        }
        if name == "font" {
            if let Some(c) = attr(tag, "color").and_then(|c| css::parse_color(&c)) {
                st.color = Some(c);
            }
        }
        if let Some(a) = attr(tag, "align") {
            st.align = match a.to_ascii_lowercase().as_str() {
                "center" => 1,
                "right" => 2,
                _ => st.align,
            };
        }
        if attr(tag, "hidden").is_some() || tag.split_whitespace().any(|w| w.eq_ignore_ascii_case("hidden")) {
            st.hidden = true;
        }
        let onclick = attr(tag, "onclick");
        // Har bir element ro'yxatga olinadi (JS: querySelectorAll, classList, hodisalar qalqişi)
        let el = if self.doc.elems.len() < 60_000 {
            self.doc.elems.push(ElInfo { id: id.clone(), tag: String::from(name), onclick, classes: classes.clone(), parent: self.el() });
            (self.doc.elems.len() - 1) as u16
        } else {
            self.el()
        };
        if name == "body" || name == "html" {
            if st.bg.is_some() {
                self.doc.bg = st.bg;
            }
            if st.color.is_some() {
                self.doc.color = st.color;
            }
        }
        let link = if name == "a" { attr(tag, "href") } else { None };
        self.serial += 1;
        let serial = self.serial;
        self.stack.push(Open { tag: String::from(name), id, classes, sty: st, el, link, serial, input: NO_EL });
    }
    fn close(&mut self, name: &str) {
        if let Some(k) = self.stack.iter().rposition(|o| o.tag == name) {
            self.stack.truncate(k);
        }
    }
}

/// <link rel=stylesheet href=...> manzillari.
pub fn stylesheet_links(src: &str) -> Vec<String> {
    let low = src.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = low[from..].find("<link") {
        let st = from + i;
        let Some(e) = low[st..].find('>') else { break };
        let tag = &src[st + 1..st + e];
        if attr(tag, "rel").is_some_and(|r| r.to_ascii_lowercase().contains("stylesheet")) {
            if let Some(h) = attr(tag, "href") {
                out.push(h);
            }
        }
        from = st + e;
    }
    out
}

/// Hujjatdagi <style> mazmunlari.
fn inline_css(src: &str) -> String {
    let low = src.to_ascii_lowercase();
    let mut out = String::new();
    let mut from = 0;
    while let Some(i) = low[from..].find("<style") {
        let st = from + i;
        let Some(gt) = low[st..].find('>') else { break };
        let body = st + gt + 1;
        let end = low[body..].find("</style").map(|j| body + j).unwrap_or(low.len());
        out.push_str(&src[body..end]);
        out.push('\n');
        from = end;
    }
    out
}

pub fn parse(src: &str) -> Doc {
    parse_with(src, "")
}

/// HTML + tashqi CSS (<link> orqali yuklangan) → hujjat.
pub fn parse_with(src: &str, extra_css: &str) -> Doc {
    let mut sheet = String::from(UA_CSS);
    sheet.push_str(extra_css);
    sheet.push_str(&inline_css(src));
    let rules = css::parse_sheet(&sheet);
    let mut d = parse_inner(src, &rules, true);
    d.rules = rules;
    d
}

fn parse_inner(src: &str, rules: &[Rule], implicit_body: bool) -> Doc {
    let rules: Vec<Rule> = rules.to_vec();
    let mut p = P { doc: Doc::empty(), rules, stack: Vec::new(), cur: None, pre: 0, lists: Vec::new(), in_title: false, serial: 0, form: NO_EL };
    // Yaşirin <html> va <body> (teglar bölmasa ham uslublari qöllanadi)
    if implicit_body {
        p.open("html", "html");
        p.open("body", "body");
    }
    let mut i = 0;
    let b = src.as_bytes();
    while i < b.len() {
        if b[i] == b'<' {
            if src[i..].starts_with("<!--") {
                i = src[i..].find("-->").map(|j| i + j + 3).unwrap_or(b.len());
                continue;
            }
            let Some(end) = src[i..].find('>').map(|j| i + j) else { break };
            let tag = &src[i + 1..end];
            i = end + 1;
            let closing = tag.starts_with('/');
            let t = tag.trim_start_matches('/');
            let name: String = t.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            if !closing && name == "select" {
                // <option value>yozuv</option> lar; tanlangani (selected) yoki birinçisi
                let low = src[i..].to_ascii_lowercase();
                let e = low.find("</select").map(|j| i + j).unwrap_or(b.len());
                let inner = &src[i..e];
                let mut opts: Vec<(String, String)> = Vec::new();
                let mut sel = 0;
                let mut from = 0;
                let il = inner.to_ascii_lowercase();
                while let Some(k) = il[from..].find("<option") {
                    let st = from + k;
                    let Some(gt) = inner[st..].find('>').map(|g| st + g) else { break };
                    let otag = &inner[st + 1..gt];
                    let end = il[gt..].find("</option").map(|x| gt + x).or_else(|| il[gt + 1..].find("<option").map(|x| gt + 1 + x)).unwrap_or(inner.len());
                    let label = decode_entities(strip_tags(&inner[gt + 1..end]).trim());
                    let value = attr(otag, "value").unwrap_or_else(|| label.clone());
                    if otag.to_ascii_lowercase().contains("selected") {
                        sel = opts.len();
                    }
                    opts.push((value, label));
                    from = end;
                }
                let v = opts.get(sel).map(|o| o.0.clone()).unwrap_or_default();
                let k = p.add_input(6, t, v, None);
                let w = opts.iter().map(|o| o.1.chars().count()).max().unwrap_or(4) as u16 + 6;
                p.doc.inputs[k as usize].size = w.clamp(6, 40);
                p.doc.inputs[k as usize].options = opts;
                p.input_span(k);
                i = e;
                continue;
            }
            if !closing && name == "textarea" {
                let low = src[i..].to_ascii_lowercase();
                let e = low.find("</textarea").map(|j| i + j).unwrap_or(b.len());
                let v = decode_entities(&src[i..e]);
                let k = p.add_input(0, t, v, None);
                p.doc.inputs[k as usize].size = attr(t, "cols").and_then(|v| v.parse().ok()).unwrap_or(40);
                p.input_span(k);
                i = e;
                continue;
            }
            if !closing && (name == "script" || name == "style" || name == "noscript" || name == "template") {
                let close = alloc::format!("</{}", name);
                let low = src[i..].to_ascii_lowercase();
                let e = low.find(&close).map(|j| i + j).unwrap_or(b.len());
                if name == "script" {
                    match attr(t, "src") {
                        Some(u) => p.doc.scripts.push(Script::Src(u)),
                        None => p.doc.scripts.push(Script::Inline(String::from(&src[i..e]))),
                    }
                }
                i = e;
                continue;
            }
            if closing && (name == "body" || name == "html") {
                continue; // yaşirin elementlar oxirigaça qoladi
            }
            if !closing && (name == "body" || name == "html") {
                // Haqiqiy teg: atributlari bilan uslubni qayta hisoblaymiz (joyida)
                if let Some(k) = p.stack.iter().position(|o| o.tag == name) {
                    let rest = p.stack.split_off(k);
                    p.open(&name, t);
                    p.stack.extend(rest.into_iter().skip(1));
                }
                p.flush();
                continue;
            }
            if closing {
                match name.as_str() {
                    "form" => {
                        p.flush();
                        p.form = NO_EL;
                    }
                    "title" => p.in_title = false,
                    "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main" | "tr" | "td" | "th" | "table" | "center" | "aside" | "figure" | "dd" | "dt" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote" => p.flush(),
                    "ul" | "ol" => {
                        p.flush();
                        p.lists.pop();
                    }
                    "pre" => {
                        p.pre = p.pre.saturating_sub(1);
                        p.flush();
                    }
                    _ => {}
                }
                p.close(&name);
                continue;
            }
            // Yopilmagan <p>/<li> — yangisi boşlanganda yopamiz
            if (name == "p" || name == "li") && p.stack.last().is_some_and(|o| o.tag == name) {
                p.stack.pop();
            }
            let void = VOID.contains(&name.as_str()) || t.ends_with('/');
            if !void {
                p.open(&name, t);
            }
            let blockish = p.top().block == Some(true);
            match name.as_str() {
                "title" => p.in_title = true,
                "h1" => p.start(Kind::H(1)),
                "h2" => p.start(Kind::H(2)),
                "h3" | "h4" | "h5" | "h6" => p.start(Kind::H(3)),
                "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main" | "tr" | "dt" | "dd" | "figcaption" | "table" | "center" | "aside" | "figure" | "body" => p.flush(),
                "br" => p.br(),
                "hr" => {
                    p.start(Kind::Hr);
                    p.flush();
                }
                "ul" => p.lists.push((false, 0)),
                "ol" => p.lists.push((true, 0)),
                "li" => {
                    let depth = p.lists.len().max(1) as u8;
                    let n = match p.lists.last_mut() {
                        Some((true, c)) => {
                            *c += 1;
                            *c
                        }
                        _ => 0,
                    };
                    p.start(Kind::Li(depth, n));
                }
                "pre" => {
                    p.start(Kind::Pre);
                    p.pre += 1;
                }
                "blockquote" => p.start(Kind::Quote),
                "img" => {
                    if !p.hidden() {
                        let src_url = attr(t, "src").unwrap_or_default();
                        let alt = attr(t, "alt").unwrap_or_default();
                        p.flush();
                        let mut blk = p.new_block(Kind::Img);
                        blk.src = src_url;
                        blk.spans.push(Span { text: alt, link: p.stack.iter().rev().find_map(|o| o.link.clone()), bold: false, code: false, br: false, color: None, size: 14.0, el: p.el(), button: false, underline: false, input: NO_EL });
                        p.doc.blocks.push(blk);
                    }
                }
                "td" | "th" => p.flush(),
                "input" => {
                    let ty = attr(t, "type").unwrap_or_default().to_ascii_lowercase();
                    let kind = match ty.as_str() {
                        "submit" | "button" | "reset" | "image" => 1,
                        "checkbox" | "radio" => 2,
                        "password" => 3,
                        "hidden" => 4,
                        _ => 0,
                    };
                    let mut v = attr(t, "value").unwrap_or_default();
                    if kind == 1 && v.is_empty() {
                        v = String::from(if ty == "reset" { "Tozalaş" } else { "Yuboriş" });
                    }
                    let k = p.add_input(kind, t, v, None);
                    if ty == "button" {
                        p.doc.inputs[k as usize].form = NO_EL; // oddiy tugma — formani yubormaydi
                    }
                    p.input_span(k);
                }
                "form" => {
                    p.flush();
                    let action = attr(t, "action").unwrap_or_default();
                    let method = attr(t, "method").unwrap_or_else(|| String::from("get")).to_ascii_lowercase();
                    let el = p.stack.last().filter(|o| o.tag == "form").map(|o| o.el).unwrap_or(NO_EL);
                    p.doc.forms.push(Form { action, method, el });
                    p.form = (p.doc.forms.len() - 1) as u16;
                }
                "button" => {
                    let ty = attr(t, "type").unwrap_or_default().to_ascii_lowercase();
                    if p.form != NO_EL && ty != "button" {
                        // id/onclick open() da ro'yxatga olingan — öşa elementni işlatamiz
                        let el = p.stack.last().map(|o| o.el).unwrap_or(NO_EL);
                        let k = p.add_input(5, t, attr(t, "value").unwrap_or_default(), Some(el));
                        if let Some(o) = p.stack.last_mut() {
                            o.input = k;
                        }
                    }
                }
                _ if blockish => p.flush(),
                _ => {}
            }
            continue;
        }
        let next = src[i..].find('<').map(|j| i + j).unwrap_or(b.len());
        let t = &src[i..next];
        p.text(t);
        i = next;
    }
    p.flush();
    p.doc.title = String::from(p.doc.title.trim());
    p.doc
}

/// Teglarsiz matn (innerHTML qator içida).
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
}

/// HTML bölagi (innerHTML / appendChild) — hujjatning uslub qoidalari bilan.
fn parse_frag(src: &str, rules: &[Rule]) -> Doc {
    let mut d = parse_inner(src, rules, false);
    d.title.clear();
    d
}

/// Oddiy matnni hujjatga (bitta <pre>) aylantiradi.
pub fn plain(text: &str) -> Doc {
    let mut d = Doc::empty();
    d.blocks.push(Block {
        kind: Kind::Pre,
        spans: alloc::vec![Span { text: String::from(text), link: None, bold: false, code: true, br: false, color: None, size: 14.0, el: NO_EL, button: false, underline: false, input: NO_EL }],
        bg: None,
        align: 0,
        size: 14.0,
        color: None,
        el: NO_EL,
        src: String::new(),
        hidden: false,
        bgk: 0,
        pad: 0.0, bw: 0.0, bc: None, radius: 0.0, maxw: None, center: false, mt: 0.0, mb: 0.0, bmt: 0.0, bmb: 0.0,
        flex: 0, fcol: 0, fmode: 0, fgap: 0.0, gcols: 0, fjust: 0, fw: None, fgrow: 0.0,
    });
    d
}

/// Namunaviy mahalliy sahifa (~/Hujjatlar/zamin.html).
pub const SAMPLE: &str = r#"<!doctype html>
<html><head><title>Zamin OS — qöllanma</title></head>
<body>
<h1>Zamin OS qöllanmasi</h1>
<p>Bu sahifa <b>Zamin Brauzer</b>da öz HTML tahlilçimiz bilan körsatilmoqda.
Fayl: <code>~/Hujjatlar/zamin.html</code>.</p>
<h2>Klaviatura yorliqlari</h2>
<ul>
<li><b>Alt+Space</b> — tezkor qidiruv</li>
<li><b>Alt+Tab</b> — oynalarni almaştiriş</li>
<li><b>PrtSc</b> — ekran rasmi</li>
<li><b>AltGr</b> + s / c / o / g — ş ç ö ğ</li>
</ul>
<h2>Ilovalar</h2>
<ol>
<li>Fayllar — sudrab tashlaş, savat, nusxa olish</li>
<li>Kalkulyator va Tizim monitori</li>
<li>Chizma — rasm chiziş va PNG saqlaş</li>
</ol>
<blockquote>Hammasi Rust tilida noldan yozilgan — tayyor kutubxonalarsiz.</blockquote>
<h3>Kod namunasi</h3>
<pre>fn main() {
    println!("Salom, Zamin!");
}</pre>
<hr>
<p>Havolalar: <a href="zamin://bosh">Bosh sahifa</a> · <a href="zamin://haqida">Brauzer haqida</a></p>
</body></html>
"#;
