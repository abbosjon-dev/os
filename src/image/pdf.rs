//! PDF dekoderi — tuzilma tahlili + mazmun oqimi interpretatori (noldan).
//! Birinçi sahifaning vektor grafikasini çizadi: yöllar (m/l/c/v/y/re/h),
//! toʻldiriş/chiziq (f/F/f*/S/s/B/b), ranglar (rg/RG/g/G/k/K), CTM (cm/q/Q),
//! FlateDecode (inflate). Matn (BT/Tj) va rasmlar — hozirça çizilmaydi.

use super::inflate;
use super::svg::parse_f32;
use super::vector::{self, flatten_cubic, Pt, Scene, Shape};
use super::Image;
use alloc::string::String;
use alloc::vec::Vec;

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 5 || &d[..5] != b"%PDF-" {
        return None;
    }
    // Barcha "N G obj ... endobj" obyektlarini yigʻamiz
    let objs = scan_objects(d);
    // Sahifa topamiz (/Type /Page, /MediaBox, /Contents)
    let (mbox, contents) = find_page(&objs)?;
    let (w, h) = (mbox.2 - mbox.0, mbox.3 - mbox.1);
    if w <= 0.0 || h <= 0.0 || w > 20000.0 || h > 20000.0 {
        return None;
    }
    // Mazmun oqim(lar)ini yigʻib ochamiz
    let mut stream = Vec::new();
    for &obj_id in &contents {
        if let Some(o) = objs.iter().find(|o| o.id == obj_id) {
            if let Some(s) = decode_stream(o) {
                stream.extend_from_slice(&s);
                stream.push(b'\n');
            }
        }
    }
    if stream.is_empty() {
        return None;
    }
    let mut scene = Scene::new(w, h);
    scene.bg = 0xffffff;
    interpret(&stream, &mut scene, mbox, h);
    if scene.shapes.is_empty() && scene.texts.is_empty() {
        return None;
    }
    Some(vector::render(&scene, 1200))
}

struct Obj {
    id: u32,
    body: Vec<u8>,          // obj va endobj orasidagi xom bayt
    stream: Option<(usize, usize)>, // (boşlanişi, oxiri) body içida
}

fn scan_objects(d: &[u8]) -> Vec<Obj> {
    let mut objs = Vec::new();
    let mut i = 0;
    while i < d.len() {
        if let Some(p) = find(d, b" obj", i) {
            // raqam G obj — orqaga raqamni öqiymiz
            let mut j = p;
            // " obj" dan oldin: generation va obj number
            // Oldingi ikki sonni topamiz
            let head_start = line_start_scan(d, p);
            let head = &d[head_start..p];
            let nums: Vec<u32> = core::str::from_utf8(head).unwrap_or("").split_whitespace().filter_map(|t| t.parse().ok()).collect();
            let id = nums.first().copied().unwrap_or(0);
            j = p + 4;
            // endobj ni topamiz
            let end = find(d, b"endobj", j).unwrap_or(d.len());
            let body = d[j..end].to_vec();
            // stream?
            let stream = find(&body, b"stream", 0).map(|s| {
                let mut ds = s + 6;
                // CRLF yoki LF
                if body.get(ds) == Some(&b'\r') {
                    ds += 1;
                }
                if body.get(ds) == Some(&b'\n') {
                    ds += 1;
                }
                let de = find(&body, b"endstream", ds).unwrap_or(body.len());
                (ds, de)
            });
            objs.push(Obj { id, body, stream });
            i = end + 6;
        } else {
            break;
        }
    }
    objs
}

fn line_start_scan(d: &[u8], mut p: usize) -> usize {
    // " obj" oldidagi ikkita son bölgan joygaça orqaga
    let mut count_sp = 0;
    while p > 0 {
        p -= 1;
        if d[p] == b'\n' || d[p] == b'\r' {
            return p + 1;
        }
        if d[p] == b' ' {
            count_sp += 1;
            if count_sp > 2 {
                return p + 1;
            }
        }
    }
    0
}

fn decode_stream(o: &Obj) -> Option<Vec<u8>> {
    let (s, e) = o.stream?;
    let raw = &o.body[s..e.min(o.body.len())];
    // Filter?
    let dict = &o.body[..o.stream.map(|(s, _)| s).unwrap_or(o.body.len())];
    let dict_s = core::str::from_utf8(dict).unwrap_or("");
    if dict_s.contains("FlateDecode") {
        inflate::zlib(raw, 64 * 1024 * 1024).or_else(|| inflate::inflate(raw, 64 * 1024 * 1024))
    } else {
        Some(raw.to_vec())
    }
}

type Box4 = (f32, f32, f32, f32);

fn find_page(objs: &[Obj]) -> Option<(Box4, Vec<u32>)> {
    // /Type /Page bölgan obyekt
    for o in objs {
        let s = core::str::from_utf8(&o.body).unwrap_or("");
        if s.contains("/Page") && !s.contains("/Pages") {
            let mbox = parse_mediabox(s).or_else(|| inherited_mediabox(objs)).unwrap_or((0.0, 0.0, 612.0, 792.0));
            let contents = parse_contents(s);
            if !contents.is_empty() {
                return Some((mbox, contents));
            }
        }
    }
    None
}

fn inherited_mediabox(objs: &[Obj]) -> Option<Box4> {
    for o in objs {
        let s = core::str::from_utf8(&o.body).unwrap_or("");
        if s.contains("/Pages") {
            if let Some(m) = parse_mediabox(s) {
                return Some(m);
            }
        }
    }
    None
}

fn parse_mediabox(s: &str) -> Option<Box4> {
    let i = s.find("/MediaBox")?;
    let after = &s[i + 9..];
    let lb = after.find('[')?;
    let rb = after.find(']')?;
    let nums: Vec<f32> = after[lb + 1..rb].split_whitespace().filter_map(parse_f32).collect();
    if nums.len() == 4 {
        Some((nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

fn parse_contents(s: &str) -> Vec<u32> {
    let Some(i) = s.find("/Contents") else { return Vec::new() };
    let after = &s[i + 9..];
    // Massiv [a 0 R b 0 R] yoki bitta "a 0 R"
    if let Some(lb) = after.find('[') {
        if lb < 20 {
            let rb = after.find(']').unwrap_or(after.len());
            return extract_refs(&after[lb + 1..rb]);
        }
    }
    // Bitta havola: /Contents dan keyingi birinçi "N G R"
    let mut v = extract_refs(&after[..after.len().min(60)]);
    v.truncate(1);
    v
}

/// Tokenlarni ajratishda '/' va '>' belgilarini ham chegara deb olamiz.
fn extract_refs(s: &str) -> Vec<u32> {
    let toks: Vec<&str> = s.split(|c: char| c.is_whitespace() || c == '/' || c == '>' || c == '<').filter(|t| !t.is_empty()).collect();
    let mut refs = Vec::new();
    let mut i = 0;
    while i + 2 < toks.len() {
        if toks[i + 2] == "R" {
            if let Ok(n) = toks[i].parse::<u32>() {
                refs.push(n);
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    refs
}

fn find(h: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= h.len() {
        return None;
    }
    (from..=h.len().saturating_sub(needle.len())).find(|&i| &h[i..i + needle.len()] == needle)
}

// ---- Mazmun oqimi interpretatori ----

#[derive(Clone, Copy)]
struct Mat(f32, f32, f32, f32, f32, f32);
impl Mat {
    fn apply(&self, x: f32, y: f32) -> Pt {
        (self.0 * x + self.2 * y + self.4, self.1 * x + self.3 * y + self.5)
    }
    fn mul(&self, o: &Mat) -> Mat {
        Mat(
            o.0 * self.0 + o.1 * self.2,
            o.0 * self.1 + o.1 * self.3,
            o.2 * self.0 + o.3 * self.2,
            o.2 * self.1 + o.3 * self.3,
            o.4 * self.0 + o.5 * self.2 + self.4,
            o.4 * self.1 + o.5 * self.3 + self.5,
        )
    }
}

fn interpret(data: &[u8], scene: &mut Scene, mbox: Box4, h: f32) {
    // Boshlangʼiç CTM: y-up → y-down, MediaBox ofseti
    let base = Mat(1.0, 0.0, 0.0, -1.0, -mbox.0, h + mbox.1);
    let mut ctm = base;
    let mut stack: Vec<Mat> = Vec::new();
    let mut fill = 0x000000u32;
    let mut stroke = 0x000000u32;
    let mut lw = 1.0f32;
    let mut lwstack: Vec<(u32, u32, f32)> = Vec::new();
    let mut ops: Vec<f32> = Vec::new();

    let mut cur: Vec<Pt> = Vec::new();
    let mut subs: Vec<Vec<Pt>> = Vec::new();
    let mut pt = (0f32, 0f32);
    let mut start = (0f32, 0f32);
    // Matn holati
    let mut tmat = Mat(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let mut tlm = tmat;
    let mut fsize = 12.0f32;
    let mut tj_buf = String::new();

    let toks = tokenize(data);
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            CTok::Num(n) => ops.push(*n),
            CTok::Str(s) => tj_buf.push_str(s),
            CTok::Op(op) => {
                let dev = |ctm: &Mat, x: f32, y: f32| ctm.apply(x, y);
                match op.as_str() {
                    "m" => {
                        if cur.len() >= 2 {
                            subs.push(core::mem::take(&mut cur));
                        } else {
                            cur.clear();
                        }
                        let (x, y) = (nth(&ops, 0), nth(&ops, 1));
                        pt = (x, y);
                        start = (x, y);
                        cur.push(dev(&ctm, x, y));
                    }
                    "l" => {
                        let (x, y) = (nth(&ops, 0), nth(&ops, 1));
                        pt = (x, y);
                        cur.push(dev(&ctm, x, y));
                    }
                    "c" => {
                        let p1 = dev(&ctm, nth(&ops, 0), nth(&ops, 1));
                        let p2 = dev(&ctm, nth(&ops, 2), nth(&ops, 3));
                        let p3 = dev(&ctm, nth(&ops, 4), nth(&ops, 5));
                        flatten_cubic(&mut cur, dev(&ctm, pt.0, pt.1), p1, p2, p3, 18);
                        pt = (nth(&ops, 4), nth(&ops, 5));
                    }
                    "v" => {
                        let p2 = dev(&ctm, nth(&ops, 0), nth(&ops, 1));
                        let p3 = dev(&ctm, nth(&ops, 2), nth(&ops, 3));
                        let p0 = dev(&ctm, pt.0, pt.1);
                        flatten_cubic(&mut cur, p0, p0, p2, p3, 18);
                        pt = (nth(&ops, 2), nth(&ops, 3));
                    }
                    "y" => {
                        let p1 = dev(&ctm, nth(&ops, 0), nth(&ops, 1));
                        let p3 = dev(&ctm, nth(&ops, 2), nth(&ops, 3));
                        flatten_cubic(&mut cur, dev(&ctm, pt.0, pt.1), p1, p3, p3, 18);
                        pt = (nth(&ops, 2), nth(&ops, 3));
                    }
                    "re" => {
                        let (x, y, w, hh) = (nth(&ops, 0), nth(&ops, 1), nth(&ops, 2), nth(&ops, 3));
                        if cur.len() >= 2 {
                            subs.push(core::mem::take(&mut cur));
                        } else {
                            cur.clear();
                        }
                        cur.push(dev(&ctm, x, y));
                        cur.push(dev(&ctm, x + w, y));
                        cur.push(dev(&ctm, x + w, y + hh));
                        cur.push(dev(&ctm, x, y + hh));
                        cur.push(dev(&ctm, x, y));
                        subs.push(core::mem::take(&mut cur));
                    }
                    "h" => {
                        if !cur.is_empty() {
                            let f = cur[0];
                            cur.push(f);
                        }
                        pt = start;
                    }
                    "f" | "F" | "f*" | "b" | "b*" | "B" | "B*" => {
                        if cur.len() >= 2 {
                            subs.push(core::mem::take(&mut cur));
                        }
                        let nz = op == "f" || op == "F" || op == "b" || op == "B";
                        if !subs.is_empty() {
                            scene.shapes.push(Shape {
                                subpaths: subs.clone(),
                                fill: Some(fill),
                                stroke: if op.starts_with('B') || op.starts_with('b') { Some(stroke) } else { None },
                                stroke_w: scale_of(&ctm) * lw,
                                nonzero: nz,
                            });
                        }
                        subs.clear();
                        cur.clear();
                    }
                    "S" | "s" => {
                        if op == "s" && !cur.is_empty() {
                            let f = cur[0];
                            cur.push(f);
                        }
                        if cur.len() >= 2 {
                            subs.push(core::mem::take(&mut cur));
                        }
                        if !subs.is_empty() {
                            scene.shapes.push(Shape {
                                subpaths: subs.clone(),
                                fill: None,
                                stroke: Some(stroke),
                                stroke_w: (scale_of(&ctm) * lw).max(0.4),
                                nonzero: false,
                            });
                        }
                        subs.clear();
                        cur.clear();
                    }
                    "n" => {
                        subs.clear();
                        cur.clear();
                    }
                    "rg" => fill = rgbf(nth(&ops, 0), nth(&ops, 1), nth(&ops, 2)),
                    "RG" => stroke = rgbf(nth(&ops, 0), nth(&ops, 1), nth(&ops, 2)),
                    "g" => fill = rgbf(nth(&ops, 0), nth(&ops, 0), nth(&ops, 0)),
                    "G" => stroke = rgbf(nth(&ops, 0), nth(&ops, 0), nth(&ops, 0)),
                    "k" => fill = cmyk(&ops),
                    "K" => stroke = cmyk(&ops),
                    "sc" | "scn" => {
                        if ops.len() >= 3 {
                            fill = rgbf(nth(&ops, 0), nth(&ops, 1), nth(&ops, 2));
                        } else if ops.len() == 1 {
                            fill = rgbf(nth(&ops, 0), nth(&ops, 0), nth(&ops, 0));
                        }
                    }
                    "SC" | "SCN" => {
                        if ops.len() >= 3 {
                            stroke = rgbf(nth(&ops, 0), nth(&ops, 1), nth(&ops, 2));
                        } else if ops.len() == 1 {
                            stroke = rgbf(nth(&ops, 0), nth(&ops, 0), nth(&ops, 0));
                        }
                    }
                    "w" => lw = nth(&ops, 0),
                    "q" => {
                        stack.push(ctm);
                        lwstack.push((fill, stroke, lw));
                    }
                    "Q" => {
                        if let Some(m) = stack.pop() {
                            ctm = m;
                        }
                        if let Some((f, s, l)) = lwstack.pop() {
                            fill = f;
                            stroke = s;
                            lw = l;
                        }
                    }
                    "cm" => {
                        let m = Mat(nth(&ops, 0), nth(&ops, 1), nth(&ops, 2), nth(&ops, 3), nth(&ops, 4), nth(&ops, 5));
                        ctm = ctm.mul(&m);
                    }
                    "BT" => {
                        tmat = Mat(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
                        tlm = tmat;
                        tj_buf.clear();
                    }
                    "Tf" => fsize = nth(&ops, 0).max(1.0), // /F size Tf (nom skip qilingan)
                    "Td" | "TD" => {
                        let t = Mat(1.0, 0.0, 0.0, 1.0, nth(&ops, 0), nth(&ops, 1));
                        tlm = tlm.mul(&t);
                        tmat = tlm;
                    }
                    "Tm" => {
                        tmat = Mat(nth(&ops, 0), nth(&ops, 1), nth(&ops, 2), nth(&ops, 3), nth(&ops, 4), nth(&ops, 5));
                        tlm = tmat;
                    }
                    "Tj" | "TJ" | "'" | "\"" => {
                        if !tj_buf.is_empty() {
                            let m = ctm.mul(&tmat); // avval tmat, keyin ctm
                            let (bx, by) = m.apply(0.0, 0.0);
                            let sz = fsize * scale_of(&m);
                            scene.texts.push(super::vector::Text {
                                x: bx,
                                baseline: by,
                                size: sz.max(1.0),
                                color: fill,
                                s: core::mem::take(&mut tj_buf),
                                mono: false,
                            });
                        }
                    }
                    "ET" => tj_buf.clear(),
                    _ => {} // boşqa matn/rasm operatorlari — eʼtiborsiz
                }
                ops.clear();
            }
        }
        i += 1;
    }
}

fn nth(ops: &[f32], from_end_rev: usize) -> f32 {
    // Operandlar ketma-ket: oxirgi operator argumentlari oxirida
    // from_end_rev 0 = birinçi argument. Argument soni operator turiga bogʻliq,
    // shuning uçun oxiridan teskari emas, boşidan olamiz (ops faqat şu operator uçun).
    ops.get(from_end_rev).copied().unwrap_or(0.0)
}

fn scale_of(m: &Mat) -> f32 {
    vector::fsqrt(vector::fabs(m.0 * m.3 - m.1 * m.2))
}

fn cmyk(ops: &[f32]) -> u32 {
    let (c, m, y, k) = (nth(ops, 0), nth(ops, 1), nth(ops, 2), nth(ops, 3));
    rgbf((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k))
}

fn rgbf(r: f32, g: f32, b: f32) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    (c(r) << 16) | (c(g) << 8) | c(b)
}

enum CTok {
    Num(f32),
    Op(String),
    Str(String),
}

fn tokenize(d: &[u8]) -> Vec<CTok> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let c = d[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'%' => {
                while i < d.len() && d[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' => {
                let mut depth = 1;
                i += 1;
                let mut s = alloc::string::String::new();
                while i < d.len() && depth > 0 {
                    match d[i] {
                        b'\\' => {
                            i += 1;
                            if let Some(&c) = d.get(i) {
                                s.push(match c {
                                    b'n' => '\n',
                                    b't' => '\t',
                                    _ => c as char,
                                });
                            }
                        }
                        b'(' => {
                            depth += 1;
                            s.push('(');
                        }
                        b')' => {
                            depth -= 1;
                            if depth > 0 {
                                s.push(')');
                            }
                        }
                        c => s.push(c as char),
                    }
                    i += 1;
                }
                out.push(CTok::Str(s));
            }
            b'<' | b'[' | b']' | b'>' | b'{' | b'}' => {
                // massiv/lugʻat/hex — ichidagi sonlarni hisobga olmaymiz (soddalaştiriş)
                i += 1;
            }
            b'/' => {
                i += 1;
                while i < d.len() && !is_ws(d[i]) {
                    i += 1;
                }
            }
            _ => {
                let start = i;
                while i < d.len() && !is_ws(d[i]) && !matches!(d[i], b'(' | b'[' | b'<' | b'/') {
                    i += 1;
                }
                let word = core::str::from_utf8(&d[start..i]).unwrap_or("");
                if word.is_empty() {
                    i += 1;
                    continue;
                }
                let first = word.as_bytes()[0];
                if first.is_ascii_digit() || first == b'-' || first == b'+' || first == b'.' {
                    if let Some(n) = parse_f32(word) {
                        out.push(CTok::Num(n));
                        continue;
                    }
                }
                out.push(CTok::Op(String::from(word)));
            }
        }
    }
    out
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\0' | 0x0c)
}
