//! QOI (Quite OK Image) dekoderi — sodda, siqilgan, yöqotişsiz.

use super::{rgb, Image};

const OP_INDEX: u8 = 0x00; // 00xxxxxx
const OP_DIFF: u8 = 0x40; // 01xxxxxx
const OP_LUMA: u8 = 0x80; // 10xxxxxx
const OP_RUN: u8 = 0xc0; // 11xxxxxx
const OP_RGB: u8 = 0xfe;
const OP_RGBA: u8 = 0xff;

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 14 || &d[..4] != b"qoif" {
        return None;
    }
    let w = u32::from_be_bytes([d[4], d[5], d[6], d[7]]) as usize;
    let h = u32::from_be_bytes([d[8], d[9], d[10], d[11]]) as usize;
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    let mut img = Image::new(w, h);
    let (mut r, mut g, mut b, mut a) = (0u8, 0u8, 0u8, 255u8);
    let mut index = [[0u8; 4]; 64];
    let mut p = 14;
    let mut i = 0;
    let n = w * h;
    while i < n && p < d.len() {
        let op = d[p];
        p += 1;
        if op == OP_RGB {
            if p + 3 > d.len() {
                break;
            }
            r = d[p];
            g = d[p + 1];
            b = d[p + 2];
            p += 3;
        } else if op == OP_RGBA {
            if p + 4 > d.len() {
                break;
            }
            r = d[p];
            g = d[p + 1];
            b = d[p + 2];
            a = d[p + 3];
            p += 4;
        } else {
            match op & 0xc0 {
                OP_INDEX => {
                    let px = index[(op & 0x3f) as usize];
                    r = px[0];
                    g = px[1];
                    b = px[2];
                    a = px[3];
                }
                OP_DIFF => {
                    r = r.wrapping_add((op >> 4 & 3).wrapping_sub(2));
                    g = g.wrapping_add((op >> 2 & 3).wrapping_sub(2));
                    b = b.wrapping_add((op & 3).wrapping_sub(2));
                }
                OP_LUMA => {
                    if p >= d.len() {
                        break;
                    }
                    let b2 = d[p];
                    p += 1;
                    let vg = (op & 0x3f).wrapping_sub(32);
                    r = r.wrapping_add(vg.wrapping_add((b2 >> 4).wrapping_sub(8)));
                    g = g.wrapping_add(vg);
                    b = b.wrapping_add(vg.wrapping_add((b2 & 0x0f).wrapping_sub(8)));
                }
                OP_RUN => {
                    let run = (op & 0x3f) as usize + 1;
                    for _ in 0..run {
                        if i < n {
                            img.px[i] = rgb(r, g, b);
                            i += 1;
                        }
                    }
                    continue;
                }
                _ => {}
            }
        }
        let idx = (r as usize * 3 + g as usize * 5 + b as usize * 7 + a as usize * 11) % 64;
        index[idx] = [r, g, b, a];
        if i < n {
            img.px[i] = rgb(r, g, b);
            i += 1;
        }
    }
    Some(img)
}
