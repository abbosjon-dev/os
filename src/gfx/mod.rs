#![allow(dead_code)] // grafika API — keyingi bosqiçlarda işlatiladi
//! Grafika: framebufferga çiziş (piksel, töğriburçak, matn).
//! Piksel — u32: 0x00RRGGBB.

use crate::drivers::virtio_gpu as gpu;
use crate::gfx_font::{CHARS, FONT, GLYPH_H, GLYPH_W};
use crate::sync::SpinLock;

pub type Color = u32;

pub const QORA: Color = 0x000000;
pub const OQ: Color = 0xffffff;
pub const ZAMIN_KOK: Color = 0x1e66b4; // Zamin köki
pub const ZAMIN_TUN: Color = 0x0d1b2a; // tün fon
pub const KULRANG: Color = 0x8a9bb0;
pub const YASHIL: Color = 0x2ecc71;
pub const SARIQ: Color = 0xf1c40f;
pub const QIZIL: Color = 0xe74c3c;

struct Fb {
    ptr: usize,
    w: usize,
    h: usize,
}

static FB: SpinLock<Option<Fb>> = SpinLock::new(None);

/// Grafikani işga tuşiradi (GPU + framebuffer).
pub fn init(width: u32, height: u32) -> bool {
    match gpu::init(width, height) {
        Some((w, h, fb)) => {
            *FB.lock() = Some(Fb { ptr: fb, w: w as usize, h: h as usize });
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
    if x < f.w && y < f.h {
        unsafe { core::ptr::write_volatile((f.ptr + (y * f.w + x) * 4) as *mut u32, c) };
    }
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

/// Töğriburçak chegarasi (ramka).
pub fn rect_outline(x: usize, y: usize, w: usize, h: usize, c: Color) {
    fill_rect(x, y, w, 1, c);
    fill_rect(x, y + h - 1, w, 1, c);
    fill_rect(x, y, 1, h, c);
    fill_rect(x + w - 1, y, 1, h, c);
}

/// Vertikal gradient (yuqoridan pastga `top`→`bottom`).
pub fn gradient(top: Color, bottom: Color) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    for y in 0..f.h {
        let t = y * 255 / f.h.max(1);
        let mix = |a: u32, b: u32| ((a * (255 - t as u32) + b * t as u32) / 255) & 0xff;
        let r = mix(top >> 16 & 0xff, bottom >> 16 & 0xff);
        let gr = mix(top >> 8 & 0xff, bottom >> 8 & 0xff);
        let bl = mix(top & 0xff, bottom & 0xff);
        let c = (r << 16) | (gr << 8) | bl;
        for x in 0..f.w {
            put(f, x, y, c);
        }
    }
}

fn glyph(ch: char) -> &'static [u8; 16] {
    let idx = CHARS.chars().position(|c| c == ch).unwrap_or(0);
    &FONT[idx]
}

/// Bitta belgini `scale` kattalikda çizadi.
pub fn draw_char(x: usize, y: usize, ch: char, c: Color, scale: usize) {
    let g = FB.lock();
    let Some(f) = g.as_ref() else { return };
    let bits = glyph(ch);
    for (row, &b) in bits.iter().enumerate() {
        for col in 0..GLYPH_W {
            if b & (1 << (7 - col)) != 0 {
                for sy in 0..scale {
                    for sx in 0..scale {
                        put(f, x + col * scale + sx, y + row * scale + sy, c);
                    }
                }
            }
        }
    }
}

/// Matn çizadi. Natija: oxirgi x koordinatasi.
pub fn draw_text(x: usize, y: usize, text: &str, c: Color, scale: usize) -> usize {
    let mut cx = x;
    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }
        draw_char(cx, y, ch, c, scale);
        cx += GLYPH_W * scale;
    }
    cx
}

/// Matnni markazlashtirib çizadi.
pub fn draw_text_centered(cx: usize, y: usize, text: &str, c: Color, scale: usize) {
    let width = text.chars().count() * GLYPH_W * scale;
    draw_text(cx.saturating_sub(width / 2), y, text, c, scale);
}

/// Matn piksel kengligi.
pub fn text_width(text: &str, scale: usize) -> usize {
    text.chars().count() * GLYPH_W * scale
}

pub fn glyph_size(scale: usize) -> (usize, usize) {
    (GLYPH_W * scale, GLYPH_H * scale)
}

/// Çizilganni ekranga çiqaradi.
pub fn flush() {
    gpu::flush();
}
