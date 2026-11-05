//! Kiçik HTML tahlilçisi — noldan. Hujjatni bloklar (sarlavha, xatboşi, ro'yxat,
//! kod, iqtibos, chiziq) va ularning ichidagi matn bölaklariga (havola, qalin,
//! kod) aylantiradi. <script>/<style> taşlanadi, belgilar (&amp; &#NN; …) yeçiladi.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    H(u8),
    P,
    Li(u8, u16), // çuqurlik, tartib raqami (0 — nuqta)
    Pre,
    Quote,
    Hr,
}

#[derive(Clone)]
pub struct Span {
    pub text: String,
    pub link: Option<String>,
    pub bold: bool,
    pub code: bool,
    pub br: bool, // qator uziliş
}

#[derive(Clone)]
pub struct Block {
    pub kind: Kind,
    pub spans: Vec<Span>,
}

pub struct Doc {
    pub title: String,
    pub blocks: Vec<Block>,
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

struct P {
    doc: Doc,
    cur: Option<Block>,
    bold: u32,
    code: u32,
    pre: u32,
    links: Vec<Option<String>>,
    lists: Vec<(bool, u16)>, // (tartibli, hisob)
    in_title: bool,
}

impl P {
    fn flush(&mut self) {
        if let Some(b) = self.cur.take() {
            let has = b.spans.iter().any(|s| !s.text.trim().is_empty());
            if has || b.kind == Kind::Hr {
                self.doc.blocks.push(b);
            }
        }
    }
    fn start(&mut self, k: Kind) {
        self.flush();
        self.cur = Some(Block { kind: k, spans: Vec::new() });
    }
    fn text(&mut self, t: &str) {
        if self.in_title {
            self.doc.title.push_str(&decode_entities(t));
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
            self.cur = Some(Block { kind: if self.pre > 0 { Kind::Pre } else { Kind::P }, spans: Vec::new() });
        }
        let link = self.links.iter().rev().find_map(|l| l.clone());
        let (bold, code) = (self.bold > 0, self.code > 0 || self.pre > 0);
        let b = self.cur.as_mut().unwrap();
        if b.spans.is_empty() && self.pre == 0 {
            s = String::from(s.trim_start());
        }
        b.spans.push(Span { text: s, link, bold, code, br: false });
    }
    fn br(&mut self) {
        if let Some(b) = self.cur.as_mut() {
            b.spans.push(Span { text: String::new(), link: None, bold: false, code: false, br: true });
        }
    }
}

pub fn parse(src: &str) -> Doc {
    let mut p = P {
        doc: Doc { title: String::new(), blocks: Vec::new() },
        cur: None,
        bold: 0,
        code: 0,
        pre: 0,
        links: Vec::new(),
        lists: Vec::new(),
        in_title: false,
    };
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
            // script/style — tarkibini taşlaymiz
            if !closing && (name == "script" || name == "style") {
                let close = alloc::format!("</{}", name);
                let low = src[i..].to_ascii_lowercase();
                i = low.find(&close).map(|j| i + j).unwrap_or(b.len());
                continue;
            }
            match (name.as_str(), closing) {
                ("title", c) => p.in_title = !c,
                ("h1", false) => p.start(Kind::H(1)),
                ("h2", false) => p.start(Kind::H(2)),
                ("h3" | "h4" | "h5" | "h6", false) => p.start(Kind::H(3)),
                ("p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main" | "tr" | "dt" | "dd" | "figcaption" | "table" | "form", _) => p.flush(),
                ("h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote", true) => p.flush(),
                ("br", _) => p.br(),
                ("hr", _) => {
                    p.start(Kind::Hr);
                    p.flush();
                }
                ("ul", false) => p.lists.push((false, 0)),
                ("ol", false) => p.lists.push((true, 0)),
                ("ul" | "ol", true) => {
                    p.flush();
                    p.lists.pop();
                }
                ("li", false) => {
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
                ("pre", false) => {
                    p.start(Kind::Pre);
                    p.pre += 1;
                }
                ("pre", true) => {
                    p.pre = p.pre.saturating_sub(1);
                    p.flush();
                }
                ("blockquote", false) => p.start(Kind::Quote),
                ("b" | "strong" | "th", false) => p.bold += 1,
                ("b" | "strong" | "th", true) => p.bold = p.bold.saturating_sub(1),
                ("code" | "kbd" | "tt" | "samp", false) => p.code += 1,
                ("code" | "kbd" | "tt" | "samp", true) => p.code = p.code.saturating_sub(1),
                ("a", false) => p.links.push(attr(t, "href")),
                ("a", true) => {
                    p.links.pop();
                }
                ("img", _) => {
                    let alt = attr(t, "alt").unwrap_or_default();
                    if !alt.is_empty() {
                        p.text(&alloc::format!("[{}]", alt));
                    }
                }
                ("td", false) => p.text(" "),
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
    Doc {
        title: String::new(),
        blocks: alloc::vec![Block {
            kind: Kind::Pre,
            spans: alloc::vec![Span { text: String::from(text), link: None, bold: false, code: true, br: false }],
        }],
    }
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
