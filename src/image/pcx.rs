//! PCX dekoderi — RLE, 8-bit palitrali va 24-bit (3 tekislik).

use super::{rgb, Image};
use alloc::vec::Vec;

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 128 || d[0] != 0x0a {
        return None;
    }
    let bpp = d[3] as usize; // bit/piksel/tekislik
    let xmin = u16::from_le_bytes([d[4], d[5]]) as usize;
    let ymin = u16::from_le_bytes([d[6], d[7]]) as usize;
    let xmax = u16::from_le_bytes([d[8], d[9]]) as usize;
    let ymax = u16::from_le_bytes([d[10], d[11]]) as usize;
    let nplanes = d[65] as usize;
    let bpl = u16::from_le_bytes([d[66], d[67]]) as usize;
    if xmax < xmin || ymax < ymin {
        return None;
    }
    let w = xmax - xmin + 1;
    let h = ymax - ymin + 1;
    if w == 0 || h == 0 || w > 8192 || h > 8192 || bpp != 8 {
        return None;
    }
    // RLE ni yeçamiz
    let total = bpl * nplanes * h;
    let mut raw = Vec::with_capacity(total);
    let mut p = 128;
    while raw.len() < total && p < d.len() {
        let b = d[p];
        p += 1;
        if b & 0xc0 == 0xc0 {
            let count = (b & 0x3f) as usize;
            let v = *d.get(p)?;
            p += 1;
            for _ in 0..count {
                raw.push(v);
            }
        } else {
            raw.push(b);
        }
    }
    let mut img = Image::new(w, h);
    if nplanes == 3 {
        // 24-bit: tekisliklar R, G, B
        for y in 0..h {
            let row = y * bpl * 3;
            for x in 0..w {
                let r = raw.get(row + x).copied().unwrap_or(0);
                let g = raw.get(row + bpl + x).copied().unwrap_or(0);
                let b = raw.get(row + 2 * bpl + x).copied().unwrap_or(0);
                img.px[y * w + x] = rgb(r, g, b);
            }
        }
    } else if nplanes == 1 {
        // 8-bit palitra: oxirida 0x0C + 768 bayt
        let pal_off = d.len().saturating_sub(769);
        let has_pal = d.get(pal_off) == Some(&0x0c);
        for y in 0..h {
            let row = y * bpl;
            for x in 0..w {
                let idx = raw.get(row + x).copied().unwrap_or(0) as usize;
                let c = if has_pal {
                    let o = pal_off + 1 + idx * 3;
                    rgb(d.get(o).copied().unwrap_or(0), d.get(o + 1).copied().unwrap_or(0), d.get(o + 2).copied().unwrap_or(0))
                } else {
                    let g = idx as u8;
                    rgb(g, g, g)
                };
                img.px[y * w + x] = c;
            }
        }
    } else {
        return None;
    }
    Some(img)
}
