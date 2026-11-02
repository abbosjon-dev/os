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

/// PNM: P6 (RGB) yoki P5 (kulrang), ASCII sarlavha.
pub fn decode_pnm(d: &[u8]) -> Option<Image> {
    let rgb6 = d[1] == b'6';
    let mut pos = 2;
    let mut vals = [0usize; 3]; // width, height, maxval
    let mut vi = 0;
    while vi < 3 && pos < d.len() {
        // boşliqlarni va izohlarni ötkazamiz
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
    if vi < 3 {
        return None;
    }
    pos += 1; // sarlavhadan keyingi bitta boşliq
    let (w, h) = (vals[0], vals[1]);
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    let mut img = Image::new(w, h);
    let mut p = pos;
    for i in 0..w * h {
        if rgb6 {
            if p + 3 > d.len() {
                break;
            }
            img.px[i] = rgb(d[p], d[p + 1], d[p + 2]);
            p += 3;
        } else {
            if p >= d.len() {
                break;
            }
            img.px[i] = rgb(d[p], d[p], d[p]);
            p += 1;
        }
    }
    Some(img)
}
