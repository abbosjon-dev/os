//! ICO/CUR dekoderi — eng katta tasvirni tanlaydi (PNG yoki BMP-DIB).

use super::{png, rgb, Image};

fn u16le(d: &[u8], o: usize) -> usize {
    d[o] as usize | (d[o + 1] as usize) << 8
}
fn u32le(d: &[u8], o: usize) -> usize {
    d[o] as usize | (d[o + 1] as usize) << 8 | (d[o + 2] as usize) << 16 | (d[o + 3] as usize) << 24
}

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 6 || d[0] != 0 || d[1] != 0 || (d[2] != 1 && d[2] != 2) {
        return None;
    }
    let count = u16le(d, 4);
    if count == 0 {
        return None;
    }
    // Eng katta yozuvni tanlaymiz
    let mut best: Option<(usize, usize)> = None; // (maydon, offset içidagi indeks)
    for i in 0..count {
        let e = 6 + i * 16;
        if e + 16 > d.len() {
            break;
        }
        let w = if d[e] == 0 { 256 } else { d[e] as usize };
        let h = if d[e + 1] == 0 { 256 } else { d[e + 1] as usize };
        let area = w * h;
        if best.map(|(a, _)| area > a).unwrap_or(true) {
            best = Some((area, e));
        }
    }
    let (_, e) = best?;
    let size = u32le(d, e + 8);
    let off = u32le(d, e + 12);
    if off + size > d.len() {
        return None;
    }
    let blob = &d[off..off + size];
    // PNG mi?
    if blob.len() >= 8 && &blob[..8] == b"\x89PNG\r\n\x1a\n" {
        return png::decode(blob);
    }
    // BMP-DIB (BITMAPINFOHEADER)
    decode_dib(blob)
}

fn decode_dib(d: &[u8]) -> Option<Image> {
    if d.len() < 40 {
        return None;
    }
    let w = u32le(d, 4);
    let h_full = u32le(d, 8);
    let h = h_full / 2; // AND niqobni hisobga olamiz
    let bpp = u16le(d, 14);
    if w == 0 || h == 0 || w > 1024 || h > 1024 {
        return None;
    }
    let pal_entries = u32le(d, 32);
    let pal_n = if pal_entries != 0 { pal_entries } else if bpp <= 8 { 1 << bpp } else { 0 };
    let pal_off = 40;
    let data_off = pal_off + pal_n * 4;
    let bytes_pp = bpp / 8;
    let row_size = ((w * bpp + 31) / 32) * 4; // 4 baytga tekislangan
    let mut img = Image::new(w, h);
    for y in 0..h {
        let sy = h - 1 - y; // pastdan yuqoriga
        let row = data_off + sy * row_size;
        for x in 0..w {
            let c = match bpp {
                32 | 24 => {
                    let p = row + x * bytes_pp;
                    rgb(*d.get(p + 2)?, *d.get(p + 1)?, *d.get(p)?)
                }
                8 => {
                    let idx = *d.get(row + x)? as usize;
                    let po = pal_off + idx * 4;
                    rgb(*d.get(po + 2)?, *d.get(po + 1)?, *d.get(po)?)
                }
                4 => {
                    let b = *d.get(row + x / 2)?;
                    let idx = (if x % 2 == 0 { b >> 4 } else { b & 15 }) as usize;
                    let po = pal_off + idx * 4;
                    rgb(*d.get(po + 2)?, *d.get(po + 1)?, *d.get(po)?)
                }
                _ => return None,
            };
            img.px[y * w + x] = c;
        }
    }
    Some(img)
}
