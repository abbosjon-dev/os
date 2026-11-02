//! Vektor grafika poydevori: sahna (shakllar) va rasterizator (Image ga çizadi).
//! SVG, PDF, EPS, DXF, WMF/EMF kabi formatlar şunga tayanadi.
//! Toʻldiriş — silliq (anti-aliased), even-odd yoki nonzero qoidasi bilan.

#![allow(dead_code)]

use super::Image;
use alloc::vec::Vec;

pub type Pt = (f32, f32);

/// Bitta shakl: bir yoki bir neçta yopiq/oçiq egri (polilin sifatida yassilangan).
pub struct Shape {
    pub subpaths: Vec<Vec<Pt>>,
    pub fill: Option<u32>,
    pub stroke: Option<u32>,
    pub stroke_w: f32,
    pub nonzero: bool, // toʻldiriş qoidasi
}

/// Matn yozuvi (tayanç chizigʻi — baseline, sahna koordinatasida).
pub struct Text {
    pub x: f32,
    pub baseline: f32,
    pub size: f32,
    pub color: u32,
    pub s: alloc::string::String,
    pub mono: bool,
}

/// Çizma sahnasi — [0,w]×[0,h] koordinata maydoni (yuqoridan pastga, y-down).
pub struct Scene {
    pub w: f32,
    pub h: f32,
    pub bg: u32,
    pub shapes: Vec<Shape>,
    pub texts: Vec<Text>,
}

impl Scene {
    pub fn new(w: f32, h: f32) -> Scene {
        Scene { w: w.max(1.0), h: h.max(1.0), bg: 0xffffff, shapes: Vec::new(), texts: Vec::new() }
    }
}

// ---- f32 yordamçilar (no_std) ----
pub fn fabs(x: f32) -> f32 {
    if x < 0.0 { -x } else { x }
}
fn fmin(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}
fn fmax(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}
pub fn fsqrt(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut g = x;
    for _ in 0..20 {
        g = 0.5 * (g + x / g);
    }
    g
}

/// Kub Bezier egrisini polilinga yassilaydi (nuqtalar qoşiladi, p0 dan taşqari).
pub fn flatten_cubic(out: &mut Vec<Pt>, p0: Pt, p1: Pt, p2: Pt, p3: Pt, steps: usize) {
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        let x = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
        let y = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
        out.push((x, y));
    }
}

/// Kvadratik Bezier.
pub fn flatten_quad(out: &mut Vec<Pt>, p0: Pt, p1: Pt, p2: Pt, steps: usize) {
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        let x = u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0;
        let y = u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1;
        out.push((x, y));
    }
}

/// Markaz, radius, boşlangʼiç/oxirgi burçak (radian) böyiça yoyni yassilaydi.
pub fn flatten_arc(out: &mut Vec<Pt>, cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32, steps: usize) {
    for i in 0..=steps {
        let t = a0 + (a1 - a0) * i as f32 / steps as f32;
        out.push((cx + rx * cos(t), cy + ry * sin(t)));
    }
}

const PI: f32 = 3.14159265;
pub fn sin(mut x: f32) -> f32 {
    // normalize to -PI..PI
    while x > PI {
        x -= 2.0 * PI;
    }
    while x < -PI {
        x += 2.0 * PI;
    }
    // Taylor (yetarli aniqlik)
    let x2 = x * x;
    x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))))
}
pub fn cos(x: f32) -> f32 {
    sin(x + PI / 2.0)
}
pub fn atan2(y: f32, x: f32) -> f32 {
    if x > 0.0 {
        atan(y / x)
    } else if x < 0.0 && y >= 0.0 {
        atan(y / x) + PI
    } else if x < 0.0 {
        atan(y / x) - PI
    } else if y > 0.0 {
        PI / 2.0
    } else if y < 0.0 {
        -PI / 2.0
    } else {
        0.0
    }
}
fn atan(x: f32) -> f32 {
    // |x|<=1 uçun qator; aks holda identity
    if fabs(x) > 1.0 {
        let s = if x > 0.0 { 1.0 } else { -1.0 };
        return s * PI / 2.0 - atan(1.0 / x);
    }
    let x2 = x * x;
    x * (1.0 - x2 / 3.0 + x2 * x2 / 5.0 - x2 * x2 * x2 / 7.0 + x2 * x2 * x2 * x2 / 9.0)
}

// ---- Rasterizator ----

/// Nuqta koʻpburçak(lar) içidami (even-odd yoki nonzero).
fn winding(subpaths: &[Vec<Pt>], px: f32, py: f32, nonzero: bool) -> bool {
    if nonzero {
        let mut wn = 0i32;
        for sp in subpaths {
            let n = sp.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let (x0, y0) = sp[i];
                let (x1, y1) = sp[(i + 1) % n];
                if y0 <= py {
                    if y1 > py && cross(x0, y0, x1, y1, px, py) > 0.0 {
                        wn += 1;
                    }
                } else if y1 <= py && cross(x0, y0, x1, y1, px, py) < 0.0 {
                    wn -= 1;
                }
            }
        }
        wn != 0
    } else {
        let mut c = false;
        for sp in subpaths {
            let n = sp.len();
            if n < 2 {
                continue;
            }
            let mut j = n - 1;
            for i in 0..n {
                let (xi, yi) = sp[i];
                let (xj, yj) = sp[j];
                if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                    c = !c;
                }
                j = i;
            }
        }
        c
    }
}

fn cross(x0: f32, y0: f32, x1: f32, y1: f32, px: f32, py: f32) -> f32 {
    (x1 - x0) * (py - y0) - (y1 - y0) * (px - x0)
}

/// Koʻpburçakni (AA) Image ga töldiradi — tez skanlin rasterizatsiya.
/// Vertikal 4x supersample + aniq gorizontal qoplaş (coverage).
fn fill_shape(img: &mut Image, subpaths: &[Vec<Pt>], color: u32, nonzero: bool) {
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for sp in subpaths {
        for &(x, y) in sp {
            minx = fmin(minx, x);
            miny = fmin(miny, y);
            maxx = fmax(maxx, x);
            maxy = fmax(maxy, y);
        }
    }
    if minx > maxx {
        return;
    }
    let x0 = fmax(minx, 0.0) as usize;
    let y0 = fmax(miny, 0.0) as usize;
    let x1 = (fmax(maxx, 0.0) as usize + 1).min(img.w);
    let y1 = (fmax(maxy, 0.0) as usize + 1).min(img.h);
    if x1 <= x0 {
        return;
    }
    const SS: usize = 4; // vertikal subpiksel
    let span_w = x1 - x0;
    let mut acc = alloc::vec![0f32; span_w];
    let mut xs: Vec<(f32, i32)> = Vec::new();
    for py in y0..y1 {
        for a in acc.iter_mut() {
            *a = 0.0;
        }
        for s in 0..SS {
            let sy = py as f32 + (s as f32 + 0.5) / SS as f32;
            xs.clear();
            for sp in subpaths {
                let n = sp.len();
                for i in 0..n {
                    let (ax, ay) = sp[i];
                    let (bx, by) = sp[(i + 1) % n];
                    // sy chetini kesadimi?
                    let (dir, lo, hi, x_lo, x_hi) = if ay < by {
                        (1, ay, by, ax, bx)
                    } else {
                        (-1, by, ay, bx, ax)
                    };
                    if sy >= lo && sy < hi && (hi - lo) > 0.0 {
                        let t = (sy - lo) / (hi - lo);
                        xs.push((x_lo + (x_hi - x_lo) * t, dir));
                    }
                }
            }
            if xs.len() < 2 {
                continue;
            }
            xs.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(core::cmp::Ordering::Equal));
            let weight = 1.0 / SS as f32;
            let mut wind = 0i32;
            for k in 0..xs.len() - 1 {
                wind += xs[k].1;
                let inside = if nonzero { wind != 0 } else { (k + 1) % 2 == 1 };
                if inside {
                    add_span(&mut acc, x0, xs[k].0, xs[k + 1].0, weight);
                }
            }
        }
        for (i, &c) in acc.iter().enumerate() {
            if c > 0.003 {
                blend_px(img, x0 + i, py, color, (c.min(1.0) * 255.0) as u32);
            }
        }
    }
}

/// [xa,xb] oraligʻini `acc` (x0 dan boşlab) ga gorizontal qoplaş bilan qoşadi.
fn add_span(acc: &mut [f32], x0: usize, xa: f32, xb: f32, weight: f32) {
    let lo = fmax(xa, x0 as f32);
    let hi = fmin(xb, (x0 + acc.len()) as f32);
    if hi <= lo {
        return;
    }
    let ia = lo as usize;
    let ib = (hi.min((x0 + acc.len()) as f32) as usize).min(x0 + acc.len() - 1);
    for px in ia..=ib {
        let l = fmax(lo, px as f32);
        let r = fmin(hi, (px + 1) as f32);
        if r > l {
            acc[px - x0] += (r - l) * weight;
        }
    }
}

fn blend_px(img: &mut Image, x: usize, y: usize, c: u32, a: u32) {
    if x >= img.w || y >= img.h || a == 0 {
        return;
    }
    let i = y * img.w + x;
    if a >= 255 {
        img.px[i] = c & 0xffffff;
        return;
    }
    let bg = img.px[i];
    let ia = 255 - a;
    let r = (((c >> 16) & 0xff) * a + ((bg >> 16) & 0xff) * ia) / 255;
    let g = (((c >> 8) & 0xff) * a + ((bg >> 8) & 0xff) * ia) / 255;
    let b = ((c & 0xff) * a + (bg & 0xff) * ia) / 255;
    img.px[i] = (r << 16) | (g << 8) | b;
}

/// Qalin chiziq (segment) — ikki uçi dumaloq.
fn stroke_seg(img: &mut Image, p0: Pt, p1: Pt, w: f32, color: u32) {
    let dx = p1.0 - p0.0;
    let dy = p1.1 - p0.1;
    let len = fsqrt(dx * dx + dy * dy);
    if len < 0.01 {
        return;
    }
    let (nx, ny) = (-dy / len * w * 0.5, dx / len * w * 0.5);
    let quad = alloc::vec![alloc::vec![
        (p0.0 + nx, p0.1 + ny),
        (p1.0 + nx, p1.1 + ny),
        (p1.0 - nx, p1.1 - ny),
        (p0.0 - nx, p0.1 - ny),
    ]];
    fill_shape(img, &quad, color, false);
}

fn stroke_path(img: &mut Image, subpaths: &[Vec<Pt>], w: f32, color: u32) {
    let w = fmax(w, 1.0);
    for sp in subpaths {
        for i in 1..sp.len() {
            stroke_seg(img, sp[i - 1], sp[i], w, color);
        }
    }
}

/// Sahnani berilgan eng katta ölçamga sigdirib Image ga çizadi.
pub fn render(scene: &Scene, max_dim: usize) -> Image {
    let max_dim = max_dim.clamp(64, 2000);
    let aspect = scene.w / scene.h;
    let (w, h) = if aspect >= 1.0 {
        (max_dim, ((max_dim as f32 / aspect) as usize).max(1))
    } else {
        (((max_dim as f32 * aspect) as usize).max(1), max_dim)
    };
    let scale = w as f32 / scene.w;
    let mut img = Image::new(w, h);
    for p in img.px.iter_mut() {
        *p = scene.bg;
    }
    for shape in &scene.shapes {
        // Koordinatalarni masştablaymiz
        let scaled: Vec<Vec<Pt>> =
            shape.subpaths.iter().map(|sp| sp.iter().map(|&(x, y)| (x * scale, y * scale)).collect()).collect();
        if let Some(c) = shape.fill {
            fill_shape(&mut img, &scaled, c, shape.nonzero);
        }
        if let Some(c) = shape.stroke {
            stroke_path(&mut img, &scaled, fmax(shape.stroke_w * scale, 1.0), c);
        }
    }
    // Matnlar — shakllar ustidan
    for t in &scene.texts {
        super::text::draw(&mut img, t.x * scale, t.baseline * scale, &t.s, t.size * scale, t.color, t.mono);
    }
    img
}
