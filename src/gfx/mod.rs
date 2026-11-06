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
    noclip: bool, // xotiradagi bufer (canvas matni): ekran qirqişi qöllanmaydi
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
            *FB.lock() = Some(Fb { ptr: fb, w: w as usize, h: h as usize, noclip: false });
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
    if x < f.w && y < f.h && (f.noclip || in_clip(x, y)) {
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
    if a == 0 || x >= f.w || y >= f.h || !(f.noclip || in_clip(x, y)) {
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

/// Berilgan sohada gorizontal gradient (çapdan öngga).
pub fn gradient_rect_h(x: usize, y: usize, w: usize, h: usize, left: Color, right: Color) {
    gradient_round_rect(x, y, w, h, 0, 0, 90, &[(left, 0), (right, 1000)], 0);
}

/// sin / cos (butun gradus) — no_std: çorakka keltirib, Teylor qatori (xato < 1e-4).
fn sin_cos_deg(a: u16) -> (f32, f32) {
    let s = |d: u32| -> f32 {
        // d — 0..=360, natija sin(d°)
        let (q, r) = (d / 90 % 4, d % 90);
        let r = if q % 2 == 1 { 90 - r } else { r };
        let x = r as f32 * 0.017453292;
        let x2 = x * x;
        let v = x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))));
        if q >= 2 { -v } else { v }
    };
    let a = a as u32 % 360;
    (s(a), s((a + 90) % 360))
}

#[inline]
fn absf(v: f32) -> f32 {
    if v < 0.0 { -v } else { v }
}

/// Kvadrat ildiz (no_std): bit taxmini + 3 Nyuton qadami.
fn sqrtf(v: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let mut y = f32::from_bits((v.to_bits() >> 1) + 0x1fbd_1df5);
    for _ in 0..3 {
        y = 0.5 * (y + v / y);
    }
    y
}

/// Rang töxtaşlari böyiça t (0..=1000) nuqtadagi rang.
fn stop_color(stops: &[(Color, u16)], t: u32) -> Color {
    let first = stops[0];
    if t <= first.1 as u32 {
        return first.0;
    }
    for w in stops.windows(2) {
        let ((c0, p0), (c1, p1)) = (w[0], w[1]);
        if t <= p1 as u32 {
            let span = (p1 as u32).saturating_sub(p0 as u32).max(1);
            let k = (t - p0 as u32).min(span) * 255 / span;
            let mix = |a: u32, b: u32| (a * (255 - k) + b * k) / 255;
            return mix(c0 >> 16 & 0xff, c1 >> 16 & 0xff) << 16 | mix(c0 >> 8 & 0xff, c1 >> 8 & 0xff) << 8 | mix(c0 & 0xff, c1 & 0xff);
        }
    }
    stops[stops.len() - 1].0
}

/// Ixtiyoriy burçakli (CSS gradus: 0 — yuqoriga, 90 — öngga, 180 — pastga) chiziqli
/// gradient, yumaloq burçaklar (rt — yuqori, rb — pastki) bilan. `stops` — (rang, joy 0..=1000).
/// `skip` — gradient qutisining yuqoridan körinmaydigan (aylantirilgan) qatorlari:
/// rang chizigʻi (h + skip) balandlikdagi butun quti böyiça hisoblanadi.
pub fn gradient_round_rect(x: usize, y: usize, w: usize, h: usize, rt: usize, rb: usize, angle: u16, stops: &[(Color, u16)], skip: usize) {
    if w == 0 || h == 0 || stops.is_empty() {
        return;
    }
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    let rt = rt.min(w / 2).min(h / 2);
    let rb = rb.min(w / 2).min(h / 2);
    // 256 rangli jadval (har piksel uçun töxtaş qidirmaslik uçun)
    let mut lut = [0u32; 256];
    for (i, c) in lut.iter_mut().enumerate() {
        *c = stop_color(stops, i as u32 * 1000 / 255);
    }
    let (sn, cs) = sin_cos_deg(angle);
    let (dxv, dyv) = (sn, -cs); // yönaliş vektori (ekran: y pastga)
    let full_h = (h + skip) as f32;
    let len = (absf(w as f32 * dxv) + absf(full_h * dyv)).max(1.0);
    let (cx, cy) = (w as f32 / 2.0, full_h / 2.0);
    for dy in 0..h {
        let py = (dy + skip) as f32 + 0.5 - cy;
        for dx in 0..w {
            let cov = match corner_at(dx, dy, w, h, rt, rb) {
                Some((ox, oy, r)) => corner_cov(ox, oy, r),
                None => 255,
            };
            if cov == 0 {
                continue;
            }
            let px = dx as f32 + 0.5 - cx;
            let t = ((px * dxv + py * dyv) / len + 0.5).clamp(0.0, 1.0);
            blend(f, x + dx, y + dy, lut[(t * 255.0 + 0.5) as usize], cov);
        }
    }
}

/// Yumaloq töğriburçakning işorali masofasi (manfiy — içkarida). (cx, cy) — markaz,
/// (hw, hh) — yarim ölçamlar, r — burçak radiusi.
#[inline]
fn rrect_sdf(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = absf(px - cx) - (hw - r);
    let qy = absf(py - cy) - (hh - r);
    let (mx, my) = (qx.max(0.0), qy.max(0.0));
    sqrtf(mx * mx + my * my) + qx.max(qy).min(0.0) - r
}

/// CSS box-shadow: element (ex, ey, ew, eh, burçak radiusi er) soyasi — (dx, dy) ga
/// surilgan, spread ga kengaygan, blur bilan yumşatilgan; rang c, eng katta alfa `alpha`.
/// Elementning öz (töliq qoplangan) sohasiga çizilmaydi.
#[allow(clippy::too_many_arguments)]
pub fn box_shadow(ex: i32, ey: i32, ew: usize, eh: usize, er: usize, dx: i32, dy: i32, blur: usize, spread: i32, c: Color, alpha: u32) {
    if ew == 0 || eh == 0 || alpha == 0 {
        return;
    }
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    let (sw, sh) = (ew as i32 + spread * 2, eh as i32 + spread * 2);
    if sw <= 0 || sh <= 0 {
        return;
    }
    let (sx, sy) = (ex + dx - spread, ey + dy - spread);
    let sr = (er as i32 + spread).clamp(0, sw.min(sh) / 2) as f32;
    let er = (er.min(ew / 2).min(eh / 2)) as f32;
    let bl = blur as i32;
    let (scx, scy) = (sx as f32 + sw as f32 / 2.0, sy as f32 + sh as f32 / 2.0);
    let (ecx, ecy) = (ex as f32 + ew as f32 / 2.0, ey as f32 + eh as f32 / 2.0);
    let x0 = (sx - bl).max(0);
    let y0 = (sy - bl).max(0);
    let x1 = (sx + sw + bl).min(f.w as i32);
    let y1 = (sy + sh + bl).min(f.h as i32);
    let alpha = alpha.min(255) as f32;
    for py in y0..y1 {
        for px in x0..x1 {
            let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
            // element ostida (töliq qoplangan) — chizmaymiz
            if rrect_sdf(fx, fy, ecx, ecy, ew as f32 / 2.0, eh as f32 / 2.0, er) < -1.0 {
                continue;
            }
            let d = rrect_sdf(fx, fy, scx, scy, sw as f32 / 2.0, sh as f32 / 2.0, sr);
            let k = if bl == 0 {
                (0.5 - d).clamp(0.0, 1.0)
            } else {
                let t = ((bl as f32 - d) / (2.0 * bl as f32)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            let a = (k * alpha) as u32;
            if a > 0 {
                blend(f, px as usize, py as usize, c, a);
            }
        }
    }
}

/// Gradient/soya tekşiruvi: burçak trigonometriyasi, töxtaş ranglari, SDF.
pub fn self_test_decor() -> Result<usize, &'static str> {
    let near = |a: f32, b: f32| absf(a - b) < 0.002;
    for (a, s, c) in [(0u16, 0.0, 1.0), (90, 1.0, 0.0), (180, 0.0, -1.0), (270, -1.0, 0.0), (45, 0.70711, 0.70711)] {
        let (ss, cc) = sin_cos_deg(a);
        if !near(ss, s) || !near(cc, c) {
            return Err("gradient: sin/cos notöğri");
        }
    }
    let st = [(0xff0000, 0u16), (0x00ff00, 500), (0x0000ff, 1000)];
    if stop_color(&st, 0) != 0xff0000 || stop_color(&st, 500) != 0x00ff00 || stop_color(&st, 1000) != 0x0000ff || stop_color(&st, 250) & 0xff != 0 {
        return Err("gradient: töxtaş rangi notöğri");
    }
    if !near(sqrtf(2.0), 1.41421) || rrect_sdf(5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 2.0) >= 0.0 || !near(rrect_sdf(13.0, 5.0, 5.0, 5.0, 5.0, 5.0, 0.0), 3.0) {
        return Err("soya: masofa funksiyasi notöğri");
    }
    Ok(4)
}

// --- Yumaloq burçakli shakllar (mac uslubi) ---

/// Burçak qamrovi (0..255). (ox, oy) — aylana markazidan hisoblangan piksel
/// indeksi (0 — markazga eng yaqin piksel), piksel [ox, ox+1]×[oy, oy+1] sohani
/// egallaydi. 4×4 namuna piksel içida teng taqsimlangan (markaz siljimagan).
/// Yoy kesib ötgan har piksel kamida kiçik qamrov oladi (teşik qolmaydi).
#[inline]
fn corner_cov(ox: i32, oy: i32, r: i32) -> u32 {
    if r <= 0 || ox < 0 || oy < 0 {
        return 0;
    }
    let rr = (r as i64 * 8) * (r as i64 * 8);
    let d2 = |a: i64, b: i64| a * a + b * b;
    let (x0, y0) = (ox as i64 * 8, oy as i64 * 8);
    if d2(x0, y0) >= rr {
        return 0; // yaqin burçak ham taşqarida
    }
    if d2(x0 + 8, y0 + 8) <= rr {
        return 255; // uzoq burçak ham içkarida
    }
    let mut inside = 0u32;
    for sy in [1i64, 3, 5, 7] {
        for sx in [1i64, 3, 5, 7] {
            if d2(x0 + sx, y0 + sy) <= rr {
                inside += 1;
            }
        }
    }
    (inside * 255 / 16).max(4)
}

/// Piksel (dx,dy) burçak zonasidami? Bölsa — (ox, oy, r). rt — yuqori, rb — pastki
/// burçaklar radiusi (çaqiruvçi ularni w/2, h/2 bilan çeklagan bölişi şart).
#[inline]
fn corner_at(dx: usize, dy: usize, w: usize, h: usize, rt: usize, rb: usize) -> Option<(i32, i32, i32)> {
    let (r, oy) = if dy < rt {
        (rt, rt - 1 - dy)
    } else if rb > 0 && dy >= h - rb {
        (rb, dy - (h - rb))
    } else {
        return None;
    };
    let ox = if dx < r {
        r - 1 - dx
    } else if dx >= w - r {
        dx - (w - r)
    } else {
        return None;
    };
    Some((ox as i32, oy as i32, r as i32))
}

fn round_rect_on(f: &Fb, x: usize, y: usize, w: usize, h: usize, rt: usize, rb: usize, c: Color, alpha: u32) {
    if w == 0 || h == 0 || alpha == 0 {
        return;
    }
    let rt = rt.min(w / 2).min(h / 2);
    let rb = rb.min(w / 2).min(h / 2);
    for dy in 0..h {
        for dx in 0..w {
            let cov = match corner_at(dx, dy, w, h, rt, rb) {
                Some((ox, oy, r)) => corner_cov(ox, oy, r),
                None => 255,
            };
            blend(f, x + dx, y + dy, c, cov * alpha.min(255) / 255);
        }
    }
}

/// Yumaloq burçakli töldirilgan töğriburçak (silliq chetlar).
pub fn fill_round_rect(x: usize, y: usize, w: usize, h: usize, r: usize, c: Color) {
    fill_round_rect_tb(x, y, w, h, r, r, c);
}

/// Yuqori (rt) va pastki (rb) burçaklari har xil radiusli töldirilgan töğriburçak
/// (masalan, yuqoridan qisman aylantirilgan quti: rt = 0, pastkisi yumaloq qoladi).
pub fn fill_round_rect_tb(x: usize, y: usize, w: usize, h: usize, rt: usize, rb: usize, c: Color) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    round_rect_on(f, x, y, w, h, rt, rb, c, 255);
}

/// Yumaloq burçakli shaffof töğriburçak (alfa 0..255).
pub fn fill_round_rect_a(x: usize, y: usize, w: usize, h: usize, r: usize, c: Color, alpha: u32) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    round_rect_on(f, x, y, w, h, r, r, c, alpha);
}

/// Yumaloq burçakli ramka (ichi boʻsh).
pub fn round_outline(x: usize, y: usize, w: usize, h: usize, r: usize, t: usize, c: Color) {
    round_outline_tb(x, y, w, h, r, r, t, c);
}

/// Ramka: yuqori va pastki burçaklar radiusi alohida.
pub fn round_outline_tb(x: usize, y: usize, w: usize, h: usize, rt: usize, rb: usize, t: usize, c: Color) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    round_ring(f, x, y, w, h, rt, rb, t, c);
}

/// t qalinlikdagi yumaloq ramka (içi böş): taşqi radius rt/rb, içki radius — (r − t),
/// markazi bir xil (CSS kabi). Bitta ötişda çiziladi — qatlamlar orasida çok qolmaydi.
#[allow(clippy::too_many_arguments)]
fn round_ring(f: &Fb, x: usize, y: usize, w: usize, h: usize, rt: usize, rb: usize, t: usize, c: Color) {
    round_box_on(f, x, y, w, h, 0, rt, rb, BoxFill::None, t, c, 255);
}

/// Yumaloq quti foni: yöq, bir rang yoki chiziqli gradient (CSS burçagi, töxtaşlar).
#[derive(Clone, Copy)]
pub enum BoxFill<'a> {
    None,
    Solid(Color),
    Grad(u16, &'a [(Color, u16)]),
}

/// Ikki rangni k (0..=255; 255 — töliq `b`) nisbatda aralaştiradi.
#[inline]
fn mix_rgb(a: Color, b: Color, k: u32) -> Color {
    let ik = 255 - k;
    let m = |s: u32| (((a >> s) & 0xff) * ik + ((b >> s) & 0xff) * k) / 255;
    m(16) << 16 | m(8) << 8 | m(0)
}

/// Veb quti: fon (rang/gradient) + t qalinlikdagi çegara, yumaloq burçaklar bilan —
/// hammasi BITTA ötişda: har piksel uçun taşqi (A) va içki (I) qamrov hisoblanib, rang
/// = çegara va fonning I/A nisbatdagi aralaşmasi, fonga A alfa bilan qöyiladi. Şunda
/// fon çetidagi AA pikseli çegara ostidan körinmaydi va çegara/fon orasida teşik qolmaydi.
/// `h` — qutining TÖLIQ balandligi; `skip` — yuqoridan körinmaydigan (aylantirilgan)
/// qatorlar, ekrandagi `y` — `skip`-qatorning örni. Burçaklar haqiqiy geometriya böyiça
/// (qisman körinadigan yoy ham töğri) çiziladi, pastdan kesilganini qirqiş çeklaydi.
#[allow(clippy::too_many_arguments)]
pub fn round_box(x: usize, y: usize, w: usize, h: usize, skip: usize, r: usize, fill: BoxFill, t: usize, bc: Color) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    round_box_on(f, x, y, w, h, skip, r, r, fill, t, bc, 255);
}

#[allow(clippy::too_many_arguments)]
fn round_box_on(f: &Fb, x: usize, y: usize, w: usize, h: usize, skip: usize, rt: usize, rb: usize, fill: BoxFill, t: usize, bc: Color, alpha: u32) {
    if w == 0 || h == 0 || skip >= h || alpha == 0 {
        return;
    }
    if t == 0 && matches!(fill, BoxFill::None) {
        return;
    }
    let rt = rt.min(w / 2).min(h / 2);
    let rb = rb.min(w / 2).min(h / 2);
    let t = t.min(w.div_ceil(2)).min(h.div_ceil(2));
    // içki (çegaradan içkaridagi) quti
    let (iw, ih) = (w.saturating_sub(2 * t), h.saturating_sub(2 * t));
    let (irt, irb) = (rt.saturating_sub(t), rb.saturating_sub(t));
    // gradient: 256 rangli jadval va yönaliş
    let mut lut = [0u32; 256];
    let (mut dxv, mut dyv, mut len) = (0.0f32, 0.0f32, 1.0f32);
    if let BoxFill::Grad(angle, stops) = fill {
        if stops.is_empty() {
            return;
        }
        for (i, c) in lut.iter_mut().enumerate() {
            *c = stop_color(stops, i as u32 * 1000 / 255);
        }
        let (sn, cs) = sin_cos_deg(angle);
        (dxv, dyv) = (sn, -cs);
        len = (absf(w as f32 * dxv) + absf(h as f32 * dyv)).max(1.0);
    }
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    // ekrandan taşqaridagi qatorlarni aylanmaymiz
    let end = h.min(skip + f.h.saturating_sub(y));
    for dy in skip..end {
        let sy = y + dy - skip;
        let py = dy as f32 + 0.5 - cy;
        for dx in 0..w {
            let a = match corner_at(dx, dy, w, h, rt, rb) {
                Some((ox, oy, r)) => corner_cov(ox, oy, r),
                None => 255,
            };
            if a == 0 {
                continue;
            }
            // içki qamrov (fon ulushi)
            let i = if t == 0 {
                a
            } else if dx < t || dy < t || dx >= t + iw || dy >= t + ih {
                0
            } else {
                match corner_at(dx - t, dy - t, iw, ih, irt, irb) {
                    Some((ox, oy, r)) => corner_cov(ox, oy, r),
                    None => 255,
                }
                .min(a)
            };
            let (col, cov) = match fill {
                BoxFill::None => (bc, a - i),
                BoxFill::Solid(c) => (mix_rgb(bc, c, i * 255 / a), a),
                BoxFill::Grad(..) => {
                    let px = dx as f32 + 0.5 - cx;
                    let k = ((px * dxv + py * dyv) / len + 0.5).clamp(0.0, 1.0);
                    (mix_rgb(bc, lut[(k * 255.0 + 0.5) as usize], i * 255 / a), a)
                }
            };
            if cov > 0 {
                blend(f, x + dx, sy, col, cov * alpha.min(255) / 255);
            }
        }
    }
}

/// Yumaloq burçak tekşiruvi: qamrov xossalari va yorugʻ fonga çizilganda
/// hech bir piksel fon va quti rangidan töqroq emasligi. Sinovlar soni.
pub fn self_test_round() -> Result<usize, &'static str> {
    let mut n = 0;
    for r in 1..=30i32 {
        let rr = (r * r) as i64;
        let d2 = |a: i32, b: i32| (a as i64) * (a as i64) + (b as i64) * (b as i64);
        for oy in 0..r {
            for ox in 0..r {
                let c = corner_cov(ox, oy, r);
                // markaz içkarida (4·(ox+½)²… ≤ 4r²) yoki yaqin burçak içkarida → > 0
                let cx2 = (2 * ox + 1) as i64 * (2 * ox + 1) as i64 + (2 * oy + 1) as i64 * (2 * oy + 1) as i64;
                if (cx2 <= 4 * rr || d2(ox, oy) < rr) && c == 0 {
                    return Err("yoy içidagi piksel qamrovi 0");
                }
                if d2(ox + 1, oy + 1) <= rr && c != 255 {
                    return Err("toʻliq içkaridagi piksel 255 emas");
                }
                if d2(ox, oy) >= rr && c != 0 {
                    return Err("taşqaridagi piksel 0 emas");
                }
                if ox + 1 < r && corner_cov(ox + 1, oy, r) > c {
                    return Err("qamrov x böyiça monoton emas");
                }
                if oy + 1 < r && corner_cov(ox, oy + 1, r) > c {
                    return Err("qamrov y böyiça monoton emas");
                }
                n += 1;
            }
        }
    }
    // Ofscreen bufer: yorugʻ fon ustida turli radius/öçamdagi qutilar
    let (bw, bh) = (64usize, 48usize);
    let mut buf = alloc::vec![0u32; bw * bh];
    let f = Fb { ptr: buf.as_mut_ptr() as usize, w: bw, h: bh, noclip: true };
    let lum = |c: u32| (c >> 16 & 0xff) + (c >> 8 & 0xff) + (c & 0xff);
    for &(bg, fg) in &[(0xeef2f8u32, 0x3a7bd5u32), (0xffffff, 0xeef2f8), (0x161b26, 0xeef2f8)] {
        for &(w, h, rt, rb) in &[(40usize, 30usize, 6usize, 6usize), (21, 13, 7, 7), (50, 40, 30, 30), (9, 9, 4, 4), (30, 20, 0, 8), (17, 33, 5, 0)] {
            for p in buf.iter_mut() {
                *p = bg;
            }
            round_rect_on(&f, 3, 2, w, h, rt, rb, fg, 255);
            round_ring(&f, 3, 2, w, h, rt, rb, 1, fg);
            let min = lum(bg).min(lum(fg));
            for &p in buf.iter() {
                if lum(p) + 3 < min {
                    return Err("burçakda fon va rangdan töqroq piksel");
                }
            }
            // toʻgʻri burçak (r = 0) toʻliq boʻyalgan, yumaloq burçakda fon qoladi
            let tl = buf[2 * bw + 3];
            let bl = buf[(2 + h - 1) * bw + 3];
            if (rt == 0) != (tl == fg) || (rb == 0) != (bl == fg) {
                return Err("yuqori/pastki radius notoʻgʻri qöllandi");
            }
            n += 1;
        }
    }
    n += self_test_round_box()?;
    Ok(n)
}

/// Çegara (round_ring), gradient va bir rangli fon + çegarali yumaloq qutilar: har
/// r = 1..30 va çegara qalinligi 1..4 da, yorugʻ va töq fon ustida ofscreen buferga
/// çiziladi. Tekşiriladi: (1) hech bir piksel fon va quti/çegara ranglarining eng
/// töqidan töqroq emas; (2) çegara rangi fon bilan bir xil bölsa — natija oddiy
/// töldirilgan quti bilan aynan bir xil (qatlamlar orasida teşik/çok yöq); (3) yuqoridan
/// `skip` qator aylantirilgan quti — töliq qutining pastki qismi bilan aynan bir xil.
fn self_test_round_box() -> Result<usize, &'static str> {
    let (bw, bh) = (72usize, 72usize);
    let mut buf = alloc::vec![0u32; bw * bh];
    let mut ref_buf = alloc::vec![0u32; bw * bh];
    let f = Fb { ptr: buf.as_mut_ptr() as usize, w: bw, h: bh, noclip: true };
    let fr = Fb { ptr: ref_buf.as_mut_ptr() as usize, w: bw, h: bh, noclip: true };
    let lum = |c: u32| (c >> 16 & 0xff) + (c >> 8 & 0xff) + (c & 0xff);
    let stops = [(0x4f8cffu32, 0u16), (0x1e3a8a, 500), (0xd4fc79, 1000)];
    let gmin = stops.iter().map(|s| lum(s.0)).min().unwrap_or(0);
    let mut n = 0;
    for &bg in &[0xf3f5f9u32, 0x161b26] {
        for r in 1..=30usize {
            for t in 1..=4usize {
                let (w, h) = (2 * r + 7 + t, 2 * r + 5);
                for (kind, fg, bc) in [(0u8, 0u32, 0x3a7bd5u32), (1, 0xeef2f8, 0x1e3a8a), (1, 0x1e3a8a, 0xeef2f8), (2, 0, 0xffffff), (2, 0, 0x101010)] {
                    let fill = match kind {
                        0 => BoxFill::None,
                        1 => BoxFill::Solid(fg),
                        _ => BoxFill::Grad(135, &stops),
                    };
                    let fmin = match kind {
                        0 => u32::MAX,
                        1 => lum(fg),
                        _ => gmin,
                    };
                    buf.fill(bg);
                    if kind == 0 {
                        round_ring(&f, 3, 2, w, h, r, r, t, bc);
                    } else {
                        round_box_on(&f, 3, 2, w, h, 0, r, r, fill, t, bc, 255);
                    }
                    let min = lum(bg).min(lum(bc)).min(fmin);
                    // +8: butun sonli aralaştiriş (3 bosqiç) yaxlitlaşi
                    if buf.iter().any(|&p| lum(p) + 8 < min) {
                        return Err("çegara/gradient burçagida fon va rangdan töqroq piksel");
                    }
                    // burçak nuqtasi (0,0) yumaloq (r ≥ 4): fon qoladi; çegara qalinligi burçakda ham bor
                    if r >= 4 && buf[2 * bw + 3] != bg {
                        return Err("yumaloq burçak nuqtasi böyalgan");
                    }
                    if buf[(2 + h / 2) * bw + 3] != bc || buf[(2 + h / 2) * bw + 3 + t - 1] != bc {
                        return Err("çegara qalinligi notöğri");
                    }
                    // (3) aylantirilgan: skip qator yuqoridan körinmaydi
                    let skip = (r + t) / 2 + 1;
                    ref_buf.fill(bg);
                    if kind == 0 {
                        round_box_on(&fr, 3, 2, w, h, skip, r, r, BoxFill::None, t, bc, 255);
                    } else {
                        round_box_on(&fr, 3, 2, w, h, skip, r, r, fill, t, bc, 255);
                    }
                    for yy in 2..2 + h - skip {
                        if ref_buf[yy * bw..(yy + 1) * bw] != buf[(yy + skip) * bw..(yy + skip + 1) * bw] {
                            return Err("aylantirilgan yumaloq quti töliq qutidan farq qiladi");
                        }
                    }
                    n += 1;
                }
                // (2) çegara rangi = fon rangi → oddiy töldirilgan quti bilan aynan bir xil
                let c = 0x3a7bd5;
                buf.fill(bg);
                round_box_on(&f, 3, 2, 2 * r + 4, 2 * r + 2, 0, r, r, BoxFill::Solid(c), t, c, 255);
                ref_buf.fill(bg);
                round_rect_on(&fr, 3, 2, 2 * r + 4, 2 * r + 2, r, r, c, 255);
                if buf != ref_buf {
                    return Err("fon+çegara bir xil rangda oddiy qutidan farq qiladi");
                }
                n += 1;
            }
        }
    }
    Ok(n)
}

/// Doira qamrovi: markazi (cx,cy) piksel markazida bölgan doira uçun (dx,dy)
/// pikselning 4×4 namunali qamrovi (0..255).
#[inline]
fn circle_cov(dx: i32, dy: i32, r: i32) -> u32 {
    let rr = (r as i64 * 8) * (r as i64 * 8);
    let mut inside = 0u32;
    for sy in [-3i64, -1, 1, 3] {
        for sx in [-3i64, -1, 1, 3] {
            let (a, b) = (dx as i64 * 8 + sx, dy as i64 * 8 + sy);
            if a * a + b * b <= rr {
                inside += 1;
            }
        }
    }
    inside * 255 / 16
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
            let a = circle_cov(dx, dy, ri);
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

/// Canvas buferini (0xAARRGGBB, alfa bilan) qutiga masshtablab çizadi.
/// `skip` — yuqoridan yaşiringan manba qatorlari (qisman körinişda).
pub fn blit_alpha(bx: usize, by: usize, bw: usize, bh: usize, iw: usize, ih: usize, px: &[u32]) {
    if iw == 0 || ih == 0 || bw == 0 || bh == 0 {
        return;
    }
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    for ty in 0..bh {
        let row = (ty * ih / bh) * iw;
        for tx in 0..bw {
            if let Some(&c) = px.get(row + tx * iw / bw) {
                blend(f, bx + tx, by + ty, c & 0xffffff, c >> 24);
            }
        }
    }
}

/// Matnning qoplama niqobi: (kenglik, balandlik, ascent, 0..255 qiymatlar).
pub fn text_mask(s: &str, px: usize) -> (usize, usize, usize, alloc::vec::Vec<u8>) {
    let fc = face(Font::Ui);
    let w = text_width(s, px) + px / 2 + 2;
    let h = line_height(px, Font::Ui).max(px) + px / 4 + 2;
    if w * h > 4 << 20 {
        return (0, 0, 0, alloc::vec::Vec::new());
    }
    let mut buf = alloc::vec![0u32; w * h];
    let f = Fb { ptr: buf.as_mut_ptr() as usize, w, h, noclip: true };
    let mut cx = 0;
    for ch in s.chars() {
        if ch == '\n' {
            continue;
        }
        let gl = &fc.glyphs[glyph_index(ch)];
        cx += draw_glyph(&f, cx, 0, gl, fc, px, OQ);
    }
    let asc = fc.ascent as usize * px / fc.base as usize;
    (w, h, asc, buf.iter().map(|&c| (c >> 8 & 0xff) as u8).collect())
}

// --- Matn (silliq, masshtablanadigan) ---

fn glyph_index(ch: char) -> usize {
    // Saralangan jadvaldan ikkilik qidiruv; topilmasa — öxşaş belgi yoki "?"
    let find = |c: char| gfx_font::CODES.binary_search(&(c as u32)).ok().map(|k| gfx_font::IDX[k] as usize);
    if let Some(i) = find(ch) {
        return i;
    }
    let alt = match ch {
        '\u{a0}' | '\u{2000}'..='\u{200b}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}' => ' ',
        '\u{2212}' => '-',
        _ if ch.is_control() => ' ',
        _ => '?',
    };
    find(alt).unwrap_or(0)
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
