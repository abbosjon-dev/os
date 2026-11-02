//! GIF dekoderi — noldan (LZW ochiş). Birinçi kadr körsatiladi.

use super::{rgb, Image};
use alloc::vec::Vec;

fn u16le(d: &[u8], o: usize) -> usize {
    d[o] as usize | (d[o + 1] as usize) << 8
}

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 13 || (&d[..6] != b"GIF87a" && &d[..6] != b"GIF89a") {
        return None;
    }
    let sw = u16le(d, 6);
    let sh = u16le(d, 8);
    if sw == 0 || sh == 0 || sw > 8192 || sh > 8192 {
        return None;
    }
    let packed = d[10];
    let mut pos = 13;
    // Global rang jadvali
    let mut gct: Vec<u32> = Vec::new();
    if packed & 0x80 != 0 {
        let n = 1 << ((packed & 7) + 1);
        for i in 0..n {
            let p = pos + i * 3;
            if p + 3 > d.len() {
                return None;
            }
            gct.push(rgb(d[p], d[p + 1], d[p + 2]));
        }
        pos += n * 3;
    }

    while pos < d.len() {
        match d[pos] {
            0x21 => {
                // kengaytma — sub-bloklarni ötkazamiz
                pos += 2; // 0x21 + yorliq
                pos = skip_subblocks(d, pos)?;
            }
            0x2c => {
                // rasm tavsifi
                if pos + 10 > d.len() {
                    return None;
                }
                let ileft = u16le(d, pos + 1);
                let itop = u16le(d, pos + 3);
                let iw = u16le(d, pos + 5);
                let ih = u16le(d, pos + 7);
                let ipacked = d[pos + 9];
                pos += 10;
                // Mahalliy rang jadvali
                let ct = if ipacked & 0x80 != 0 {
                    let n = 1 << ((ipacked & 7) + 1);
                    let mut t = Vec::new();
                    for i in 0..n {
                        let p = pos + i * 3;
                        if p + 3 > d.len() {
                            return None;
                        }
                        t.push(rgb(d[p], d[p + 1], d[p + 2]));
                    }
                    pos += n * 3;
                    t
                } else {
                    gct.clone()
                };
                let interlaced = ipacked & 0x40 != 0;
                if pos >= d.len() {
                    return None;
                }
                let min_code = d[pos];
                pos += 1;
                let (data, next) = gather_subblocks(d, pos)?;
                pos = next;
                let indices = lzw_decode(&data, min_code, iw * ih)?;
                // Kadrni ekranga joylaymiz
                let mut img = Image::new(sw, sh);
                let rows: Vec<usize> = if interlaced {
                    interlace_rows(ih)
                } else {
                    (0..ih).collect()
                };
                for (ri, &dy) in rows.iter().enumerate() {
                    for dx in 0..iw {
                        let idx = indices.get(ri * iw + dx).copied().unwrap_or(0) as usize;
                        let color = ct.get(idx).copied().unwrap_or(0);
                        let (px, py) = (ileft + dx, itop + dy);
                        if px < sw && py < sh {
                            img.px[py * sw + px] = color;
                        }
                    }
                }
                return Some(img); // birinçi kadr yetarli
            }
            0x3b => break, // tugaş
            _ => return None,
        }
    }
    None
}

fn skip_subblocks(d: &[u8], mut pos: usize) -> Option<usize> {
    while pos < d.len() {
        let n = d[pos] as usize;
        pos += 1;
        if n == 0 {
            return Some(pos);
        }
        pos += n;
    }
    None
}

fn gather_subblocks(d: &[u8], mut pos: usize) -> Option<(Vec<u8>, usize)> {
    let mut out = Vec::new();
    while pos < d.len() {
        let n = d[pos] as usize;
        pos += 1;
        if n == 0 {
            return Some((out, pos));
        }
        if pos + n > d.len() {
            return None;
        }
        out.extend_from_slice(&d[pos..pos + n]);
        pos += n;
    }
    None
}

fn interlace_rows(h: usize) -> Vec<usize> {
    let mut rows = Vec::with_capacity(h);
    for &(start, step) in &[(0usize, 8usize), (4, 8), (2, 4), (1, 2)] {
        let mut y = start;
        while y < h {
            rows.push(y);
            y += step;
        }
    }
    rows
}

fn lzw_decode(data: &[u8], min_code: u8, expected: usize) -> Option<Vec<u8>> {
    let clear = 1usize << min_code;
    let end = clear + 1;
    let mut code_size = min_code as usize + 1;
    let mut dict: Vec<Vec<u8>> = Vec::with_capacity(4096);
    let reset = |dict: &mut Vec<Vec<u8>>| {
        dict.clear();
        for i in 0..clear {
            dict.push(alloc::vec![i as u8]);
        }
        dict.push(Vec::new()); // clear
        dict.push(Vec::new()); // end
    };
    reset(&mut dict);
    let mut out = Vec::with_capacity(expected);
    let mut bitpos = 0usize;
    let mut prev: Option<usize> = None;
    let read_code = |bitpos: &mut usize, size: usize| -> Option<usize> {
        let mut code = 0usize;
        for i in 0..size {
            let byte = *bitpos / 8;
            if byte >= data.len() {
                return None;
            }
            let bit = (data[byte] >> (*bitpos % 8)) & 1;
            code |= (bit as usize) << i;
            *bitpos += 1;
        }
        Some(code)
    };
    while out.len() < expected {
        let Some(code) = read_code(&mut bitpos, code_size) else { break };
        if code == clear {
            reset(&mut dict);
            code_size = min_code as usize + 1;
            prev = None;
            continue;
        }
        if code == end {
            break;
        }
        let entry = if code < dict.len() {
            dict[code].clone()
        } else if let Some(p) = prev {
            // KwKwK holati
            let mut e = dict[p].clone();
            e.push(dict[p][0]);
            e
        } else {
            return None;
        };
        out.extend_from_slice(&entry);
        if let Some(p) = prev {
            let mut new_entry = dict[p].clone();
            new_entry.push(entry[0]);
            dict.push(new_entry);
            if dict.len() == (1 << code_size) && code_size < 12 {
                code_size += 1;
            }
        }
        prev = Some(code);
    }
    Some(out)
}
