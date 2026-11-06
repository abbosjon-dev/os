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
}

/// JS körinadigan element (id yoki onclick bölganlar).
#[derive(Clone)]
pub struct ElInfo {
    pub id: String,
    pub tag: String,
    pub onclick: Option<String>,
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
}

pub const NO_EL: u16 = u16::MAX;

impl Doc {
    pub fn empty() -> Doc {
        Doc { title: String::new(), blocks: Vec::new(), elems: Vec::new(), scripts: Vec::new(), bg: None, color: None }
    }
    pub fn find_id(&self, id: &str) -> Option<usize> {
        self.elems.iter().position(|e| e.id == id)
    }
    /// Elementning matni (bloklari yoki bölaklari).
    pub fn text_of(&self, el: usize) -> String {
        let e = el as u16;
        let mut out = String::new();
        for b in &self.blocks {
            if b.el == e {
                for s in &b.spans {
                    out.push_str(&s.text);
                }
            } else {
                for s in b.spans.iter().filter(|s| s.el == e) {
                    out.push_str(&s.text);
                }
            }
        }
        out
    }
    /// Element matnini almaştiradi (JS: textContent / innerHTML).
    pub fn set_text(&mut self, el: usize, text: &str) {
        let e = el as u16;
        for b in self.blocks.iter_mut() {
            if b.el == e {
                let tmpl = b.spans.first().cloned().unwrap_or(Span {
                    text: String::new(), link: None, bold: false, code: false, br: false, color: None, size: b.size, el: e, button: false, underline: false,
                });
                b.spans = alloc::vec![Span { text: String::from(text), br: false, ..tmpl }];
                return;
            }
        }
        for b in self.blocks.iter_mut() {
            if let Some(first) = b.spans.iter().position(|s| s.el == e) {
                b.spans[first].text = String::from(text);
                let mut k = 0;
                b.spans.retain(|s| {
                    let keep = s.el != e || k == first;
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
        for b in self.blocks.iter_mut() {
            let whole = b.el == e;
            if whole {
                match prop.as_str() {
                    "background" | "background-color" => b.bg = css::parse_color(val),
                    "text-align" => b.align = if low == "center" { 1 } else if low == "right" { 2 } else { 0 },
                    "display" => b.hidden = low == "none",
                    "color" => b.color = css::parse_color(val),
                    "font-size" => {
                        let mut st = Sty::root();
                        css::apply(&mut st, "font-size", val, b.size);
                        b.size = st.size;
                    }
                    _ => {}
                }
            }
            for sp in b.spans.iter_mut().filter(|s| whole || s.el == e) {
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
a{text-decoration:underline}pre,code,kbd,samp,tt{font-size:.9em}button{font-weight:bold}";

const VOID: [&str; 12] = ["br", "hr", "img", "meta", "link", "input", "area", "base", "col", "embed", "source", "wbr"];

struct Open {
    tag: String,
    id: String,
    classes: Vec<String>,
    sty: Sty,
    el: u16,
    link: Option<String>,
    serial: u32,
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
            let has = b.spans.iter().any(|s| !s.text.trim().is_empty());
            if has || b.kind == Kind::Hr || b.kind == Kind::Img {
                self.doc.blocks.push(b);
            }
        }
    }
    fn new_block(&self, k: Kind) -> Block {
        let st = self.top();
        // Fon: eng yaqin fonli ajdod (body/html dan taşqari) — butun konteyner bölib bo'yaladi
        let (bg, bgk) = self
            .stack
            .iter()
            .rev()
            .take_while(|o| o.tag != "body" && o.tag != "html")
            .find(|o| o.sty.bg.is_some())
            .map(|o| (o.sty.bg, o.serial))
            .unwrap_or((None, 0));
        Block { kind: k, spans: Vec::new(), bg, align: st.align, size: st.size, color: st.color, el: self.el(), src: String::new(), hidden: false, bgk }
    }
    fn start(&mut self, k: Kind) {
        self.flush();
        if self.hidden() {
            return;
        }
        self.cur = Some(self.new_block(k));
    }
    fn text(&mut self, t: &str) {
        if self.in_title {
            self.doc.title.push_str(&decode_entities(t));
            return;
        }
        if self.hidden() {
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
        let el = self.el();
        let b = self.cur.as_mut().unwrap();
        if b.spans.is_empty() && self.pre == 0 {
            s = String::from(s.trim_start());
        }
        let underline = st.underline.unwrap_or(false) && link.is_some();
        b.spans.push(Span { text: s, link, bold: st.bold, code, br: false, color: st.color, size: st.size, el, button, underline });
    }
    fn br(&mut self) {
        if let Some(b) = self.cur.as_mut() {
            let size = b.size;
            b.spans.push(Span { text: String::new(), link: None, bold: false, code: false, br: true, color: None, size, el: NO_EL, button: false, underline: false });
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
        let el = if !id.is_empty() || onclick.is_some() {
            self.doc.elems.push(ElInfo { id: id.clone(), tag: String::from(name), onclick });
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
        self.stack.push(Open { tag: String::from(name), id, classes, sty: st, el, link, serial });
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
    let mut p = P { doc: Doc::empty(), rules: css::parse_sheet(&sheet), stack: Vec::new(), cur: None, pre: 0, lists: Vec::new(), in_title: false, serial: 0 };
    // Yaşirin <html> va <body> (teglar bölmasa ham uslublari qöllanadi)
    p.open("html", "html");
    p.open("body", "body");
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
                    "title" => p.in_title = false,
                    "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main" | "tr" | "table" | "form" | "center" | "aside" | "figure" | "dd" | "dt" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote" => p.flush(),
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
                "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main" | "tr" | "dt" | "dd" | "figcaption" | "table" | "form" | "center" | "aside" | "figure" | "body" => p.flush(),
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
                        blk.spans.push(Span { text: alt, link: p.stack.iter().rev().find_map(|o| o.link.clone()), bold: false, code: false, br: false, color: None, size: 14.0, el: p.el(), button: false, underline: false });
                        p.doc.blocks.push(blk);
                    }
                }
                "td" | "th" => p.text(" "),
                "input" => {
                    let v = attr(t, "value").or_else(|| attr(t, "placeholder")).unwrap_or_default();
                    if !v.is_empty() {
                        p.text(&alloc::format!("[{}]", v));
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

/// Oddiy matnni hujjatga (bitta <pre>) aylantiradi.
pub fn plain(text: &str) -> Doc {
    let mut d = Doc::empty();
    d.blocks.push(Block {
        kind: Kind::Pre,
        spans: alloc::vec![Span { text: String::from(text), link: None, bold: false, code: true, br: false, color: None, size: 14.0, el: NO_EL, button: false, underline: false }],
        bg: None,
        align: 0,
        size: 14.0,
        color: None,
        el: NO_EL,
        src: String::new(),
        hidden: false,
        bgk: 0,
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
