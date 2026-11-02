//! Vektor hujjatlar (SVG/PDF/EPS) uçun matnni Image ga chizadi — gfx shrift
//! atlasidan (silliq qamrov) foydalanib, istalgan ölçamga masştablab.

use super::Image;
use crate::gfx_font::{self, Face};

fn face(mono: bool) -> &'static Face {
    if mono { &gfx_font::MONO } else { &gfx_font::UI }
}

fn glyph_index(ch: char) -> usize {
    gfx_font::CHARS.chars().position(|c| c == ch).unwrap_or(0)
}

fn blend(img: &mut Image, x: i32, y: i32, c: u32, a: u32) {
    if x < 0 || y < 0 || x as usize >= img.w || y as usize >= img.h || a == 0 {
        return;
    }
    let i = y as usize * img.w + x as usize;
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

/// Matnni (y — tayanç/baseline chizigʻi) Image ga chizadi. `px` — EM balandligi.
/// Natija: oxirgi x (keyingi matn uçun).
pub fn draw(img: &mut Image, x: f32, baseline: f32, s: &str, px: f32, color: u32, mono: bool) -> f32 {
    let fc = face(mono);
    let base = fc.base as f32;
    let scale = px / base;
    let top = baseline - fc.ascent as f32 * scale; // qator yuqori chekkasi
    let mut cx = x;
    for ch in s.chars() {
        if ch == '\n' {
            continue;
        }
        let g = &fc.glyphs[glyph_index(ch)];
        let adv = g.adv as f32 * scale;
        if g.w > 0 && g.h > 0 {
            draw_glyph(img, cx, top, g, fc, scale, color);
        }
        cx += adv;
    }
    cx
}

fn draw_glyph(img: &mut Image, x: f32, top: f32, g: &gfx_font::Glyph, fc: &Face, scale: f32, color: u32) {
    let gw = g.w as i32;
    let gh = g.h as i32;
    let cov = &fc.cov[g.off as usize..g.off as usize + (g.w as usize * g.h as usize)];
    let tw = ((g.w as f32 * scale + 0.999) as i32).max(1); // ceil (musbat)
    let th = ((g.h as f32 * scale + 0.999) as i32).max(1);
    let gx = x + g.xoff as f32 * scale;
    let gy = top + g.yoff as f32 * scale;
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
            let mut acc = 0u32;
            for &sub in &[64i32, 192] {
                let fx = ((tx * 256 + sub) * gw) / tw - 128;
                for &subv in &[64i32, 192] {
                    let fy = ((ty * 256 + subv) * gh) / th - 128;
                    acc += sample(fx, fy);
                }
            }
            let a = acc / 4;
            if a > 6 {
                blend(img, gx as i32 + tx, gy as i32 + ty, color, a.min(255));
            }
        }
    }
}

/// Matn kengligi (piksel).
pub fn width(s: &str, px: f32, mono: bool) -> f32 {
    let fc = face(mono);
    let scale = px / fc.base as f32;
    s.chars().map(|ch| fc.glyphs[glyph_index(ch)].adv as f32 * scale).sum()
}
