//! BMP (24/32-bit, siqilmagan) va PNM (P5/P6) dekoderi.

use super::{rgb, Image};

fn u16le(d: &[u8], o: usize) -> u32 {
    d[o] as u32 | (d[o + 1] as u32) << 8
}
fn u32le(d: &[u8], o: usize) -> u32 {
    d[o] as u32 | (d[o + 1] as u32) << 8 | (d[o + 2] as u32) << 16 | (d[o + 3] as u32) << 24
}

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 54 || &d[..2] != b"BM" {
        return None;
    }
    let data_off = u32le(d, 10) as usize;
    let dib = u32le(d, 14) as usize;
    let width = u32le(d, 18) as i32;
    let height_raw = u32le(d, 22) as i32;
    let bpp = u16le(d, 28);
    let compression = u32le(d, 30);
    if dib < 40 || width <= 0 || bpp != 24 && bpp != 32 || compression != 0 {
        return None; // faqat siqilmagan 24/32-bit
    }
    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs() as usize;
    let width = width as usize;
    if width > 8192 || height > 8192 {
        return None;
    }
    let bytes_pp = (bpp / 8) as usize;
    let row_size = (width * bytes_pp).div_ceil(4) * 4; // 4 baytga tekislangan
    if data_off + row_size * height > d.len() {
        return None;
    }
    let mut img = Image::new(width, height);
    for y in 0..height {
        let src_row = if top_down { y } else { height - 1 - y };
        let row = data_off + src_row * row_size;
        for x in 0..width {
            let p = row + x * bytes_pp;
            // BMP — BGR(A)
            img.px[y * width + x] = rgb(d[p + 2], d[p + 1], d[p]);
        }
    }
    Some(img)
}

/// PNM: P1 (ASCII bitmap), P2 (ASCII gray), P3 (ASCII RGB),
/// P4 (binary bitmap), P5 (binary gray), P6 (binary RGB).
pub fn decode_pnm(d: &[u8]) -> Option<Image> {
    let fmt = d[1];
    let bitmap = fmt == b'1' || fmt == b'4';
    let nvals = if bitmap { 2 } else { 3 }; // bitmapda maxval yöq
    let mut pos = 2;
    let mut vals = [0usize; 3];
    let mut vi = 0;
    while vi < nvals && pos < d.len() {
        while pos < d.len() && (d[pos] as char).is_ascii_whitespace() {
            pos += 1;
        }
        if pos < d.len() && d[pos] == b'#' {
            while pos < d.len() && d[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        let mut n = 0usize;
        let mut got = false;
        while pos < d.len() && d[pos].is_ascii_digit() {
            n = n * 10 + (d[pos] - b'0') as usize;
            pos += 1;
            got = true;
        }
        if got {
            vals[vi] = n;
            vi += 1;
        } else {
            break;
        }
    }
    if vi < nvals {
        return None;
    }
    let (w, h) = (vals[0], vals[1]);
    let maxval = if bitmap { 1 } else { vals[2].max(1) };
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    pos += 1; // sarlavhadan keyingi bitta boşliq (binary uçun)
    let mut img = Image::new(w, h);
    let scale = |v: usize| -> u8 { (v * 255 / maxval).min(255) as u8 };
    match fmt {
        b'6' | b'5' => {
            let mut p = pos;
            let rgb6 = fmt == b'6';
            for i in 0..w * h {
                if rgb6 {
                    if p + 3 > d.len() {
                        break;
                    }
                    img.px[i] = rgb(scale(d[p] as usize), scale(d[p + 1] as usize), scale(d[p + 2] as usize));
                    p += 3;
                } else {
                    if p >= d.len() {
                        break;
                    }
                    let g = scale(d[p] as usize);
                    img.px[i] = rgb(g, g, g);
                    p += 1;
                }
            }
        }
        b'4' => {
            // binary bitmap: 1 bit/piksel, qator baytga tekislangan, 1 = qora
            let stride = w.div_ceil(8);
            for y in 0..h {
                for x in 0..w {
                    let byte = d.get(pos + y * stride + x / 8).copied().unwrap_or(0);
                    let bit = byte >> (7 - x % 8) & 1;
                    let g = if bit == 1 { 0 } else { 255 };
                    img.px[y * w + x] = rgb(g, g, g);
                }
            }
        }
        b'1' | b'2' | b'3' => {
            // ASCII: boşliq bilan ajratilgan sonlar
            let mut p = pos;
            let mut next = || -> Option<usize> {
                while p < d.len() && !d[p].is_ascii_digit() {
                    p += 1;
                }
                if p >= d.len() {
                    return None;
                }
                let mut n = 0;
                while p < d.len() && d[p].is_ascii_digit() {
                    n = n * 10 + (d[p] - b'0') as usize;
                    p += 1;
                }
                Some(n)
            };
            for i in 0..w * h {
                let c = match fmt {
                    b'1' => {
                        let v = next().unwrap_or(0);
                        let g = if v == 1 { 0 } else { 255 };
                        rgb(g, g, g)
                    }
                    b'2' => {
                        let g = scale(next().unwrap_or(0));
                        rgb(g, g, g)
                    }
                    _ => rgb(scale(next().unwrap_or(0)), scale(next().unwrap_or(0)), scale(next().unwrap_or(0))),
                };
                img.px[i] = c;
            }
        }
        _ => return None,
    }
    Some(img)
}
