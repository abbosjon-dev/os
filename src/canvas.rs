//! <canvas> 2D: piksel buferi va oddiy rasterlaş (fillRect, strokeRect,
//! yöl — moveTo/lineTo/arc, fill/stroke, fillText).
//! Piksel — u32: 0xAARRGGBB (alfa 0 — şaffof; sahifa foni körinadi).

use alloc::vec::Vec;

/// Bufer ölçami çegarasi (xotira: 4 bayt/piksel).
pub const MAX_SIDE: usize = 2048;
pub const MAX_AREA: usize = 1 << 21;

pub struct Canvas {
    pub el: u16,
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
    /// Joriy yöl: kontur ro'yxati (har biri nuqtalar).
    pub path: Vec<Vec<(f32, f32)>>,
}

impl Canvas {
    pub fn new(el: u16, w: usize, h: usize) -> Canvas {
        let (w, h) = clamp_size(w, h);
        Canvas { el, w, h, px: alloc::vec![0; w * h], path: Vec::new() }
    }

    /// width/height özgarsa — bufer qayta yaratiladi (brauzerlardagidek tozalanadi).
    pub fn resize(&mut self, w: usize, h: usize) {
        let (w, h) = clamp_size(w, h);
        self.w = w;
        self.h = h;
        self.px = alloc::vec![0; w * h];
        self.path.clear();
    }

    /// "Source-over" aralaştiriş: rang c (RGB), alfa a (0..255).
    #[inline]
    fn blend(&mut self, x: usize, y: usize, c: u32, a: u32) {
        if a == 0 || x >= self.w || y >= self.h {
            return;
        }
        let d = &mut self.px[y * self.w + x];
        if a >= 255 {
            *d = 0xff00_0000 | (c & 0xffffff);
            return;
        }
        let da = *d >> 24;
        let oa = a + da * (255 - a) / 255;
        if oa == 0 {
            return;
        }
        let ch = |s: u32| -> u32 {
            let sc = (c >> s) & 0xff;
            let dc = (*d >> s) & 0xff;
            (sc * a + dc * da * (255 - a) / 255) / oa
        };
        *d = (oa << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0);
    }

    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (x0, y0, x1, y1) = self.rect_px(x, y, w, h);
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.px[yy * self.w + xx] = 0;
            }
        }
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: u32, a: u32) {
        let (x0, y0, x1, y1) = self.rect_px(x, y, w, h);
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.blend(xx, yy, c, a);
            }
        }
    }

    /// Töğriburçak koordinatalarini bufer çegarasiga qirqadi (manfiy w/h ham).
    fn rect_px(&self, x: f32, y: f32, w: f32, h: f32) -> (usize, usize, usize, usize) {
        let (xa, xb) = if w < 0.0 { (x + w, x) } else { (x, x + w) };
        let (ya, yb) = if h < 0.0 { (y + h, y) } else { (y, y + h) };
        let cl = |v: f32, m: usize| -> usize {
            if v.is_nan() || v <= 0.0 {
                0
            } else if v >= m as f32 {
                m
            } else {
                (v + 0.5) as usize
            }
        };
        (cl(xa, self.w), cl(ya, self.h), cl(xb, self.w), cl(yb, self.h))
    }

    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, lw: f32, c: u32, a: u32) {
        let ring = alloc::vec![alloc::vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]];
        self.stroke_contours(&ring, lw, c, a);
    }

    // --- Yöl ---
    pub fn begin_path(&mut self) {
        self.path.clear();
    }
    pub fn move_to(&mut self, x: f32, y: f32) {
        if self.path.len() >= 256 {
            return;
        }
        self.path.push(alloc::vec![(x, y)]);
    }
    pub fn line_to(&mut self, x: f32, y: f32) {
        match self.path.last_mut() {
            Some(p) if p.len() < 8192 => p.push((x, y)),
            Some(_) => {}
            None => self.move_to(x, y),
        }
    }
    pub fn close_path(&mut self) {
        let mut start = None;
        if let Some(p) = self.path.last_mut() {
            if p.len() > 1 {
                let f = p[0];
                p.push(f);
                start = Some(f);
            }
        }
        // keyingi lineTo yangi kontur sifatida boşlanadi (yopilgan nuqtadan)
        if let Some((x, y)) = start {
            self.move_to(x, y);
        }
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.move_to(x, y);
        for p in [(x + w, y), (x + w, y + h), (x, y + h), (x, y)] {
            self.line_to(p.0, p.1);
        }
        self.move_to(x, y);
    }
    /// arc(x, y, r, boşlanğiç, oxirgi burçak, soat millariga teskari).
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, ccw: bool) {
        use crate::image::vector::{cos, sin};
        const TAU: f32 = core::f32::consts::PI * 2.0;
        if !(r >= 0.0) || !a0.is_finite() || !a1.is_finite() {
            return;
        }
        let mut sweep = a1 - a0;
        if !ccw {
            if sweep >= TAU {
                sweep = TAU;
            } else {
                while sweep < 0.0 {
                    sweep += TAU;
                }
            }
        } else if -sweep >= TAU {
            sweep = -TAU;
        } else {
            while sweep > 0.0 {
                sweep -= TAU;
            }
        }
        let steps = ((sweep.abs() / TAU) * (r * 0.75 + 16.0).min(160.0)) as usize + 2;
        for k in 0..=steps {
            let t = a0 + sweep * k as f32 / steps as f32;
            self.line_to(cx + r * cos(t), cy + r * sin(t));
        }
    }

    /// Joriy yölni böyaydi (nonzero qoidasi).
    pub fn fill_path(&mut self, c: u32, a: u32) {
        let cs: Vec<Vec<(f32, f32)>> = self.path.iter().filter(|p| p.len() >= 3).cloned().collect();
        self.fill_contours(&cs, c, a);
    }

    pub fn stroke_path(&mut self, lw: f32, c: u32, a: u32) {
        let cs = core::mem::take(&mut self.path);
        self.stroke_contours(&cs, lw, c, a);
        self.path = cs;
    }

    /// Har bir kesma — qalinligi lw bölgan töğriburçak; tutaşmalarda
    /// kiçik köpburçak (qalin çiziqlarda bösliq qolmasin). Hammasi bitta
    /// nonzero böyaşda birlaşadi (yarim şaffof rang ikki marta qoplanmaydi).
    fn stroke_contours(&mut self, cs: &[Vec<(f32, f32)>], lw: f32, c: u32, a: u32) {
        use crate::image::vector::fsqrt;
        let hw = if lw > 0.0 && lw.is_finite() { lw / 2.0 } else { 0.5 };
        let mut polys: Vec<Vec<(f32, f32)>> = Vec::new();
        for p in cs {
            for s in p.windows(2) {
                let ((x0, y0), (x1, y1)) = (s[0], s[1]);
                let (dx, dy) = (x1 - x0, y1 - y0);
                let len = fsqrt(dx * dx + dy * dy);
                if !(len > 1e-4) {
                    continue;
                }
                let (nx, ny) = (-dy / len * hw, dx / len * hw);
                polys.push(alloc::vec![(x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)]);
                if polys.len() > 20000 {
                    break;
                }
            }
            if hw > 1.0 {
                for &(x, y) in p.iter().skip(1).take(p.len().saturating_sub(2)) {
                    polys.push((0..8).map(|k| {
                        let t = k as f32 * core::f32::consts::PI / 4.0;
                        (x + hw * crate::image::vector::cos(t), y + hw * crate::image::vector::sin(t))
                    }).collect());
                }
            }
        }
        // hammasini bir yönalişga keltiramiz — nonzero birlaşmasi töğri bölsin
        for q in polys.iter_mut() {
            let mut area = 0.0;
            for i in 0..q.len() {
                let (a0, b0) = q[i];
                let (a1, b1) = q[(i + 1) % q.len()];
                area += a0 * b1 - a1 * b0;
            }
            if area < 0.0 {
                q.reverse();
            }
        }
        self.fill_contours(&polys, c, a);
    }

    /// Köp konturli köpburçakni böyaydi: nonzero qoidasi, 4×4 superdiskretlaş.
    fn fill_contours(&mut self, cs: &[Vec<(f32, f32)>], c: u32, a: u32) {
        // qirralar: (x0, y0, x1, y1, yönaliş)
        let mut edges: Vec<(f32, f32, f32, f32, i32)> = Vec::new();
        let (mut miny, mut maxy, mut minx, mut maxx) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in cs {
            if p.len() < 3 {
                continue;
            }
            for i in 0..p.len() {
                let (x0, y0) = p[i];
                let (x1, y1) = p[(i + 1) % p.len()];
                if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
                    continue;
                }
                minx = minx.min(x0);
                maxx = maxx.max(x0);
                miny = miny.min(y0);
                maxy = maxy.max(y0);
                if y0 == y1 {
                    continue;
                }
                if y0 < y1 {
                    edges.push((x0, y0, x1, y1, 1));
                } else {
                    edges.push((x1, y1, x0, y0, -1));
                }
            }
        }
        if edges.is_empty() || maxy <= 0.0 || maxx <= 0.0 {
            return;
        }
        let ys = miny.max(0.0) as usize;
        let ye = (maxy.max(0.0) as usize + 1).min(self.h);
        let xs = minx.max(0.0) as usize;
        let xe = (maxx.max(0.0) as usize + 1).min(self.w);
        if xs >= xe {
            return;
        }
        const S: usize = 4;
        let mut cov: Vec<u16> = alloc::vec![0; xe - xs];
        let mut xings: Vec<(f32, i32)> = Vec::new();
        for py in ys..ye {
            cov.iter_mut().for_each(|v| *v = 0);
            for sy in 0..S {
                let fy = py as f32 + (sy as f32 + 0.5) / S as f32;
                xings.clear();
                for &(x0, y0, x1, y1, d) in &edges {
                    if fy >= y0 && fy < y1 {
                        xings.push((x0 + (fy - y0) * (x1 - x0) / (y1 - y0), d));
                    }
                }
                if xings.is_empty() {
                    continue;
                }
                xings.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(core::cmp::Ordering::Equal));
                let mut wind = 0;
                for k in 0..xings.len() - 1 {
                    wind += xings[k].1;
                    if wind == 0 {
                        continue;
                    }
                    // [xa, xb) oraliği içida: S ta gorizontal namuna/piksel
                    let (xa, xb) = (xings[k].0, xings[k + 1].0);
                    let sa = ((xa - xs as f32) * S as f32 - 0.5).max(0.0);
                    let sb = ((xb - xs as f32) * S as f32 - 0.5).min(((xe - xs) * S) as f32);
                    if sb <= sa {
                        continue;
                    }
                    // namuna markazlari: i + 0.5 — ceil(sa) .. ceil(sb)
                    let mut i = sa as usize + if sa > sa as usize as f32 { 1 } else { 0 };
                    let end = sb as usize + if sb > sb as usize as f32 { 1 } else { 0 };
                    while i < end {
                        cov[i / S] += 1;
                        i += 1;
                    }
                }
            }
            for (k, &v) in cov.iter().enumerate() {
                if v > 0 {
                    let al = a * (v as u32).min((S * S) as u32) / (S * S) as u32;
                    self.blend(xs + k, py, c, al);
                }
            }
        }
    }

    /// Matn: (x, y) — asosiy çiziq (baseline) yoki textBaseline böyiça.
    /// align: 0 — chap, 1 — markaz, 2 — öng; base: 0 alphabetic, 1 top, 2 middle, 3 bottom.
    pub fn fill_text(&mut self, s: &str, x: f32, y: f32, px: usize, align: u8, base: u8, c: u32, a: u32) {
        let px = px.clamp(4, 200);
        let (mw, mh, asc, mask) = crate::gfx::text_mask(s, px);
        if mw == 0 || mh == 0 {
            return;
        }
        let tw = crate::gfx::text_width(s, px) as f32;
        let x0 = match align {
            1 => x - tw / 2.0,
            2 => x - tw,
            _ => x,
        };
        let top = match base {
            1 => y,
            2 => y - mh as f32 / 2.0,
            3 => y - mh as f32,
            _ => y - asc as f32,
        };
        let (ox, oy) = (floor(x0 + 0.5), floor(top + 0.5));
        for my in 0..mh {
            let yy = oy + my as i32;
            if yy < 0 || yy as usize >= self.h {
                continue;
            }
            for mx in 0..mw {
                let xx = ox + mx as i32;
                if xx < 0 || xx as usize >= self.w {
                    continue;
                }
                let m = mask[my * mw + mx] as u32;
                if m > 0 {
                    self.blend(xx as usize, yy as usize, c, a * m / 255);
                }
            }
        }
    }
}

fn floor(v: f32) -> i32 {
    let t = v as i32;
    if (t as f32) > v { t - 1 } else { t }
}

fn clamp_size(w: usize, h: usize) -> (usize, usize) {
    let w = w.clamp(1, MAX_SIDE);
    let mut h = h.clamp(1, MAX_SIDE);
    if w * h > MAX_AREA {
        h = (MAX_AREA / w).max(1);
    }
    (w, h)
}

/// CSS rang satri → (RGB, alfa 0..255). rgba(...)/#rrggbbaa alfasi ham.
pub fn parse_style(s: &str) -> Option<(u32, u32)> {
    let t = s.trim().to_ascii_lowercase();
    if t == "transparent" {
        return Some((0, 0));
    }
    let c = crate::css::parse_color(s)?;
    let mut a = 255u32;
    if let Some(r) = t.strip_prefix("rgba(").or_else(|| t.strip_prefix("rgb(")) {
        let parts: Vec<&str> = r.trim_end_matches(')').split([',', ' ', '/']).filter(|x| !x.is_empty()).collect();
        if let Some(v) = parts.get(3) {
            let f = if let Some(p) = v.strip_suffix('%') { p.parse::<f32>().ok().map(|x| x / 100.0) } else { v.parse::<f32>().ok() };
            if let Some(f) = f {
                a = (f.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
            }
        }
    } else if let Some(h) = t.strip_prefix('#') {
        if h.len() == 8 {
            a = u32::from_str_radix(&h[6..8], 16).unwrap_or(255);
        } else if h.len() == 4 {
            a = h[3..4].chars().next().and_then(|c| c.to_digit(16)).unwrap_or(15) * 17;
        }
    }
    Some((c, a))
}

/// font: "bold 16px sans-serif" → 16 (px). pt → px.
pub fn font_px(f: &str) -> usize {
    for w in f.split_whitespace() {
        let w = w.split('/').next().unwrap_or(w);
        if let Some(n) = w.strip_suffix("px").and_then(|n| n.parse::<f32>().ok()) {
            return n as usize;
        }
        if let Some(n) = w.strip_suffix("pt").and_then(|n| n.parse::<f32>().ok()) {
            return (n * 4.0 / 3.0) as usize;
        }
    }
    10
}
