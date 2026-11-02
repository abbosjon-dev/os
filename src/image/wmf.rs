//! WMF/EMF (Windows Metafile) dekoderi — GDI yozuvlari (noldan).
//! Qöllanadi: Polygon, Polyline, Rectangle, Ellipse, PolyPolygon, MoveTo/LineTo,
//! qalam/çötka obyektlari (rang). Matn/rasm — hozirça yöq.

use super::vector::{self, flatten_arc, Pt, Scene, Shape};
use super::Image;
use alloc::vec::Vec;

fn u16le(d: &[u8], o: usize) -> u32 {
    *d.get(o).unwrap_or(&0) as u32 | (*d.get(o + 1).unwrap_or(&0) as u32) << 8
}
fn u32le(d: &[u8], o: usize) -> u32 {
    u16le(d, o) | u16le(d, o + 2) << 16
}
fn i16le(d: &[u8], o: usize) -> i32 {
    u16le(d, o) as i16 as i32
}
fn i32le(d: &[u8], o: usize) -> i32 {
    u32le(d, o) as i32
}
fn colorref(c: u32) -> u32 {
    // 0x00BBGGRR → RGB
    let r = c & 0xff;
    let g = (c >> 8) & 0xff;
    let b = (c >> 16) & 0xff;
    (r << 16) | (g << 8) | b
}

#[derive(Clone, Copy)]
enum GObj {
    Pen(u32, f32),
    Brush(Option<u32>), // None = null (toʻldiriş yöq)
    None,
}

struct St {
    objs: Vec<GObj>,
    pen: (u32, f32),
    brush: Option<u32>,
    cur: Pt,
    shapes: Vec<Shape>,
    minx: f32,
    miny: f32,
    maxx: f32,
    maxy: f32,
}

impl St {
    fn new() -> St {
        St {
            objs: Vec::new(),
            pen: (0x000000, 1.0),
            brush: Some(0xffffff),
            cur: (0.0, 0.0),
            shapes: Vec::new(),
            minx: f32::MAX,
            miny: f32::MAX,
            maxx: f32::MIN,
            maxy: f32::MIN,
        }
    }
    fn bound(&mut self, pts: &[Pt]) {
        for &(x, y) in pts {
            self.minx = self.minx.min(x);
            self.miny = self.miny.min(y);
            self.maxx = self.maxx.max(x);
            self.maxy = self.maxy.max(y);
        }
    }
    fn add_poly(&mut self, pts: Vec<Pt>, closed: bool) {
        if pts.len() < 2 {
            return;
        }
        self.bound(&pts);
        let mut sp = pts;
        if closed && sp.len() > 2 {
            let f = sp[0];
            sp.push(f);
        }
        self.shapes.push(Shape {
            subpaths: alloc::vec![sp],
            fill: if closed { self.brush } else { None },
            stroke: Some(self.pen.0),
            stroke_w: self.pen.1,
            nonzero: false,
        });
    }
    fn select(&mut self, i: usize) {
        if let Some(o) = self.objs.get(i) {
            match *o {
                GObj::Pen(c, w) => self.pen = (c, w),
                GObj::Brush(b) => self.brush = b,
                GObj::None => {}
            }
        }
    }
    fn add_obj(&mut self, o: GObj) {
        // birinçi boʻsh joyni topamiz
        if let Some(slot) = self.objs.iter_mut().find(|x| matches!(x, GObj::None)) {
            *slot = o;
        } else {
            self.objs.push(o);
        }
    }
}

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() >= 44 && u32le(d, 40) == 0x464D4520 {
        decode_emf(d)
    } else {
        decode_wmf(d)
    }
}

fn finish(st: St) -> Option<Image> {
    if st.shapes.is_empty() || st.minx > st.maxx {
        return None;
    }
    let (w, h) = ((st.maxx - st.minx).max(1.0), (st.maxy - st.miny).max(1.0));
    let mut scene = Scene::new(w, h);
    scene.bg = 0xffffff;
    for mut s in st.shapes {
        for sp in &mut s.subpaths {
            for p in sp {
                p.0 -= st.minx;
                p.1 -= st.miny;
            }
        }
        scene.shapes.push(s);
    }
    Some(vector::render(&scene, 1000))
}

// ---- WMF ----
fn decode_wmf(d: &[u8]) -> Option<Image> {
    let mut pos;
    // Placeable sarlavha? (magic 0x9AC6CDD7, baytlarda D7 CD C6 9A)
    if u32le(d, 0) == 0x9AC6CDD7 {
        pos = 22;
    } else if (d[0] == 1 || d[0] == 2) && d[1] == 0 && d[2] == 9 {
        pos = 0;
    } else {
        return None;
    }
    // Standart sarlavha (18 bayt)
    pos += 18;
    let mut st = St::new();
    let mut guard = 0;
    while pos + 6 <= d.len() {
        guard += 1;
        if guard > 200000 {
            break;
        }
        let size = u32le(d, pos) as usize; // WORD larda
        let func = u16le(d, pos + 4);
        if size < 3 {
            break;
        }
        let params = pos + 6;
        match func {
            0x0325 => wmf_poly(&mut st, d, params, false), // META_POLYLINE
            0x0324 => wmf_poly(&mut st, d, params, true),  // META_POLYGON
            0x0538 => wmf_polypolygon(&mut st, d, params), // META_POLYPOLYGON
            0x041B => wmf_rect(&mut st, d, params),        // META_RECTANGLE
            0x0418 => wmf_ellipse(&mut st, d, params),     // META_ELLIPSE
            0x0214 => st.cur = (i16le(d, params + 2) as f32, i16le(d, params) as f32), // MOVETO (y,x)
            0x0213 => {
                // LINETO
                let p = (i16le(d, params + 2) as f32, i16le(d, params) as f32);
                st.add_poly(alloc::vec![st.cur, p], false);
                st.cur = p;
            }
            0x02FA => {
                // CREATEPENINDIRECT: style(2) width(4:x,y) color(4)
                let w = i16le(d, params + 2) as f32;
                let c = colorref(u32le(d, params + 6));
                st.add_obj(GObj::Pen(c, w.max(1.0)));
            }
            0x02FC => {
                // CREATEBRUSHINDIRECT: style(2) color(4) hatch(2)
                let style = u16le(d, params);
                let c = colorref(u32le(d, params + 2));
                st.add_obj(GObj::Brush(if style == 1 { None } else { Some(c) })); // 1 = BS_NULL
            }
            0x012D => st.select(u16le(d, params) as usize), // SELECTOBJECT
            0x01F0 => {
                // DELETEOBJECT
                let i = u16le(d, params) as usize;
                if let Some(o) = st.objs.get_mut(i) {
                    *o = GObj::None;
                }
            }
            _ => {}
        }
        pos += size * 2;
    }
    finish(st)
}

fn wmf_poly(st: &mut St, d: &[u8], params: usize, closed: bool) {
    let n = u16le(d, params) as usize;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let o = params + 2 + i * 4;
        pts.push((i16le(d, o) as f32, i16le(d, o + 2) as f32));
    }
    st.add_poly(pts, closed);
}

fn wmf_polypolygon(st: &mut St, d: &[u8], params: usize) {
    let npoly = u16le(d, params) as usize;
    let mut counts = Vec::with_capacity(npoly);
    for i in 0..npoly {
        counts.push(u16le(d, params + 2 + i * 2) as usize);
    }
    let mut o = params + 2 + npoly * 2;
    for c in counts {
        let mut pts = Vec::with_capacity(c);
        for _ in 0..c {
            pts.push((i16le(d, o) as f32, i16le(d, o + 2) as f32));
            o += 4;
        }
        st.add_poly(pts, true);
    }
}

fn wmf_rect(st: &mut St, d: &[u8], params: usize) {
    // bottom right top left (WORD) — tartib: left top right bottom teskari saqlanadi
    let b = i16le(d, params) as f32;
    let r = i16le(d, params + 2) as f32;
    let t = i16le(d, params + 4) as f32;
    let l = i16le(d, params + 6) as f32;
    st.add_poly(alloc::vec![(l, t), (r, t), (r, b), (l, b)], true);
}

fn wmf_ellipse(st: &mut St, d: &[u8], params: usize) {
    let b = i16le(d, params) as f32;
    let r = i16le(d, params + 2) as f32;
    let t = i16le(d, params + 4) as f32;
    let l = i16le(d, params + 6) as f32;
    let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
    let (rx, ry) = (vector::fabs(r - l) / 2.0, vector::fabs(b - t) / 2.0);
    let mut pts = Vec::new();
    flatten_arc(&mut pts, cx, cy, rx, ry, 0.0, 6.2831853, 48);
    st.add_poly(pts, true);
}

// ---- EMF ----
fn decode_emf(d: &[u8]) -> Option<Image> {
    let mut st = St::new();
    let mut pos = 0;
    let mut guard = 0;
    while pos + 8 <= d.len() {
        guard += 1;
        if guard > 500000 {
            break;
        }
        let typ = u32le(d, pos);
        let size = u32le(d, pos + 4) as usize;
        if size < 8 || pos + size > d.len() {
            break;
        }
        let p = pos + 8;
        match typ {
            3 => emf_poly(&mut st, d, p, true, false),   // EMR_POLYGON
            4 => emf_poly(&mut st, d, p, false, false),  // EMR_POLYLINE
            86 => emf_poly(&mut st, d, p, true, true),   // EMR_POLYGON16
            87 => emf_poly(&mut st, d, p, false, true),  // EMR_POLYLINE16
            43 => emf_rect(&mut st, d, p),               // EMR_RECTANGLE
            42 => emf_ellipse(&mut st, d, p),            // EMR_ELLIPSE
            27 => st.cur = (i32le(d, p) as f32, i32le(d, p + 4) as f32), // EMR_MOVETOEX
            54 => {
                // EMR_LINETO
                let np = (i32le(d, p) as f32, i32le(d, p + 4) as f32);
                st.add_poly(alloc::vec![st.cur, np], false);
                st.cur = np;
            }
            38 => {
                // EMR_CREATEPEN: ihPen(4) logpen: style(4) width(x4,y4) color(4)
                let w = i32le(d, p + 8) as f32;
                let c = colorref(u32le(d, p + 16));
                st.add_obj(GObj::Pen(c, w.max(1.0)));
            }
            39 => {
                // EMR_CREATEBRUSHINDIRECT: ihBrush(4) style(4) color(4) hatch(4)
                let style = u32le(d, p + 4);
                let c = colorref(u32le(d, p + 8));
                st.add_obj(GObj::Brush(if style == 1 { None } else { Some(c) }));
            }
            37 => st.select((u32le(d, p) & 0x7fffffff) as usize), // EMR_SELECTOBJECT
            _ => {}
        }
        pos += size;
    }
    finish(st)
}

fn emf_poly(st: &mut St, d: &[u8], p: usize, closed: bool, p16: bool) {
    // bounds(16) count(4) points...
    let count = u32le(d, p + 16) as usize;
    let base = p + 20;
    let mut pts = Vec::with_capacity(count);
    for i in 0..count.min(100000) {
        if p16 {
            let o = base + i * 4;
            pts.push((i16le(d, o) as f32, i16le(d, o + 2) as f32));
        } else {
            let o = base + i * 8;
            pts.push((i32le(d, o) as f32, i32le(d, o + 4) as f32));
        }
    }
    st.add_poly(pts, closed);
}

fn emf_rect(st: &mut St, d: &[u8], p: usize) {
    let (l, t, r, b) = (i32le(d, p) as f32, i32le(d, p + 4) as f32, i32le(d, p + 8) as f32, i32le(d, p + 12) as f32);
    st.add_poly(alloc::vec![(l, t), (r, t), (r, b), (l, b)], true);
}

fn emf_ellipse(st: &mut St, d: &[u8], p: usize) {
    let (l, t, r, b) = (i32le(d, p) as f32, i32le(d, p + 4) as f32, i32le(d, p + 8) as f32, i32le(d, p + 12) as f32);
    let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
    let (rx, ry) = (vector::fabs(r - l) / 2.0, vector::fabs(b - t) / 2.0);
    let mut pts = Vec::new();
    flatten_arc(&mut pts, cx, cy, rx, ry, 0.0, 6.2831853, 48);
    st.add_poly(pts, true);
}
