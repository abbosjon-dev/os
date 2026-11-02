//! PNG dekoderi — noldan. zlib ochiş (inflate) + filtrlarni yeçiş.
//! Qöllanadi: 8/16 bit çuqurlik, kulrang/RGB/palitra/alfa; palitra/kulrang uçun
//! 1/2/4 bit ham. Interlace (Adam7) qöllanmaydi.

use super::{rgb, Image};
use super::inflate;
use alloc::vec::Vec;

fn be32(d: &[u8], o: usize) -> usize {
    (d[o] as usize) << 24 | (d[o + 1] as usize) << 16 | (d[o + 2] as usize) << 8 | d[o + 3] as usize
}

fn paeth(a: i32, b: i32, c: i32) -> i32 {
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 8 || &d[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut pos = 8;
    let (mut w, mut h) = (0usize, 0usize);
    let (mut depth, mut color) = (0u8, 0u8);
    let mut interlace = 0u8;
    let mut palette: Vec<u32> = Vec::new();
    let mut idat: Vec<u8> = Vec::new();
    while pos + 8 <= d.len() {
        let len = be32(d, pos);
        let typ = &d[pos + 4..pos + 8];
        let body = pos + 8;
        if body + len + 4 > d.len() {
            break;
        }
        let chunk = &d[body..body + len];
        match typ {
            b"IHDR" => {
                if len < 13 {
                    return None;
                }
                w = be32(chunk, 0);
                h = be32(chunk, 4);
                depth = chunk[8];
                color = chunk[9];
                interlace = chunk[12];
            }
            b"PLTE" => {
                for c in chunk.chunks(3) {
                    if c.len() == 3 {
                        palette.push(rgb(c[0], c[1], c[2]));
                    }
                }
            }
            b"IDAT" => idat.extend_from_slice(chunk),
            b"IEND" => break,
            _ => {}
        }
        pos = body + len + 4; // CRC ni ötkazamiz
    }
    if w == 0 || h == 0 || w > 8192 || h > 8192 || interlace != 0 {
        return None;
    }
    let channels = match color {
        0 => 1, // kulrang
        2 => 3, // RGB
        3 => 1, // palitra
        4 => 2, // kulrang + alfa
        6 => 4, // RGBA
        _ => return None,
    };
    let bits_pp = channels * depth as usize;
    let stride = (w * bits_pp).div_ceil(8);
    let limit = (stride + 1) * h + 64;
    let raw = inflate::zlib(&idat, limit)?;
    if raw.len() < (stride + 1) * h {
        return None;
    }
    // Filtrlarni yeçamiz
    let bpp = bits_pp.div_ceil(8).max(1); // filtr uçun bayt qadami
    let mut lines = alloc::vec![0u8; stride * h];
    for y in 0..h {
        let src = y * (stride + 1);
        let filter = raw[src];
        let row = &raw[src + 1..src + 1 + stride];
        let (cur, prev_rows) = lines.split_at_mut(y * stride);
        let prev = if y > 0 { &cur[(y - 1) * stride..y * stride] } else { &[][..] };
        let out = &mut prev_rows[..stride];
        for i in 0..stride {
            let x = row[i] as i32;
            let a = if i >= bpp { out[i - bpp] as i32 } else { 0 };
            let b = if y > 0 { prev[i] as i32 } else { 0 };
            let c = if y > 0 && i >= bpp { prev[i - bpp] as i32 } else { 0 };
            let v = match filter {
                0 => x,
                1 => x + a,
                2 => x + b,
                3 => x + (a + b) / 2,
                4 => x + paeth(a, b, c),
                _ => return None,
            };
            out[i] = (v & 0xff) as u8;
        }
    }
    // Piksellarni ajratamiz
    let mut img = Image::new(w, h);
    for y in 0..h {
        let line = &lines[y * stride..(y + 1) * stride];
        for x in 0..w {
            img.px[y * w + x] = sample(line, x, depth, color, channels, &palette);
        }
    }
    Some(img)
}

fn sample(line: &[u8], x: usize, depth: u8, color: u8, channels: usize, palette: &[u32]) -> u32 {
    // Bitli qiymatni oladi (kanal bo'yicha)
    let get = |ch: usize| -> u32 {
        match depth {
            8 => line.get(x * channels + ch).copied().unwrap_or(0) as u32,
            16 => line.get((x * channels + ch) * 2).copied().unwrap_or(0) as u32, // yuqori bayt
            _ => {
                // 1/2/4 bit (kanal=1 deb hisoblaymiz)
                let bit = x * depth as usize;
                let byte = line.get(bit / 8).copied().unwrap_or(0);
                let shift = 8 - depth as usize - (bit % 8);
                let mask = (1u32 << depth) - 1;
                (byte as u32 >> shift) & mask
            }
        }
    };
    match color {
        0 => {
            // kulrang → RGB (chuqurlikka qarab kengaytiramiz)
            let v = get(0);
            let s = scale8(v, depth);
            rgb(s, s, s)
        }
        2 => rgb(get(0) as u8, get(1) as u8, get(2) as u8),
        3 => {
            let i = get(0) as usize;
            palette.get(i).copied().unwrap_or(0)
        }
        4 => {
            let s = scale8(get(0), depth);
            rgb(s, s, s)
        }
        6 => rgb(get(0) as u8, get(1) as u8, get(2) as u8),
        _ => 0,
    }
}

/// Berilgan çuqurlikdagi qiymatni 0..255 ga keltiradi.
fn scale8(v: u32, depth: u8) -> u8 {
    match depth {
        8 | 16 => v as u8,
        1 => {
            if v != 0 {
                255
            } else {
                0
            }
        }
        2 => (v * 85) as u8,
        4 => (v * 17) as u8,
        _ => v as u8,
    }
}
