//! SVG dekoderi — noldan. XML elementlari (rect, circle, ellipse, line,
//! polyline, polygon, path) va path `d` tili (M/L/H/V/C/S/Q/T/A/Z).
//! Ranglar: #rgb, #rrggbb, nomli, rgb(), none. Gradient/filtr/transform —
//! soddalaştirilgan (hozirça eʼtiborsiz yoki asosiy).

use super::vector::{self, flatten_arc, flatten_cubic, flatten_quad, Pt, Scene, Shape};
use super::Image;
use alloc::string::String;
use alloc::vec::Vec;

pub fn decode(d: &[u8]) -> Option<Image> {
    let text = core::str::from_utf8(d).ok()?;
    let scene = parse(text)?;
    Some(vector::render(&scene, 1000))
}

fn parse(text: &str) -> Option<Scene> {
    // <svg ...> dagi viewBox / width / height
    let svg_attrs = find_tag(text, "svg")?;
    let (mut vx, mut vy, mut vw, mut vh) = (0f32, 0f32, 0f32, 0f32);
    if let Some(vb) = attr(&svg_attrs, "viewBox").or_else(|| attr(&svg_attrs, "viewbox")) {
        let n: Vec<f32> = vb.split(|c: char| c == ' ' || c == ',').filter_map(num).collect();
        if n.len() == 4 {
            vx = n[0];
            vy = n[1];
            vw = n[2];
            vh = n[3];
        }
    }
    if vw <= 0.0 {
        vw = attr(&svg_attrs, "width").and_then(|s| num(strip_unit(&s))).unwrap_or(300.0);
        vh = attr(&svg_attrs, "height").and_then(|s| num(strip_unit(&s))).unwrap_or(150.0);
    }
    let mut scene = Scene::new(vw, vh);
    scene.bg = 0xffffff;
    // Har bir shakl elementini qayta işlaymiz (daraxtsiz, oddiy)
    let mut pos = 0;
    while let Some((name, attrs, next)) = next_tag(text, pos) {
        pos = next;
        // <text> — mazmunni </text> gaça öqiymiz
        if name == "text" || name == "tspan" {
            let content_end = text[pos..].find("</").map(|i| pos + i).unwrap_or(pos);
            let content = decode_entities(text[pos..content_end].trim());
            if !content.is_empty() {
                let x = fnum(&attrs, "x") - vx;
                let y = fnum(&attrs, "y") - vy;
                let size = attr(&attrs, "font-size").and_then(|s| num(strip_unit(&s))).unwrap_or(16.0).max(1.0);
                let (fill, _, _) = style(&attrs);
                let col = fill.unwrap_or(0x000000);
                let anchor = attr(&attrs, "text-anchor").unwrap_or_default();
                let w = super::text::width(&content, size, false);
                let ax = match anchor.as_str() {
                    "middle" => x - w / 2.0,
                    "end" => x - w,
                    _ => x,
                };
                scene.texts.push(super::vector::Text { x: ax, baseline: y, size, color: col, s: content, mono: false });
            }
            continue;
        }
        let shape = match name {
            "rect" => rect(&attrs),
            "circle" => circle(&attrs),
            "ellipse" => ellipse(&attrs),
            "line" => line(&attrs),
            "polyline" => poly(&attrs, false),
            "polygon" => poly(&attrs, true),
            "path" => path(&attrs),
            _ => None,
        };
        if let Some(mut s) = shape {
            // viewBox ofsetini ayiramiz
            if vx != 0.0 || vy != 0.0 {
                for sp in &mut s.subpaths {
                    for p in sp {
                        p.0 -= vx;
                        p.1 -= vy;
                    }
                }
            }
            scene.shapes.push(s);
        }
    }
    Some(scene)
}

fn style(attrs: &str) -> (Option<u32>, Option<u32>, f32) {
    // fill/stroke/stroke-width: alohida atributlar yoki style="..."
    let get = |key: &str| -> Option<String> {
        if let Some(v) = attr(attrs, key) {
            return Some(v);
        }
        if let Some(st) = attr(attrs, "style") {
            for part in st.split(';') {
                if let Some((k, v)) = part.split_once(':') {
                    if k.trim() == key {
                        return Some(String::from(v.trim()));
                    }
                }
            }
        }
        None
    };
    let fill = match get("fill") {
        Some(s) if s == "none" => None,
        Some(s) => color(&s),
        None => Some(0x000000), // SVG standart toʻldiriş — qora
    };
    let stroke = get("stroke").filter(|s| s != "none").and_then(|s| color(&s));
    let sw = get("stroke-width").and_then(|s| num(strip_unit(&s))).unwrap_or(1.0);
    (fill, stroke, sw)
}

fn mk(subpaths: Vec<Vec<Pt>>, attrs: &str) -> Option<Shape> {
    let (fill, stroke, sw) = style(attrs);
    if fill.is_none() && stroke.is_none() {
        return None;
    }
    Some(Shape { subpaths, fill, stroke, stroke_w: sw, nonzero: true })
}

fn rect(a: &str) -> Option<Shape> {
    let x = fnum(a, "x");
    let y = fnum(a, "y");
    let w = fnum(a, "width");
    let h = fnum(a, "height");
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    mk(alloc::vec![alloc::vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]], a)
}

fn circle(a: &str) -> Option<Shape> {
    let cx = fnum(a, "cx");
    let cy = fnum(a, "cy");
    let r = fnum(a, "r");
    if r <= 0.0 {
        return None;
    }
    let mut pts = Vec::new();
    flatten_arc(&mut pts, cx, cy, r, r, 0.0, 6.2831853, 48);
    mk(alloc::vec![pts], a)
}

fn ellipse(a: &str) -> Option<Shape> {
    let cx = fnum(a, "cx");
    let cy = fnum(a, "cy");
    let rx = fnum(a, "rx");
    let ry = fnum(a, "ry");
    if rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    let mut pts = Vec::new();
    flatten_arc(&mut pts, cx, cy, rx, ry, 0.0, 6.2831853, 48);
    mk(alloc::vec![pts], a)
}

fn line(a: &str) -> Option<Shape> {
    let p0 = (fnum(a, "x1"), fnum(a, "y1"));
    let p1 = (fnum(a, "x2"), fnum(a, "y2"));
    let mut s = mk(alloc::vec![alloc::vec![p0, p1]], a)?;
    s.fill = None; // chiziqda toʻldiriş yöq
    if s.stroke.is_none() {
        s.stroke = Some(0x000000);
    }
    Some(s)
}

fn poly(a: &str, closed: bool) -> Option<Shape> {
    let pts = attr(a, "points")?;
    let nums: Vec<f32> = pts.split(|c: char| c == ' ' || c == ',' || c == '\n').filter_map(num).collect();
    let mut v: Vec<Pt> = nums.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect();
    if v.len() < 2 {
        return None;
    }
    if closed {
        v.push(v[0]);
    }
    let mut s = mk(alloc::vec![v], a)?;
    if !closed {
        s.fill = None;
        if s.stroke.is_none() {
            s.stroke = Some(0x000000);
        }
    }
    Some(s)
}

fn path(a: &str) -> Option<Shape> {
    let d = attr(a, "d")?;
    let subpaths = parse_path(&d);
    if subpaths.is_empty() {
        return None;
    }
    mk(subpaths, a)
}

/// path `d` ni subpath'larga (yassilangan polilinlar) aylantiradi.
fn parse_path(d: &str) -> Vec<Vec<Pt>> {
    let mut subpaths: Vec<Vec<Pt>> = Vec::new();
    let mut cur: Vec<Pt> = Vec::new();
    let (mut cx, mut cy) = (0f32, 0f32);
    let (mut sx, mut sy) = (0f32, 0f32); // subpath boşi
    let (mut pcx, mut pcy) = (0f32, 0f32); // oxirgi nazorat nuqtasi (S/T uçun)
    let mut toks = Tokenizer::new(d);
    let mut cmd = ' ';
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 1_000_000 {
            break;
        }
        let c = match toks.peek_cmd() {
            Some(c) => {
                cmd = c;
                toks.next_cmd();
                c
            }
            None => {
                if cmd == ' ' {
                    break;
                }
                cmd // argumentlar takrorlanişi
            }
        };
        let rel = c.is_ascii_lowercase();
        let base = |cx: f32, cy: f32, v: f32, i: usize| if rel { if i == 0 { cx + v } else { cy + v } } else { v };
        match c.to_ascii_uppercase() {
            'M' => {
                let Some(x) = toks.num() else { break };
                let Some(y) = toks.num() else { break };
                if !cur.is_empty() {
                    subpaths.push(core::mem::take(&mut cur));
                }
                cx = if rel { cx + x } else { x };
                cy = if rel { cy + y } else { y };
                sx = cx;
                sy = cy;
                cur.push((cx, cy));
                cmd = if rel { 'l' } else { 'L' }; // keyingi juftlar — lineto
            }
            'L' => {
                let Some(x) = toks.num() else { break };
                let Some(y) = toks.num() else { break };
                cx = base(cx, cy, x, 0);
                cy = base(cx, cy, y, 1);
                cur.push((cx, cy));
            }
            'H' => {
                let Some(x) = toks.num() else { break };
                cx = if rel { cx + x } else { x };
                cur.push((cx, cy));
            }
            'V' => {
                let Some(y) = toks.num() else { break };
                cy = if rel { cy + y } else { y };
                cur.push((cx, cy));
            }
            'C' => {
                let p = toks.nums(6);
                let Some(p) = p else { break };
                let p1 = (base(cx, cy, p[0], 0), base(cx, cy, p[1], 1));
                let p2 = (base(cx, cy, p[2], 0), base(cx, cy, p[3], 1));
                let p3 = (base(cx, cy, p[4], 0), base(cx, cy, p[5], 1));
                flatten_cubic(&mut cur, (cx, cy), p1, p2, p3, 20);
                pcx = p2.0;
                pcy = p2.1;
                cx = p3.0;
                cy = p3.1;
            }
            'S' => {
                let p = toks.nums(4);
                let Some(p) = p else { break };
                let p1 = (2.0 * cx - pcx, 2.0 * cy - pcy);
                let p2 = (base(cx, cy, p[0], 0), base(cx, cy, p[1], 1));
                let p3 = (base(cx, cy, p[2], 0), base(cx, cy, p[3], 1));
                flatten_cubic(&mut cur, (cx, cy), p1, p2, p3, 20);
                pcx = p2.0;
                pcy = p2.1;
                cx = p3.0;
                cy = p3.1;
            }
            'Q' => {
                let p = toks.nums(4);
                let Some(p) = p else { break };
                let p1 = (base(cx, cy, p[0], 0), base(cx, cy, p[1], 1));
                let p2 = (base(cx, cy, p[2], 0), base(cx, cy, p[3], 1));
                flatten_quad(&mut cur, (cx, cy), p1, p2, 16);
                pcx = p1.0;
                pcy = p1.1;
                cx = p2.0;
                cy = p2.1;
            }
            'T' => {
                let p = toks.nums(2);
                let Some(p) = p else { break };
                let p1 = (2.0 * cx - pcx, 2.0 * cy - pcy);
                let p2 = (base(cx, cy, p[0], 0), base(cx, cy, p[1], 1));
                flatten_quad(&mut cur, (cx, cy), p1, p2, 16);
                pcx = p1.0;
                pcy = p1.1;
                cx = p2.0;
                cy = p2.1;
            }
            'A' => {
                let p = toks.nums(7);
                let Some(p) = p else { break };
                let (rx, ry) = (p[0], p[1]);
                let large = p[3] != 0.0;
                let sweep = p[4] != 0.0;
                let ex = if rel { cx + p[5] } else { p[5] };
                let ey = if rel { cy + p[6] } else { p[6] };
                arc_to(&mut cur, cx, cy, rx, ry, p[2], large, sweep, ex, ey);
                cx = ex;
                cy = ey;
            }
            'Z' => {
                if !cur.is_empty() {
                    cur.push((sx, sy));
                    subpaths.push(core::mem::take(&mut cur));
                }
                cx = sx;
                cy = sy;
                cmd = ' '; // Z argument olmaydi — takrorlanmasin (aks holda cheksiz tsikl)
            }
            _ => break,
        }
    }
    if !cur.is_empty() {
        subpaths.push(cur);
    }
    subpaths
}

/// SVG yoyi (endpoint) → markaz parametrizatsiyasi, keyin yassilaş.
#[allow(clippy::too_many_arguments)]
fn arc_to(out: &mut Vec<Pt>, x0: f32, y0: f32, mut rx: f32, mut ry: f32, _rot: f32, large: bool, sweep: bool, x1: f32, y1: f32) {
    if rx == 0.0 || ry == 0.0 {
        out.push((x1, y1));
        return;
    }
    rx = vector::fabs(rx);
    ry = vector::fabs(ry);
    // Rotatsiyani soddalaştirib eʼtiborsiz qoldiramiz (x-axis-rotation=0 keng tarqalgan)
    let dx = (x0 - x1) / 2.0;
    let dy = (y0 - y1) / 2.0;
    let mut p = dx * dx / (rx * rx) + dy * dy / (ry * ry);
    if p > 1.0 {
        let s = vector::fsqrt(p);
        rx *= s;
        ry *= s;
        p = 1.0;
    }
    let sign = if large != sweep { 1.0 } else { -1.0 };
    let coef = sign * vector::fsqrt(((1.0 - p) / p).max(0.0));
    let cxp = coef * rx * dy / ry;
    let cyp = -coef * ry * dx / rx;
    let cx = cxp + (x0 + x1) / 2.0;
    let cy = cyp + (y0 + y1) / 2.0;
    let a0 = vector::atan2((dy - cyp) / ry, (dx - cxp) / rx);
    let a1 = vector::atan2((-dy - cyp) / ry, (-dx - cxp) / rx);
    let mut da = a1 - a0;
    if sweep && da < 0.0 {
        da += 6.2831853;
    }
    if !sweep && da > 0.0 {
        da -= 6.2831853;
    }
    let steps = 32;
    for i in 1..=steps {
        let t = a0 + da * i as f32 / steps as f32;
        out.push((cx + rx * vector::cos(t), cy + ry * vector::sin(t)));
    }
}

// ---- Kiçik XML / son yordamçilari ----

fn fnum(a: &str, key: &str) -> f32 {
    attr(a, key).and_then(|s| num(strip_unit(&s))).unwrap_or(0.0)
}

/// XML belgili obyektlarini (&amp; &lt; …) matnga aylantiradi.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return String::from(s);
    }
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#39;", "'")
}

fn strip_unit(s: &str) -> &str {
    s.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%').trim()
}

fn num(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    parse_f32(s)
}

/// Berilgan nomli birinçi tagning atributlar qatorini qaytaradi.
fn find_tag(text: &str, name: &str) -> Option<String> {
    let mut pos = 0;
    while let Some((n, a, next)) = next_tag(text, pos) {
        if n == name {
            return Some(a);
        }
        pos = next;
    }
    None
}

/// Keyingi element tagi: (nom, atributlar, keyingi pozitsiya).
fn next_tag(text: &str, from: usize) -> Option<(&str, String, usize)> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // izoh/deklaratsiya/yopiş tagini ötkazamiz
            if bytes.get(i + 1) == Some(&b'!') || bytes.get(i + 1) == Some(&b'?') || bytes.get(i + 1) == Some(&b'/') {
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            if j >= bytes.len() {
                return None;
            }
            let inner = &text[start..j];
            // nom — birinçi soʻz
            let name_end = inner.find(|c: char| c.is_ascii_whitespace() || c == '/').unwrap_or(inner.len());
            let name = &inner[..name_end];
            let attrs = String::from(&inner[name_end..]);
            return Some((name, attrs, j + 1));
        }
        i += 1;
    }
    None
}

/// Atribut qiymatini oladi: `key="val"` yoki `key='val'`.
fn attr(attrs: &str, key: &str) -> Option<String> {
    let bytes = attrs.as_bytes();
    let mut i = 0;
    while i + key.len() < bytes.len() {
        // kalit chegarasini tekşiramiz (oldi — boşliq yoki boş)
        if attrs[i..].starts_with(key) {
            let before_ok = i == 0 || bytes[i - 1].is_ascii_whitespace();
            let after = i + key.len();
            let mut k = after;
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            if before_ok && k < bytes.len() && bytes[k] == b'=' {
                k += 1;
                while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < bytes.len() && (bytes[k] == b'"' || bytes[k] == b'\'') {
                    let q = bytes[k];
                    let vs = k + 1;
                    let mut ve = vs;
                    while ve < bytes.len() && bytes[ve] != q {
                        ve += 1;
                    }
                    return Some(String::from(&attrs[vs..ve]));
                }
            }
        }
        i += 1;
    }
    None
}

/// Rang: #rgb, #rrggbb, rgb(r,g,b), nomli, none.
fn color(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let r = hexd(hex.as_bytes()[0])?;
                let g = hexd(hex.as_bytes()[1])?;
                let b = hexd(hex.as_bytes()[2])?;
                Some((r * 17) << 16 | (g * 17) << 8 | (b * 17))
            }
            6 => u32::from_str_radix(hex, 16).ok(),
            _ => None,
        };
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|x| x.strip_suffix(')')) {
        let n: Vec<u32> = inner.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if n.len() == 3 {
            return Some(n[0] << 16 | n[1] << 8 | n[2]);
        }
    }
    named_color(s)
}

fn hexd(c: u8) -> Option<u32> {
    match c {
        b'0'..=b'9' => Some((c - b'0') as u32),
        b'a'..=b'f' => Some((c - b'a' + 10) as u32),
        b'A'..=b'F' => Some((c - b'A' + 10) as u32),
        _ => None,
    }
}

fn named_color(s: &str) -> Option<u32> {
    Some(match s.to_ascii_lowercase().as_str() {
        "black" => 0x000000,
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "lime" => 0x00ff00,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "cyan" | "aqua" => 0x00ffff,
        "magenta" | "fuchsia" => 0xff00ff,
        "gray" | "grey" => 0x808080,
        "silver" => 0xc0c0c0,
        "orange" => 0xffa500,
        "purple" => 0x800080,
        "brown" => 0xa52a2a,
        "pink" => 0xffc0cb,
        "navy" => 0x000080,
        "teal" => 0x008080,
        "olive" => 0x808000,
        "maroon" => 0x800000,
        _ => return None,
    })
}

/// Son tokenizatori (path `d` uçun).
struct Tokenizer<'a> {
    b: &'a [u8],
    i: usize,
}
impl<'a> Tokenizer<'a> {
    fn new(s: &'a str) -> Self {
        Tokenizer { b: s.as_bytes(), i: 0 }
    }
    fn skip_sep(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b',' | b'\n' | b'\r' | b'\t') {
            self.i += 1;
        }
    }
    fn peek_cmd(&mut self) -> Option<char> {
        self.skip_sep();
        let c = *self.b.get(self.i)? as char;
        if c.is_ascii_alphabetic() {
            Some(c)
        } else {
            None
        }
    }
    fn next_cmd(&mut self) {
        self.i += 1;
    }
    fn num(&mut self) -> Option<f32> {
        self.skip_sep();
        let start = self.i;
        if self.i < self.b.len() && (self.b[self.i] == b'+' || self.b[self.i] == b'-') {
            self.i += 1;
        }
        let mut seen_dot = false;
        let mut seen_e = false;
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if c.is_ascii_digit() {
                self.i += 1;
            } else if c == b'.' && !seen_dot {
                seen_dot = true;
                self.i += 1;
            } else if (c == b'e' || c == b'E') && !seen_e {
                seen_e = true;
                self.i += 1;
                if self.i < self.b.len() && (self.b[self.i] == b'+' || self.b[self.i] == b'-') {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
        if self.i == start {
            return None;
        }
        parse_f32(core::str::from_utf8(&self.b[start..self.i]).ok()?)
    }
    fn nums(&mut self, n: usize) -> Option<Vec<f32>> {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(self.num()?);
        }
        Some(v)
    }
}

/// Oddiy f32 tahlilçi (no_std).
pub fn parse_f32(s: &str) -> Option<f32> {
    let s = s.trim();
    let b = s.as_bytes();
    let mut i = 0;
    let neg = if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        let n = b[i] == b'-';
        i += 1;
        n
    } else {
        false
    };
    let mut int = 0f64;
    let mut any = false;
    while i < b.len() && b[i].is_ascii_digit() {
        int = int * 10.0 + (b[i] - b'0') as f64;
        i += 1;
        any = true;
    }
    let mut frac = 0f64;
    let mut scale = 1f64;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            frac = frac * 10.0 + (b[i] - b'0') as f64;
            scale *= 10.0;
            i += 1;
            any = true;
        }
    }
    if !any {
        return None;
    }
    let mut val = int + frac / scale;
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        let eneg = if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
            let n = b[i] == b'-';
            i += 1;
            n
        } else {
            false
        };
        let mut exp = 0i32;
        while i < b.len() && b[i].is_ascii_digit() {
            exp = exp * 10 + (b[i] - b'0') as i32;
            i += 1;
        }
        let mut m = 1f64;
        for _ in 0..exp {
            m *= 10.0;
        }
        if eneg {
            val /= m;
        } else {
            val *= m;
        }
    }
    Some(if neg { -val as f32 } else { val as f32 })
}
