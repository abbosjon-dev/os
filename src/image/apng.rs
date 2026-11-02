//! APNG (Animated PNG) dekoderi — fcTL/fdAT/acTL. PNG kadr dekoderidan
//! foydalanadi; kadrlarni kanvasga joylaydi (ofset, dispose). RGB çiqaradi.

use super::{png, Image};
use alloc::vec::Vec;

fn be32(d: &[u8], o: usize) -> usize {
    (d[o] as usize) << 24 | (d[o + 1] as usize) << 16 | (d[o + 2] as usize) << 8 | d[o + 3] as usize
}

struct FrameCtl {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    delay_ms: u32,
    dispose: u8,
    data: Vec<u8>,
}

/// APNG mi? (acTL chunk bor).
pub fn is_apng(d: &[u8]) -> bool {
    find_chunk(d, b"acTL").is_some()
}

fn find_chunk(d: &[u8], typ: &[u8]) -> Option<usize> {
    let mut pos = 8;
    while pos + 8 <= d.len() {
        let len = be32(d, pos);
        if &d[pos + 4..pos + 8] == typ {
            return Some(pos);
        }
        if &d[pos + 4..pos + 8] == b"IEND" {
            return None;
        }
        pos = pos + 8 + len + 4;
    }
    None
}

pub fn decode_all(d: &[u8]) -> Option<Vec<(Image, u32)>> {
    if d.len() < 8 || &d[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let (mut cw, mut ch) = (0usize, 0usize);
    let (mut depth, mut color, mut interlace) = (0u8, 0u8, 0u8);
    let mut palette: Vec<u32> = Vec::new();
    let mut frames: Vec<FrameCtl> = Vec::new();
    let mut cur: Option<FrameCtl> = None;
    let mut seen_fctl = false;

    let mut pos = 8;
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
                cw = be32(chunk, 0);
                ch = be32(chunk, 4);
                depth = chunk[8];
                color = chunk[9];
                interlace = chunk[12];
            }
            b"PLTE" => {
                for c in chunk.chunks(3) {
                    if c.len() == 3 {
                        palette.push(super::rgb(c[0], c[1], c[2]));
                    }
                }
            }
            b"fcTL" => {
                if let Some(f) = cur.take() {
                    frames.push(f);
                }
                seen_fctl = true;
                let fw = be32(chunk, 4);
                let fh = be32(chunk, 8);
                let fx = be32(chunk, 12);
                let fy = be32(chunk, 16);
                let dn = (chunk[20] as u32) << 8 | chunk[21] as u32;
                let dd = ((chunk[22] as u32) << 8 | chunk[23] as u32).max(1);
                let dispose = chunk[24];
                cur = Some(FrameCtl { x: fx, y: fy, w: fw, h: fh, delay_ms: dn * 1000 / dd, dispose, data: Vec::new() });
            }
            b"IDAT" => {
                if seen_fctl {
                    if let Some(f) = cur.as_mut() {
                        f.data.extend_from_slice(chunk);
                    }
                }
            }
            b"fdAT" => {
                if len >= 4 {
                    if let Some(f) = cur.as_mut() {
                        f.data.extend_from_slice(&chunk[4..]); // 4 bayt ketma-ketlik raqamini ötkazamiz
                    }
                }
            }
            b"IEND" => break,
            _ => {}
        }
        pos = body + len + 4;
    }
    if let Some(f) = cur.take() {
        frames.push(f);
    }
    if frames.len() < 2 || cw == 0 || ch == 0 {
        return None;
    }
    // Kadrlarni kanvasga joylaymiz
    let mut canvas = alloc::vec![0u32; cw * ch];
    let mut out: Vec<(Image, u32)> = Vec::with_capacity(frames.len());
    for f in &frames {
        if f.w == 0 || f.h == 0 || f.x + f.w > cw || f.y + f.h > ch {
            continue;
        }
        let saved = if f.dispose == 2 { Some(canvas.clone()) } else { None };
        let Some(sub) = png::decode_frame(f.w, f.h, depth, color, interlace, &palette, &f.data) else { continue };
        for yy in 0..f.h {
            for xx in 0..f.w {
                canvas[(f.y + yy) * cw + (f.x + xx)] = sub.px[yy * f.w + xx];
            }
        }
        out.push((Image { w: cw, h: ch, px: canvas.clone() }, f.delay_ms.max(20)));
        // dispose
        match f.dispose {
            1 => {
                for yy in 0..f.h {
                    for xx in 0..f.w {
                        canvas[(f.y + yy) * cw + (f.x + xx)] = 0;
                    }
                }
            }
            2 => {
                if let Some(s) = saved {
                    canvas = s;
                }
            }
            _ => {}
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
