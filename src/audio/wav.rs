//! WAV (RIFF/WAVE) dekoderi — PCM 8/16/24/32-bit, float, μ-law, A-law,
//! IMA ADPCM va Microsoft ADPCM. Mono/stereo. Noldan yozilgan.

use super::Clip;
use alloc::vec::Vec;

fn u16le(d: &[u8], o: usize) -> u32 {
    d[o] as u32 | (d[o + 1] as u32) << 8
}
fn u32le(d: &[u8], o: usize) -> u32 {
    u16le(d, o) | u16le(d, o + 2) << 16
}

pub fn decode(d: &[u8]) -> Option<Clip> {
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    let (mut channels, mut rate, mut bits, mut fmt) = (0u16, 0u32, 0u16, 0u16);
    let mut block_align = 0u16;
    let mut data: Option<(usize, usize)> = None;
    while pos + 8 <= d.len() {
        let id = &d[pos..pos + 4];
        let size = u32le(d, pos + 4) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= d.len() {
            fmt = u16le(d, body) as u16;
            channels = u16le(d, body + 2) as u16;
            rate = u32le(d, body + 4);
            block_align = u16le(d, body + 12) as u16;
            bits = u16le(d, body + 14) as u16;
            // WAVE_FORMAT_EXTENSIBLE: haqiqiy format subformat GUID boşidan
            if fmt == 0xfffe && body + 26 <= d.len() {
                fmt = u16le(d, body + 24) as u16;
            }
        } else if id == b"data" {
            data = Some((body, (body + size).min(d.len())));
        }
        pos = body + size + (size & 1); // juft tekislaş
    }
    let (ds, de) = data?;
    if channels == 0 || rate == 0 {
        return None;
    }
    let raw = &d[ds..de];
    let samples = match fmt {
        1 | 3 => decode_pcm(raw, bits, fmt)?,
        6 => decode_alaw(raw),
        7 => decode_ulaw(raw),
        0x11 => decode_ima_adpcm(raw, channels, block_align as usize)?,
        2 => decode_ms_adpcm(raw, channels, block_align as usize)?,
        _ => return None,
    };
    Some(Clip { rate, channels, samples })
}

/// Oddiy PCM (butun/float) → i16.
fn decode_pcm(raw: &[u8], bits: u16, fmt: u16) -> Option<Vec<i16>> {
    let bytes = (bits / 8) as usize;
    if bytes == 0 {
        return None;
    }
    let n = raw.len() / bytes;
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let o = i * bytes;
        let s = match bits {
            8 => (raw[o] as i16 - 128) << 8, // 8-bit işorasiz → i16
            16 => i16::from_le_bytes([raw[o], raw[o + 1]]),
            24 => {
                let v = (raw[o] as i32) | (raw[o + 1] as i32) << 8 | (raw[o + 2] as i32) << 16;
                let v = if v & 0x800000 != 0 { v | !0xffffff } else { v };
                (v >> 8) as i16
            }
            32 => {
                if fmt == 3 {
                    let f = f32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
                    (f.clamp(-1.0, 1.0) * 32767.0) as i16
                } else {
                    let v = i32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
                    (v >> 16) as i16
                }
            }
            _ => return None,
        };
        samples.push(s);
    }
    Some(samples)
}

/// G.711 μ-law → i16 (har bayt bitta namuna).
fn ulaw_to_i16(u: u8) -> i16 {
    let u = !u;
    let sign = u & 0x80;
    let exp = (u >> 4) & 0x07;
    let mant = u & 0x0f;
    let mut mag = ((mant as i32) << 3) + 0x84;
    mag <<= exp;
    mag -= 0x84;
    if sign != 0 {
        (-mag) as i16
    } else {
        mag as i16
    }
}
fn decode_ulaw(raw: &[u8]) -> Vec<i16> {
    raw.iter().map(|&b| ulaw_to_i16(b)).collect()
}

/// G.711 A-law → i16.
fn alaw_to_i16(a: u8) -> i16 {
    let a = a ^ 0x55;
    let sign = a & 0x80;
    let exp = (a >> 4) & 0x07;
    let mant = a & 0x0f;
    let mut mag = (mant as i32) << 4;
    mag += 8;
    if exp > 1 {
        mag += 0x100;
        mag <<= exp - 1;
    }
    if sign != 0 {
        (-mag) as i16
    } else {
        mag as i16
    }
}
fn decode_alaw(raw: &[u8]) -> Vec<i16> {
    raw.iter().map(|&b| alaw_to_i16(b)).collect()
}

const IMA_STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73,
    80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494,
    544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499,
    2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442, 11487,
    12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IMA_IDX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

fn ima_decode_nibble(nib: u8, pred: &mut i32, idx: &mut i32) -> i16 {
    let step = IMA_STEP[(*idx).clamp(0, 88) as usize];
    let mut diff = step >> 3;
    if nib & 1 != 0 {
        diff += step >> 2;
    }
    if nib & 2 != 0 {
        diff += step >> 1;
    }
    if nib & 4 != 0 {
        diff += step;
    }
    if nib & 8 != 0 {
        *pred -= diff;
    } else {
        *pred += diff;
    }
    *pred = (*pred).clamp(-32768, 32767);
    *idx = (*idx + IMA_IDX[(nib & 0x0f) as usize]).clamp(0, 88);
    *pred as i16
}

/// IMA/DVI ADPCM (fmt 0x11) — blokli, interleaved kanallar.
fn decode_ima_adpcm(raw: &[u8], channels: u16, block_align: usize) -> Option<Vec<i16>> {
    let ch = channels as usize;
    if ch == 0 || block_align < 4 * ch {
        return None;
    }
    let mut out: Vec<i16> = Vec::new();
    for block in raw.chunks(block_align) {
        if block.len() < 4 * ch {
            break;
        }
        // Har kanal uçun boşlang'iç prediktor/indeks (4 bayt sarlavha)
        let mut pred = alloc::vec![0i32; ch];
        let mut idx = alloc::vec![0i32; ch];
        for c in 0..ch {
            let o = c * 4;
            pred[c] = i16::from_le_bytes([block[o], block[o + 1]]) as i32;
            idx[c] = (block[o + 2] as i32).clamp(0, 88);
        }
        // Birinchi namunalar — sarlavhadagi prediktorlar
        let mut frame: Vec<Vec<i16>> = (0..ch).map(|c| alloc::vec![pred[c] as i16]).collect();
        // Qolgan baytlar: har kanal uçun 4 baytli so'z (8 nibble) navbatma-navbat
        let mut p = 4 * ch;
        while p + 4 * ch <= block.len() {
            for c in 0..ch {
                for k in 0..4 {
                    let byte = block[p + c * 4 + k];
                    frame[c].push(ima_decode_nibble(byte & 0x0f, &mut pred[c], &mut idx[c]));
                    frame[c].push(ima_decode_nibble(byte >> 4, &mut pred[c], &mut idx[c]));
                }
            }
            p += 4 * ch;
        }
        // Interleave qilib çiqaramiz
        let len = frame.iter().map(|f| f.len()).min().unwrap_or(0);
        for i in 0..len {
            for c in 0..ch {
                out.push(frame[c][i]);
            }
        }
    }
    Some(out)
}

const MS_ADAPT: [i32; 16] = [230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230];
const MS_COEF1: [i32; 7] = [256, 512, 0, 192, 240, 460, 392];
const MS_COEF2: [i32; 7] = [0, -256, 0, 64, 0, -208, -232];

fn ms_decode_nibble(nib: u8, s1: &mut i32, s2: &mut i32, delta: &mut i32, c1: i32, c2: i32) -> i16 {
    let signed = if nib & 0x08 != 0 { nib as i32 - 16 } else { nib as i32 };
    let mut predict = (*s1 * c1 + *s2 * c2) >> 8;
    predict += signed * *delta;
    predict = predict.clamp(-32768, 32767);
    *s2 = *s1;
    *s1 = predict;
    *delta = (MS_ADAPT[(nib & 0x0f) as usize] * *delta) >> 8;
    if *delta < 16 {
        *delta = 16;
    }
    predict as i16
}

/// Microsoft ADPCM (fmt 2) — blokli.
fn decode_ms_adpcm(raw: &[u8], channels: u16, block_align: usize) -> Option<Vec<i16>> {
    let ch = channels as usize;
    if ch == 0 || ch > 2 || block_align < 7 * ch {
        return None;
    }
    let mut out: Vec<i16> = Vec::new();
    for block in raw.chunks(block_align) {
        let hdr = 7 * ch;
        if block.len() < hdr {
            break;
        }
        let mut pch = alloc::vec![0usize; ch];
        let mut delta = alloc::vec![0i32; ch];
        let mut s1 = alloc::vec![0i32; ch];
        let mut s2 = alloc::vec![0i32; ch];
        for c in 0..ch {
            pch[c] = (block[c] as usize).min(6);
        }
        let mut o = ch;
        for c in 0..ch {
            delta[c] = i16::from_le_bytes([block[o], block[o + 1]]) as i32;
            o += 2;
        }
        for c in 0..ch {
            s1[c] = i16::from_le_bytes([block[o], block[o + 1]]) as i32;
            o += 2;
        }
        for c in 0..ch {
            s2[c] = i16::from_le_bytes([block[o], block[o + 1]]) as i32;
            o += 2;
        }
        // Boşlang'iç namunalar: s2, keyin s1 (interleaved)
        for c in 0..ch {
            out.push(s2[c] as i16);
        }
        for c in 0..ch {
            out.push(s1[c] as i16);
        }
        // Qolgan nibble'lar
        let mut p = hdr;
        let mut c = 0usize;
        while p < block.len() {
            let byte = block[p];
            for &nib in &[byte >> 4, byte & 0x0f] {
                let c1 = MS_COEF1[pch[c]];
                let c2 = MS_COEF2[pch[c]];
                let v = ms_decode_nibble(nib, &mut s1[c], &mut s2[c], &mut delta[c], c1, c2);
                out.push(v);
                if ch == 2 {
                    c ^= 1;
                }
            }
            p += 1;
        }
    }
    Some(out)
}
