//! AIFF/AIFC dekoderi — big-endian PCM (siqilmagan).

use super::Clip;
use alloc::vec::Vec;

fn u16be(d: &[u8], o: usize) -> u32 {
    (d[o] as u32) << 8 | d[o + 1] as u32
}
fn u32be(d: &[u8], o: usize) -> u32 {
    u16be(d, o) << 16 | u16be(d, o + 2)
}

/// 80-bit kengaytirilgan float (sample rate) → u32.
fn extended(d: &[u8], o: usize) -> u32 {
    let exp = (u16be(d, o) & 0x7fff) as i32;
    let mant = (u32be(d, o + 2) as u64) << 32 | u32be(d, o + 6) as u64;
    if exp == 0 {
        return 0;
    }
    let shift = exp - 16383 - 63;
    if shift >= 0 {
        (mant << shift.min(40)) as u32
    } else {
        (mant >> (-shift).min(63)) as u32
    }
}

pub fn decode(d: &[u8]) -> Option<Clip> {
    if d.len() < 12 || &d[..4] != b"FORM" {
        return None;
    }
    let mut pos = 12;
    let (mut channels, mut rate, mut bits) = (0u16, 0u32, 0u16);
    let mut ssnd: Option<(usize, usize)> = None;
    while pos + 8 <= d.len() {
        let id = &d[pos..pos + 4];
        let size = u32be(d, pos + 4) as usize;
        let body = pos + 8;
        if id == b"COMM" && body + 18 <= d.len() {
            channels = u16be(d, body) as u16;
            bits = u16be(d, body + 6) as u16;
            rate = extended(d, body + 8);
        } else if id == b"SSND" && body + 8 <= d.len() {
            let offset = u32be(d, body) as usize;
            ssnd = Some((body + 8 + offset, (body + size).min(d.len())));
        }
        pos = body + size + (size & 1);
    }
    let (ds, de) = ssnd?;
    if channels == 0 || rate == 0 || bits == 0 {
        return None;
    }
    let bytes = (bits / 8) as usize;
    if bytes == 0 || ds >= de {
        return None;
    }
    let raw = &d[ds..de];
    let n = raw.len() / bytes;
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let o = i * bytes;
        let s = match bits {
            8 => (raw[o] as i8 as i16) << 8,
            16 => i16::from_be_bytes([raw[o], raw[o + 1]]),
            24 => {
                let v = (raw[o] as i32) << 16 | (raw[o + 1] as i32) << 8 | raw[o + 2] as i32;
                let v = if v & 0x800000 != 0 { v | !0xffffff } else { v };
                (v >> 8) as i16
            }
            32 => (i32::from_be_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]) >> 16) as i16,
            _ => return None,
        };
        samples.push(s);
    }
    Some(Clip { rate, channels, samples })
}
