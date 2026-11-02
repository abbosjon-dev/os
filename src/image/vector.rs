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

/// Çizma sahnasi — [0,w]×[0,h] koordinata maydoni (yuqoridan pastga, y-down).
pub struct Scene {
    pub w: f32,
    pub h: f32,
    pub bg: u32,
    pub shapes: Vec<Shape>,
}

impl Scene {
    pub fn new(w: f32, h: f32) -> Scene {
        Scene { w: w.max(1.0), h: h.max(1.0), bg: 0xffffff, shapes: Vec::new() }
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

/// Koʻpburçakni (AA) Image ga töldiradi (2x2 supersample).
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
    let x1 = (fmin(maxx, img.w as f32 - 1.0) as usize).min(img.w.saturating_sub(1));
    let y1 = (fmin(maxy, img.h as f32 - 1.0) as usize).min(img.h.saturating_sub(1));
    for py in y0..=y1 {
        for px in x0..=x1 {
            let mut hit = 0u32;
            for sy in 0..2 {
                for sx in 0..2 {
                    let fx = px as f32 + 0.25 + sx as f32 * 0.5;
                    let fy = py as f32 + 0.25 + sy as f32 * 0.5;
                    if winding(subpaths, fx, fy, nonzero) {
                        hit += 1;
                    }
                }
            }
            if hit > 0 {
                let a = hit * 255 / 4;
                blend_px(img, px, py, color, a);
            }
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
    img
}
