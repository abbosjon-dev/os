//! EPS/PostScript dekoderi — kiçik PS interpretatori (noldan).
//! Qöllanadi: yöl operatorlari (moveto/lineto/curveto/closepath/fill/stroke),
//! ranglar (rgb/gray/cmyk), CTM (translate/scale/rotate), def/gsave/grestore,
//! stek amallari va boşqaruv (if/ifelse/for/repeat/exec). Matn/rasm/font — yöq.

use super::svg::parse_f32;
use super::vector::{self, flatten_cubic, Pt, Scene, Shape};
use super::Image;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone)]
enum Tok {
    Num(f32),
    Lit(String),  // /name
    Exec(String), // name
    Proc(Vec<Tok>),
    Str,
}

#[derive(Clone)]
enum Val {
    Num(f32),
    Name(String),
    Proc(Vec<Tok>),
    Bool(bool),
}

#[derive(Clone, Copy)]
struct Mat {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}
impl Mat {
    fn id() -> Mat {
        Mat { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
    }
    fn apply(&self, x: f32, y: f32) -> Pt {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
    fn mul(&self, o: &Mat) -> Mat {
        // o keyin self (o * self): avval o, keyin self
        Mat {
            a: o.a * self.a + o.b * self.c,
            b: o.a * self.b + o.b * self.d,
            c: o.c * self.a + o.d * self.c,
            d: o.c * self.b + o.d * self.d,
            e: o.e * self.a + o.f * self.c + self.e,
            f: o.e * self.b + o.f * self.d + self.f,
        }
    }
}

#[derive(Clone)]
struct GState {
    ctm: Mat,
    color: u32,
    lw: f32,
}

struct Interp {
    stack: Vec<Val>,
    dict: Vec<(String, Val)>,
    gs: GState,
    gstack: Vec<GState>,
    cur: Vec<Pt>,          // joriy subpath (qurilma koordinatasida)
    subpaths: Vec<Vec<Pt>>, // joriy yöl
    start: Pt,
    pt: Pt, // joriy nuqta (foydalanuvçi koordinatasida)
    scene: Scene,
    steps: usize,
}

pub fn decode(d: &[u8]) -> Option<Image> {
    let text = ascii(d);
    // %%BoundingBox
    let (mut w, mut h) = (612.0f32, 792.0f32);
    let mut bx = 0.0;
    let mut by = 0.0;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("%%BoundingBox:") {
            let n: Vec<f32> = rest.split_whitespace().filter_map(parse_f32).collect();
            if n.len() == 4 && n[2] > n[0] && n[3] > n[1] {
                bx = n[0];
                by = n[1];
                w = n[2] - n[0];
                h = n[3] - n[1];
            }
            break;
        }
    }
    let toks = tokenize(&text);
    let mut it = Interp {
        stack: Vec::new(),
        dict: Vec::new(),
        gs: GState { ctm: Mat::id(), color: 0x000000, lw: 1.0 },
        gstack: Vec::new(),
        cur: Vec::new(),
        subpaths: Vec::new(),
        start: (0.0, 0.0),
        pt: (0.0, 0.0),
        scene: Scene::new(w, h),
        steps: 0,
    };
    // Koordinatalarni BoundingBox ga ko'çiramiz va y ni ag'daramiz (PS y-up)
    it.gs.ctm = Mat { a: 1.0, b: 0.0, c: 0.0, d: -1.0, e: -bx, f: h + by };
    it.scene.bg = 0xffffff;
    it.run(&toks);
    if it.scene.shapes.is_empty() {
        return None;
    }
    Some(vector::render(&it.scene, 1000))
}

fn ascii(d: &[u8]) -> String {
    // EPS binar sarlavha (EPSF) bölsa — ASCII qismini olamiz
    d.iter().map(|&b| if b == b'\r' { b'\n' } else { b } as char).collect()
}

fn tokenize(s: &str) -> Vec<Tok> {
    let b = s.as_bytes();
    let mut i = 0;
    tokenize_until(b, &mut i, false)
}

fn tokenize_until(b: &[u8], i: &mut usize, in_proc: bool) -> Vec<Tok> {
    let mut out = Vec::new();
    while *i < b.len() {
        let c = b[*i];
        match c {
            b'%' => {
                while *i < b.len() && b[*i] != b'\n' {
                    *i += 1;
                }
            }
            b' ' | b'\t' | b'\n' | b'\r' => *i += 1,
            b'{' => {
                *i += 1;
                out.push(Tok::Proc(tokenize_until(b, i, true)));
            }
            b'}' => {
                *i += 1;
                if in_proc {
                    return out;
                }
            }
            b'(' => {
                // satr — muvozanatli qavslar
                let mut depth = 1;
                *i += 1;
                while *i < b.len() && depth > 0 {
                    match b[*i] {
                        b'\\' => *i += 1,
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    *i += 1;
                }
                out.push(Tok::Str);
            }
            b'[' | b']' => {
                out.push(Tok::Exec(String::from(if c == b'[' { "[" } else { "]" })));
                *i += 1;
            }
            b'/' => {
                *i += 1;
                let start = *i;
                while *i < b.len() && !is_delim(b[*i]) {
                    *i += 1;
                }
                out.push(Tok::Lit(String::from_utf8_lossy(&b[start..*i]).into_owned()));
            }
            _ => {
                let start = *i;
                while *i < b.len() && !is_delim(b[*i]) {
                    *i += 1;
                }
                let word = core::str::from_utf8(&b[start..*i]).unwrap_or("");
                if let Some(num) = parse_f32(word) {
                    if word.bytes().next().map(|c| c.is_ascii_digit() || c == b'-' || c == b'.' || c == b'+').unwrap_or(false) {
                        out.push(Tok::Num(num));
                        continue;
                    }
                }
                if !word.is_empty() {
                    out.push(Tok::Exec(String::from(word)));
                }
            }
        }
    }
    out
}

fn is_delim(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'{' | b'}' | b'[' | b']' | b'(' | b')' | b'/' | b'%')
}

impl Interp {
    fn run(&mut self, toks: &[Tok]) {
        for t in toks {
            self.steps += 1;
            if self.steps > 4_000_000 {
                return;
            }
            match t {
                Tok::Num(n) => self.stack.push(Val::Num(*n)),
                Tok::Lit(s) => self.stack.push(Val::Name(s.clone())),
                Tok::Proc(p) => self.stack.push(Val::Proc(p.clone())),
                Tok::Str => self.stack.push(Val::Bool(false)), // satr — eʼtiborsiz joʻy toʻldiruvçi
                Tok::Exec(name) => self.exec(name),
            }
        }
    }

    fn num(&mut self) -> f32 {
        match self.stack.pop() {
            Some(Val::Num(n)) => n,
            _ => 0.0,
        }
    }

    fn lookup(&self, name: &str) -> Option<Val> {
        self.dict.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    fn exec(&mut self, name: &str) {
        // Avval lugʻatda (foydalanuvçi aniqlagan procedura)
        if let Some(v) = self.lookup(name) {
            match v {
                Val::Proc(p) => self.run(&p),
                other => self.stack.push(other),
            }
            return;
        }
        match name {
            "moveto" => {
                let y = self.num();
                let x = self.num();
                self.flush_sub();
                self.pt = (x, y);
                self.start = (x, y);
                self.cur.push(self.dev(x, y));
            }
            "rmoveto" => {
                let y = self.num();
                let x = self.num();
                self.flush_sub();
                self.pt = (self.pt.0 + x, self.pt.1 + y);
                self.start = self.pt;
                self.cur.push(self.dev(self.pt.0, self.pt.1));
            }
            "lineto" => {
                let y = self.num();
                let x = self.num();
                self.pt = (x, y);
                self.cur.push(self.dev(x, y));
            }
            "rlineto" => {
                let y = self.num();
                let x = self.num();
                self.pt = (self.pt.0 + x, self.pt.1 + y);
                self.cur.push(self.dev(self.pt.0, self.pt.1));
            }
            "curveto" => {
                let y3 = self.num();
                let x3 = self.num();
                let y2 = self.num();
                let x2 = self.num();
                let y1 = self.num();
                let x1 = self.num();
                let p0 = self.dev(self.pt.0, self.pt.1);
                let p1 = self.dev(x1, y1);
                let p2 = self.dev(x2, y2);
                let p3 = self.dev(x3, y3);
                flatten_cubic(&mut self.cur, p0, p1, p2, p3, 18);
                self.pt = (x3, y3);
            }
            "rcurveto" => {
                let dy3 = self.num();
                let dx3 = self.num();
                let dy2 = self.num();
                let dx2 = self.num();
                let dy1 = self.num();
                let dx1 = self.num();
                let (px, py) = self.pt;
                let p0 = self.dev(px, py);
                let p1 = self.dev(px + dx1, py + dy1);
                let p2 = self.dev(px + dx2, py + dy2);
                let p3 = self.dev(px + dx3, py + dy3);
                flatten_cubic(&mut self.cur, p0, p1, p2, p3, 18);
                self.pt = (px + dx3, py + dy3);
            }
            "closepath" => {
                if !self.cur.is_empty() {
                    let f = self.cur[0];
                    self.cur.push(f);
                }
                self.pt = self.start;
            }
            "newpath" => {
                self.cur.clear();
                self.subpaths.clear();
            }
            "fill" | "eofill" => {
                self.flush_sub();
                let paths = core::mem::take(&mut self.subpaths);
                if !paths.is_empty() {
                    self.scene.shapes.push(Shape {
                        subpaths: paths,
                        fill: Some(self.gs.color),
                        stroke: None,
                        stroke_w: 0.0,
                        nonzero: name == "fill",
                    });
                }
            }
            "stroke" => {
                self.flush_sub();
                let paths = core::mem::take(&mut self.subpaths);
                if !paths.is_empty() {
                    let sc = self.scale_factor();
                    self.scene.shapes.push(Shape {
                        subpaths: paths,
                        fill: None,
                        stroke: Some(self.gs.color),
                        stroke_w: (self.gs.lw * sc).max(0.3),
                        nonzero: false,
                    });
                }
            }
            "setrgbcolor" => {
                let b = self.num();
                let g = self.num();
                let r = self.num();
                self.gs.color = rgbf(r, g, b);
            }
            "setgray" => {
                let g = self.num();
                self.gs.color = rgbf(g, g, g);
            }
            "sethsbcolor" => {
                let _b = self.num();
                let _s = self.num();
                let _h = self.num();
                self.gs.color = 0x808080;
            }
            "setcmykcolor" => {
                let k = self.num();
                let y = self.num();
                let m = self.num();
                let c = self.num();
                self.gs.color = rgbf((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k));
            }
            "setlinewidth" => self.gs.lw = self.num(),
            "translate" => {
                let ty = self.num();
                let tx = self.num();
                self.gs.ctm = self.gs.ctm.mul(&Mat { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: tx, f: ty });
            }
            "scale" => {
                let sy = self.num();
                let sx = self.num();
                self.gs.ctm = self.gs.ctm.mul(&Mat { a: sx, b: 0.0, c: 0.0, d: sy, e: 0.0, f: 0.0 });
            }
            "rotate" => {
                let a = self.num() * 3.14159265 / 180.0;
                let (s, co) = (vector::sin(a), vector::cos(a));
                self.gs.ctm = self.gs.ctm.mul(&Mat { a: co, b: s, c: -s, d: co, e: 0.0, f: 0.0 });
            }
            "gsave" => self.gstack.push(self.gs.clone()),
            "grestore" => {
                if let Some(g) = self.gstack.pop() {
                    self.gs = g;
                }
            }
            "def" => {
                let v = self.stack.pop();
                let k = self.stack.pop();
                if let (Some(Val::Name(k)), Some(v)) = (k, v) {
                    self.dict.push((k, v));
                }
            }
            "dup" => {
                if let Some(v) = self.stack.last().cloned() {
                    self.stack.push(v);
                }
            }
            "pop" => {
                self.stack.pop();
            }
            "exch" => {
                let n = self.stack.len();
                if n >= 2 {
                    self.stack.swap(n - 1, n - 2);
                }
            }
            "exec" => {
                if let Some(Val::Proc(p)) = self.stack.pop() {
                    self.run(&p);
                }
            }
            "if" => {
                let p = self.stack.pop();
                let cond = self.stack.pop();
                if let (Some(Val::Bool(true)), Some(Val::Proc(p))) = (cond, p) {
                    self.run(&p);
                }
            }
            "ifelse" => {
                let p2 = self.stack.pop();
                let p1 = self.stack.pop();
                let cond = self.stack.pop();
                match (cond, p1, p2) {
                    (Some(Val::Bool(true)), Some(Val::Proc(p)), _) => self.run(&p),
                    (Some(Val::Bool(false)), _, Some(Val::Proc(p))) => self.run(&p),
                    _ => {}
                }
            }
            "for" => {
                let proc = self.stack.pop();
                let limit = self.num();
                let incr = self.num();
                let init = self.num();
                if let Some(Val::Proc(p)) = proc {
                    let mut i = init;
                    let mut guard = 0;
                    while (incr > 0.0 && i <= limit) || (incr < 0.0 && i >= limit) {
                        self.stack.push(Val::Num(i));
                        self.run(&p);
                        i += incr;
                        guard += 1;
                        if guard > 100000 || incr == 0.0 {
                            break;
                        }
                    }
                }
            }
            "repeat" => {
                let proc = self.stack.pop();
                let n = self.num() as i32;
                if let Some(Val::Proc(p)) = proc {
                    for _ in 0..n.min(100000) {
                        self.run(&p);
                    }
                }
            }
            "add" => {
                let b = self.num();
                let a = self.num();
                self.stack.push(Val::Num(a + b));
            }
            "sub" => {
                let b = self.num();
                let a = self.num();
                self.stack.push(Val::Num(a - b));
            }
            "mul" => {
                let b = self.num();
                let a = self.num();
                self.stack.push(Val::Num(a * b));
            }
            "div" => {
                let b = self.num();
                let a = self.num();
                self.stack.push(Val::Num(if b != 0.0 { a / b } else { 0.0 }));
            }
            "neg" => {
                let a = self.num();
                self.stack.push(Val::Num(-a));
            }
            "true" => self.stack.push(Val::Bool(true)),
            "false" => self.stack.push(Val::Bool(false)),
            // Eʼtiborsiz (lekin stekni buzmaslik uçun baʼzilari)
            "showpage" | "newpath2" | "bind" | "load" | "setlinecap" | "setlinejoin" | "setmiterlimit"
            | "setdash" | "stroketopath" | "clip" | "eoclip" | "[" | "]" | "gsave2" => {}
            _ => {} // nomaʼlum operatorlarni ötkazamiz
        }
    }

    fn dev(&self, x: f32, y: f32) -> Pt {
        self.gs.ctm.apply(x, y)
    }
    fn scale_factor(&self) -> f32 {
        let m = &self.gs.ctm;
        vector::fsqrt(vector::fabs(m.a * m.d - m.b * m.c))
    }
    fn flush_sub(&mut self) {
        if self.cur.len() >= 2 {
            self.subpaths.push(core::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }
}

fn rgbf(r: f32, g: f32, b: f32) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    (c(r) << 16) | (c(g) << 8) | c(b)
}
