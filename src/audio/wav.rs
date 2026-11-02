//! WAV (RIFF/WAVE) dekoderi — PCM 8/16/24/32-bit, mono/stereo.

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
    let mut data: Option<(usize, usize)> = None;
    while pos + 8 <= d.len() {
        let id = &d[pos..pos + 4];
        let size = u32le(d, pos + 4) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= d.len() {
            fmt = u16le(d, body) as u16;
            channels = u16le(d, body + 2) as u16;
            rate = u32le(d, body + 4);
            bits = u16le(d, body + 14) as u16;
        } else if id == b"data" {
            data = Some((body, (body + size).min(d.len())));
        }
        pos = body + size + (size & 1); // juft tekislaş
    }
    let (ds, de) = data?;
    if channels == 0 || rate == 0 || !matches!(fmt, 1 | 3 | 0xfffe) {
        return None;
    }
    let bytes = (bits / 8) as usize;
    if bytes == 0 {
        return None;
    }
    let raw = &d[ds..de];
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
                    // float32
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
    Some(Clip { rate, channels, samples })
}
