//! TGA (Targa) dekoderi — RGB/kulrang, siqilmagan va RLE, palitrali.

use super::{rgb, Image};

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 18 {
        return None;
    }
    let id_len = d[0] as usize;
    let cmap_type = d[1];
    let img_type = d[2];
    let cmap_first = u16::from_le_bytes([d[3], d[4]]) as usize;
    let cmap_len = u16::from_le_bytes([d[5], d[6]]) as usize;
    let cmap_bpp = d[7] as usize;
    let w = u16::from_le_bytes([d[12], d[13]]) as usize;
    let h = u16::from_le_bytes([d[14], d[15]]) as usize;
    let bpp = d[16] as usize;
    let desc = d[17];
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    let top = desc & 0x20 != 0; // yuqoridan-pastga
    let rle = matches!(img_type, 9 | 10 | 11);
    let base_type = img_type & 0x07; // 1=palitra, 2=RGB, 3=kulrang
    let mut p = 18 + id_len;
    // Palitra
    let cmap_bytes = cmap_bpp / 8;
    let cmap_start = p;
    if cmap_type == 1 {
        p += cmap_len * cmap_bytes;
    }
    if p > d.len() {
        return None;
    }
    let read_px = |src: &[u8], o: usize| -> Option<u32> {
        match base_type {
            2 => match bpp {
                24 | 32 => Some(rgb(*src.get(o + 2)?, *src.get(o + 1)?, *src.get(o)?)),
                16 => {
                    let v = u16::from_le_bytes([*src.get(o)?, *src.get(o + 1)?]);
                    let r = ((v >> 10 & 31) * 255 / 31) as u8;
                    let g = ((v >> 5 & 31) * 255 / 31) as u8;
                    let b = ((v & 31) * 255 / 31) as u8;
                    Some(rgb(r, g, b))
                }
                _ => None,
            },
            3 => {
                let g = *src.get(o)?;
                Some(rgb(g, g, g))
            }
            1 => {
                let idx = match bpp {
                    8 => *src.get(o)? as usize,
                    16 => u16::from_le_bytes([*src.get(o)?, *src.get(o + 1)?]) as usize,
                    _ => return None,
                };
                let ci = cmap_start + (cmap_first + idx) * cmap_bytes;
                match cmap_bpp {
                    24 | 32 => Some(rgb(*d.get(ci + 2)?, *d.get(ci + 1)?, *d.get(ci)?)),
                    _ => None,
                }
            }
            _ => None,
        }
    };
    let pxbytes = bpp / 8;
    let n = w * h;
    let mut pixels = alloc::vec![0u32; n];
    // Maʼlumotni ketma-ket piksellarga yoyamiz
    let mut flat = alloc::vec![0u32; n];
    if rle {
        let mut i = 0;
        while i < n && p < d.len() {
            let hdr = d[p];
            p += 1;
            let count = (hdr & 0x7f) as usize + 1;
            if hdr & 0x80 != 0 {
                // RLE paketi — bitta piksel takrorlanadi
                let c = read_px(d, p)?;
                p += pxbytes;
                for _ in 0..count {
                    if i < n {
                        flat[i] = c;
                        i += 1;
                    }
                }
            } else {
                for _ in 0..count {
                    if i < n {
                        flat[i] = read_px(d, p)?;
                        p += pxbytes;
                        i += 1;
                    }
                }
            }
        }
    } else {
        for item in flat.iter_mut().take(n) {
            *item = read_px(d, p)?;
            p += pxbytes;
        }
    }
    // Qator tartibini toʻgʻrilaymiz
    for y in 0..h {
        let sy = if top { y } else { h - 1 - y };
        pixels[y * w..y * w + w].copy_from_slice(&flat[sy * w..sy * w + w]);
    }
    Some(Image { w, h, px: pixels })
}
