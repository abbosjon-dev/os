#![allow(dead_code)] // grafika API
//! Grafika: framebufferga çiziş. Piksel — u32: 0x00RRGGBB.
//! Matn silliq (anti-aliased) va masshtablanadigan; oynalar mac uslubida
//! (yumaloq burçaklar, soya, shaffoflik). Hamma narsa alfa aralaştiriş bilan.

use crate::drivers::virtio_gpu as gpu;
use crate::gfx_font::{self, Face, Glyph};
use crate::sync::SpinLock;

pub type Color = u32;

// Asosiy ranglar
pub const QORA: Color = 0x000000;
pub const OQ: Color = 0xffffff;
pub const ZAMIN_KOK: Color = 0x2f7fe0;
pub const ZAMIN_TUN: Color = 0x0d1b2a;
pub const KULRANG: Color = 0x9fb0c3;
pub const YASHIL: Color = 0x30d158;
pub const SARIQ: Color = 0xffd426;
pub const QIZIL: Color = 0xff5f57;

// Matn oʻlchamlari (piksel, EM balandligi)
pub const T_KICHIK: usize = 15;
pub const T_ODDIY: usize = 18;
pub const T_KATTA: usize = 24;
pub const T_SARLAVHA: usize = 30;
pub const T_HERO: usize = 54;

#[derive(Clone, Copy, PartialEq)]
pub enum Font {
    Ui,
    Mono,
}

fn face(f: Font) -> &'static Face {
    match f {
        Font::Ui => &gfx_font::UI,
        Font::Mono => &gfx_font::MONO,
    }
}

struct Fb {
    ptr: usize,
    w: usize,
    h: usize,
}

static FB: SpinLock<Option<Fb>> = SpinLock::new(None);

// Qirqiş (clip) töğriburçagi — chiziş şu soha bilan çeklanadi (oyna içidagi matn
// çetga çiqib ketmasligi uçun). Boşlangʼiç holatda — butun ekran.
use core::sync::atomic::{AtomicUsize, Ordering};
static CLIP_X0: AtomicUsize = AtomicUsize::new(0);
static CLIP_Y0: AtomicUsize = AtomicUsize::new(0);
static CLIP_X1: AtomicUsize = AtomicUsize::new(usize::MAX);
static CLIP_Y1: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Chiziş sohasini çeklaydi (oyna mazmuni uçun).
pub fn set_clip(x: usize, y: usize, w: usize, h: usize) {
    CLIP_X0.store(x, Ordering::Relaxed);
    CLIP_Y0.store(y, Ordering::Relaxed);
    CLIP_X1.store(x + w, Ordering::Relaxed);
    CLIP_Y1.store(y + h, Ordering::Relaxed);
}

/// Qirqişni olib taşlaydi (butun ekran).
pub fn reset_clip() {
    CLIP_X0.store(0, Ordering::Relaxed);
    CLIP_Y0.store(0, Ordering::Relaxed);
    CLIP_X1.store(usize::MAX, Ordering::Relaxed);
    CLIP_Y1.store(usize::MAX, Ordering::Relaxed);
}

#[inline]
fn in_clip(x: usize, y: usize) -> bool {
    x >= CLIP_X0.load(Ordering::Relaxed)
        && x < CLIP_X1.load(Ordering::Relaxed)
        && y >= CLIP_Y0.load(Ordering::Relaxed)
        && y < CLIP_Y1.load(Ordering::Relaxed)
}

pub fn init(width: u32, height: u32) -> bool {
    match gpu::init(width, height) {
        Some((w, h, fb)) => {
            *FB.lock() = Some(Fb { ptr: fb, w: w as usize, h: h as usize });
            reset_clip();
            true
        }
        None => false,
    }
}

pub fn present() -> bool {
    FB.lock().is_some()
}

pub fn size() -> (usize, usize) {
    FB.lock().as_ref().map_or((0, 0), |f| (f.w, f.h))
}

#[inline]
fn put(f: &Fb, x: usize, y: usize, c: Color) {
    if x < f.w && y < f.h && in_clip(x, y) {
        unsafe { core::ptr::write_volatile((f.ptr + (y * f.w + x) * 4) as *mut u32, c) };
    }
}

#[inline]
fn get(f: &Fb, x: usize, y: usize) -> Color {
    if x < f.w && y < f.h {
        unsafe { core::ptr::read_volatile((f.ptr + (y * f.w + x) * 4) as *const u32) }
    } else {
        0
    }
}

/// Alfa (0..255) bilan aralaştirib qöyadi.
#[inline]
fn blend(f: &Fb, x: usize, y: usize, c: Color, a: u32) {
    if a == 0 || x >= f.w || y >= f.h || !in_clip(x, y) {
        return;
    }
    if a >= 255 {
        put(f, x, y, c);
        return;
    }
    let bg = get(f, x, y);
    let ia = 255 - a;
    let r = (((c >> 16) & 0xff) * a + ((bg >> 16) & 0xff) * ia) / 255;
    let g = (((c >> 8) & 0xff) * a + ((bg >> 8) & 0xff) * ia) / 255;
    let b = ((c & 0xff) * a + (bg & 0xff) * ia) / 255;
    put(f, x, y, (r << 16) | (g << 8) | b);
}

/// Ekrandagi barça piksellar (0xRRGGBB) — ekran rasmi uçun.
pub fn read_pixels() -> alloc::vec::Vec<u32> {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return alloc::vec::Vec::new() };
    let mut v = alloc::vec::Vec::with_capacity(f.w * f.h);
    for y in 0..f.h {
        for x in 0..f.w {
            v.push(get(f, x, y) & 0xff_ffff);
        }
    }
    v
}

pub fn clear(c: Color) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    for i in 0..f.w * f.h {
        unsafe { core::ptr::write_volatile((f.ptr + i * 4) as *mut u32, c) };
    }
}

pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, c: Color) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    for dy in 0..h {
        for dx in 0..w {
            put(f, x + dx, y + dy, c);
        }
    }
}

/// Shaffof töğriburçak (alfa 0..255).
pub fn blend_rect(x: usize, y: usize, w: usize, h: usize, c: Color, a: u32) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    for dy in 0..h {
        for dx in 0..w {
            blend(f, x + dx, y + dy, c, a);
        }
    }
}

pub fn rect_outline(x: usize, y: usize, w: usize, h: usize, c: Color) {
    fill_rect(x, y, w, 1, c);
    fill_rect(x, y + h - 1, w, 1, c);
    fill_rect(x, y, 1, h, c);
    fill_rect(x + w - 1, y, 1, h, c);
}

/// Vertikal gradient.
pub fn gradient(top: Color, bottom: Color) {
    gradient_rect(0, 0, usize::MAX, usize::MAX, top, bottom);
}

/// Berilgan sohada vertikal gradient.
pub fn gradient_rect(x: usize, y: usize, w: usize, h: usize, top: Color, bottom: Color) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    let x1 = (x + w).min(f.w);
    let y1 = (y + h).min(f.h);
    let span = (y1.saturating_sub(y)).max(1);
    for yy in y..y1 {
        let t = (yy - y) * 255 / span;
        let mix = |a: u32, b: u32| ((a * (255 - t as u32) + b * t as u32) / 255) & 0xff;
        let r = mix(top >> 16 & 0xff, bottom >> 16 & 0xff);
        let gr = mix(top >> 8 & 0xff, bottom >> 8 & 0xff);
        let bl = mix(top & 0xff, bottom & 0xff);
        let c = (r << 16) | (gr << 8) | bl;
        for xx in x..x1 {
            put(f, xx, yy, c);
        }
    }
}

// --- Yumaloq burçakli shakllar (mac uslubi) ---

/// Burçak qamrovi: aylana markazidan (dx,dy) masofadagi piksel içidami (0..255).
#[inline]
fn corner_cov(dx: i32, dy: i32, r: i32) -> u32 {
    // 4x supersample
    let mut inside = 0;
    let r2 = r * r;
    for &sy in &[-1i32, 1] {
        for &sx in &[-1i32, 1] {
            let px = dx * 4 + sx; // chorak piksel
            let py = dy * 4 + sy;
            if px * px + py * py <= r2 * 16 {
                inside += 1;
            }
        }
    }
    inside * 255 / 4
}

/// Yumaloq burçakli töldirilgan töğriburçak (silliq chetlar).
pub fn fill_round_rect(x: usize, y: usize, w: usize, h: usize, r: usize, c: Color) {
    if w == 0 || h == 0 {
        return;
    }
    let r = r.min(w / 2).min(h / 2);
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    for dy in 0..h {
        for dx in 0..w {
            // Qaysi burçak zonasidamiz?
            let cx = if dx < r {
                Some((r - 1 - dx) as i32)
            } else if dx >= w - r {
                Some((dx - (w - r)) as i32)
            } else {
                None
            };
            let cy = if dy < r {
                Some((r - 1 - dy) as i32)
            } else if dy >= h - r {
                Some((dy - (h - r)) as i32)
            } else {
                None
            };
            match (cx, cy) {
                (Some(ox), Some(oy)) => {
                    let a = corner_cov(ox, oy, r as i32);
                    blend(f, x + dx, y + dy, c, a);
                }
                _ => put(f, x + dx, y + dy, c),
            }
        }
    }
}

/// Yumaloq burçakli shaffof töğriburçak (alfa 0..255).
pub fn fill_round_rect_a(x: usize, y: usize, w: usize, h: usize, r: usize, c: Color, alpha: u32) {
    if w == 0 || h == 0 || alpha == 0 {
        return;
    }
    let r = r.min(w / 2).min(h / 2);
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    for dy in 0..h {
        for dx in 0..w {
            let cx = if dx < r {
                Some((r - 1 - dx) as i32)
            } else if dx >= w - r {
                Some((dx - (w - r)) as i32)
            } else {
                None
            };
            let cy = if dy < r {
                Some((r - 1 - dy) as i32)
            } else if dy >= h - r {
                Some((dy - (h - r)) as i32)
            } else {
                None
            };
            let cov = match (cx, cy) {
                (Some(ox), Some(oy)) => corner_cov(ox, oy, r as i32),
                _ => 255,
            };
            let a = cov * alpha / 255;
            blend(f, x + dx, y + dy, c, a);
        }
    }
}

/// Yumaloq burçakli ramka (ichi boʻsh).
pub fn round_outline(x: usize, y: usize, w: usize, h: usize, r: usize, t: usize, c: Color) {
    // Oddiy: toʻliq yumaloq töldirib, keyin içini kesib olmaymiz — chetga t piksel chizamiz.
    for i in 0..t {
        round_ring(x + i, y + i, w - 2 * i, h - 2 * i, r.saturating_sub(i), c);
    }
}

fn round_ring(x: usize, y: usize, w: usize, h: usize, r: usize, c: Color) {
    if w < 2 || h < 2 {
        return;
    }
    let r = r.min(w / 2).min(h / 2);
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    let edge = |dx: usize, dy: usize| dx == 0 || dy == 0 || dx == w - 1 || dy == h - 1;
    for dy in 0..h {
        for dx in 0..w {
            let in_corner_x = dx < r || dx >= w - r;
            let in_corner_y = dy < r || dy >= h - r;
            if in_corner_x && in_corner_y {
                let ox = if dx < r { (r - 1 - dx) as i32 } else { (dx - (w - r)) as i32 };
                let oy = if dy < r { (r - 1 - dy) as i32 } else { (dy - (h - r)) as i32 };
                let outer = corner_cov(ox, oy, r as i32);
                let inner = corner_cov(ox, oy, r as i32 - 1);
                let a = outer.saturating_sub(inner);
                if a > 0 {
                    blend(f, x + dx, y + dy, c, a);
                }
            } else if edge(dx, dy) {
                put(f, x + dx, y + dy, c);
            }
        }
    }
}

/// Silliq töldirilgan doira (ikona uçun).
pub fn fill_circle(cx: usize, cy: usize, r: usize, c: Color) {
    if r == 0 {
        return;
    }
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    let ri = r as i32;
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            let a = corner_cov(dx.abs(), dy.abs(), ri);
            if a > 0 {
                let xx = cx as i32 + dx;
                let yy = cy as i32 + dy;
                if xx >= 0 && yy >= 0 {
                    blend(f, xx as usize, yy as usize, c, a);
                }
            }
        }
    }
}

fn isqrt(n: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Yumşoq tuşuvçi soya. Muhim: soya oynaning öz sohasiga (töğriburçak içiga)
/// çizilmaydi — şunda oyna chetining silliq (AA) pikseli qora soya ustiga emas,
/// fon ustiga tuşadi va burçaklarda qora halqa paydo bölmaydi.
/// Soya biroz pastga suriladi (tabiiy tuşiş).
pub fn shadow(x: usize, y: usize, w: usize, h: usize, _r: usize, blur: usize) {
    if blur == 0 || w == 0 || h == 0 {
        return;
    }
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    let off = (blur / 3).max(2) as i32; // pastga suriş
    let (bx, by) = (x as i32, y as i32 + off);
    let (bw, bh) = (w as i32, h as i32);
    let bl = blur as i32;
    let x0 = (bx - bl).max(0);
    let y0 = (by - bl).max(0);
    let x1 = bx + bw + bl;
    let y1 = by + bh + bl;
    for py in y0..y1 {
        for px in x0..x1 {
            // Soya töğriburçagidan tashqaridagi masofa
            let dx = if px < bx { bx - px } else if px >= bx + bw { px - (bx + bw) + 1 } else { 0 };
            let dy = if py < by { by - py } else if py >= by + bh { py - (by + bh) + 1 } else { 0 };
            if dx == 0 && dy == 0 {
                continue; // oyna içi — soyasiz
            }
            // Oynaning asl (surilmagan) sohasini ham chetlab ötamiz
            if (x as i32..x as i32 + bw).contains(&px) && (y as i32..y as i32 + bh).contains(&py) {
                continue;
            }
            let d = isqrt((dx * dx + dy * dy) as u32) as i32;
            if d >= bl {
                continue;
            }
            // Alfa chetga borgan sari pasayadi (kvadratik yumşoqlik)
            let t = (bl - d) as u32;
            let a = 42 * t * t / (bl as u32 * bl as u32);
            blend(f, px as usize, py as usize, 0x000000, a.min(90));
        }
    }
}

/// Yögon chiziq (ikona uçun), AA'siz.
pub fn hline(x: usize, y: usize, len: usize, c: Color) {
    fill_rect(x, y, len, 1, c);
}

/// Silliq (anti-aliased) töldirilgan köpburçak. Nuqtalar — f32 ekran koordinatalari.
/// Her piksel 3x3 supersample bilan baholanadi.
pub fn fill_poly(pts: &[(f32, f32)], c: Color) {
    if pts.len() < 3 {
        return;
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in pts {
        minx = minx.min(x);
        miny = miny.min(y);
        maxx = maxx.max(x);
        maxy = maxy.max(y);
    }
    // no_std: musbat f32 uçun `as usize` — floor; ceil uçun +1
    let x0 = minx.max(0.0) as usize;
    let y0 = miny.max(0.0) as usize;
    let x1 = maxx.max(0.0) as usize + 1;
    let y1 = maxy.max(0.0) as usize + 1;
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    let inside = |px: f32, py: f32| -> bool {
        // juft-toq (even-odd) qoida
        let mut c = false;
        let n = pts.len();
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = pts[i];
            let (xj, yj) = pts[j];
            if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                c = !c;
            }
            j = i;
        }
        c
    };
    for py in y0..=y1 {
        for px in x0..=x1 {
            let mut hit = 0u32;
            for sy in 0..3 {
                for sx in 0..3 {
                    let fx = px as f32 + (sx as f32 + 0.5) / 3.0;
                    let fy = py as f32 + (sy as f32 + 0.5) / 3.0;
                    if inside(fx, fy) {
                        hit += 1;
                    }
                }
            }
            if hit > 0 {
                blend(f, px, py, c, hit * 255 / 9);
            }
        }
    }
}

/// Rasmni (iw×ih piksel) berilgan qutiga (bw×bh) nisbatni saqlab sigdiradi
/// (markazlashgan, nearest). Natija: haqiqiy çizilgan (x, y, w, h).
pub fn blit_fit(bx: usize, by: usize, bw: usize, bh: usize, iw: usize, ih: usize, px: &[u32]) -> (usize, usize, usize, usize) {
    if iw == 0 || ih == 0 || bw == 0 || bh == 0 {
        return (bx, by, 0, 0);
    }
    // Nisbatni saqlab sigdiramiz
    let (tw, th) = if iw * bh > ih * bw {
        (bw, (ih * bw / iw).max(1))
    } else {
        ((iw * bh / ih).max(1), bh)
    };
    let ox = bx + (bw - tw) / 2;
    let oy = by + (bh - th) / 2;
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return (ox, oy, tw, th) };
    for ty in 0..th {
        let sy = ty * ih / th;
        for tx in 0..tw {
            let sx = tx * iw / tw;
            if let Some(&c) = px.get(sy * iw + sx) {
                put(f, ox + tx, oy + ty, c);
            }
        }
    }
    (ox, oy, tw, th)
}

/// Rasmni berilgan töğriburçakni to'liq to'ldirib chizadi (eng-yaqin namuna).
/// Fon (wallpaper) uçun — nisbat saqlashsiz, butun maydonni qoplaydi.
pub fn blit_scaled(bx: usize, by: usize, bw: usize, bh: usize, iw: usize, ih: usize, px: &[u32]) {
    if iw == 0 || ih == 0 || bw == 0 || bh == 0 {
        return;
    }
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    for ty in 0..bh {
        let sy = ty * ih / bh;
        let row = sy * iw;
        for tx in 0..bw {
            let sx = tx * iw / bw;
            if let Some(&c) = px.get(row + sx) {
                put(f, bx + tx, by + ty, c);
            }
        }
    }
}

// --- Matn (silliq, masshtablanadigan) ---

fn glyph_index(ch: char) -> usize {
    gfx_font::CHARS.chars().position(|c| c == ch).unwrap_or(0)
}

/// Bir belgini chizadi. `top` — qator yuqori chekkasi, `px` — EM balandligi.
/// Natija: keyingi belgi uçun x siljişi.
fn draw_glyph(f: &Fb, x: usize, top: usize, g: &Glyph, face: &Face, px: usize, c: Color) -> usize {
    let base = face.base as usize;
    let adv = (g.adv as usize * px) / base;
    if g.w == 0 || g.h == 0 {
        return adv;
    }
    let tw = (g.w as usize * px).div_ceil(base);
    let th = (g.h as usize * px).div_ceil(base);
    let gx = x as i32 + (g.xoff as i32 * px as i32) / base as i32;
    let gy = top as i32 + (g.yoff as i32 * px as i32) / base as i32;
    let cov = &face.cov[g.off as usize..g.off as usize + (g.w as usize * g.h as usize)];
    let gw = g.w as i32;
    let gh = g.h as i32;
    // Har bir maqsad pikseli — manba qamrovidan 2x2 supersample bilinear namuna
    let sample = |fx_q8: i32, fy_q8: i32| -> u32 {
        let ix = fx_q8 >> 8;
        let iy = fy_q8 >> 8;
        let fxx = (fx_q8 & 255) as u32;
        let fyy = (fy_q8 & 255) as u32;
        let at = |xi: i32, yi: i32| -> u32 {
            if xi < 0 || yi < 0 || xi >= gw || yi >= gh {
                0
            } else {
                cov[(yi * gw + xi) as usize] as u32
            }
        };
        let c00 = at(ix, iy);
        let c10 = at(ix + 1, iy);
        let c01 = at(ix, iy + 1);
        let c11 = at(ix + 1, iy + 1);
        let topm = c00 * (256 - fxx) + c10 * fxx;
        let botm = c01 * (256 - fxx) + c11 * fxx;
        (topm * (256 - fyy) + botm * fyy) >> 16
    };
    for ty in 0..th {
        for tx in 0..tw {
            // 2x2 supersample
            let mut acc = 0u32;
            for &sub in &[64i32, 192] {
                let fx = ((tx as i32 * 256 + sub) * gw) / tw as i32 - 128;
                for &subv in &[64i32, 192] {
                    let fy = ((ty as i32 * 256 + subv) * gh) / th as i32 - 128;
                    acc += sample(fx, fy);
                }
            }
            let a = acc / 4;
            if a > 4 {
                let xx = gx + tx as i32;
                let yy = gy + ty as i32;
                if xx >= 0 && yy >= 0 {
                    blend(f, xx as usize, yy as usize, c, a.min(255));
                }
            }
        }
    }
    adv
}

/// Matn chizadi. `y` — qator yuqori chekkasi. Natija: oxirgi x.
pub fn text(x: usize, y: usize, s: &str, c: Color, px: usize, font: Font) -> usize {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return x };
    let fc = face(font);
    let mut cx = x;
    for ch in s.chars() {
        if ch == '\n' {
            continue;
        }
        let gl = &fc.glyphs[glyph_index(ch)];
        cx += draw_glyph(f, cx, y, gl, fc, px, c);
    }
    cx
}

pub fn draw_text(x: usize, y: usize, s: &str, c: Color, px: usize) -> usize {
    text(x, y, s, c, px, Font::Ui)
}

pub fn draw_text_mono(x: usize, y: usize, s: &str, c: Color, px: usize) -> usize {
    text(x, y, s, c, px, Font::Mono)
}

pub fn text_width_font(s: &str, px: usize, font: Font) -> usize {
    let fc = face(font);
    let base = fc.base as usize;
    s.chars().map(|ch| fc.glyphs[glyph_index(ch)].adv as usize * px / base).sum()
}

pub fn text_width(s: &str, px: usize) -> usize {
    text_width_font(s, px, Font::Ui)
}

pub fn text_width_mono(s: &str, px: usize) -> usize {
    text_width_font(s, px, Font::Mono)
}

pub fn draw_text_centered(cx: usize, y: usize, s: &str, c: Color, px: usize) {
    let w = text_width(s, px);
    text(cx.saturating_sub(w / 2), y, s, c, px, Font::Ui);
}

/// Monospace belgi kengligi (ustun hisobi uçun).
pub fn mono_advance(px: usize) -> usize {
    let fc = face(Font::Mono);
    fc.glyphs[glyph_index('M')].adv as usize * px / fc.base as usize
}

/// Qator balandligi.
pub fn line_height(px: usize, font: Font) -> usize {
    let fc = face(font);
    fc.line as usize * px / fc.base as usize
}

pub fn flush() {
    gpu::flush();
}
