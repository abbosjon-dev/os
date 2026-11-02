//! Audio: fayllarni dekodlaş (WAV, AIFF) va övoz çiqariş drayveri.
//! Dekoder PCM namunalarni (i16) chiqaradi; GUI toʻlqin shaklini körsatadi.

#![allow(dead_code)]

use alloc::vec::Vec;

mod aiff;
mod flac;
mod flac_test;
mod sample_wav;
pub mod sound;
mod wav;

pub use sample_wav::SAMPLE_WAV;

/// Dekodlangan övoz: namuna chastotasi, kanallar, i16 namunalar (interleaved).
pub struct Clip {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>, // kanallar araloş (L R L R ...)
}

impl Clip {
    /// Davomiyligi (ms).
    pub fn duration_ms(&self) -> u32 {
        if self.rate == 0 || self.channels == 0 {
            return 0;
        }
        let per_ch = self.samples.len() as u64 / self.channels as u64;
        (per_ch * 1000 / self.rate as u64) as u32
    }
    /// Mono (barcha kanallar oʻrtachasi) — toʻlqin uçun.
    pub fn mono(&self) -> Vec<i16> {
        if self.channels <= 1 {
            return self.samples.clone();
        }
        let ch = self.channels as usize;
        self.samples
            .chunks(ch)
            .map(|c| (c.iter().map(|&s| s as i32).sum::<i32>() / ch as i32) as i16)
            .collect()
    }
    /// Toʻlqin shakli uçun `n` ta (min, max) juftlik.
    pub fn envelope(&self, n: usize) -> Vec<(i16, i16)> {
        let mono = self.mono();
        if mono.is_empty() || n == 0 {
            return Vec::new();
        }
        let step = mono.len().max(1).div_ceil(n);
        let mut out = Vec::with_capacity(n);
        let mut i = 0;
        while i < mono.len() {
            let end = (i + step).min(mono.len());
            let (mut mn, mut mx) = (i16::MAX, i16::MIN);
            for &s in &mono[i..end] {
                mn = mn.min(s);
                mx = mx.max(s);
            }
            out.push((mn, mx));
            i = end;
        }
        out
    }
}

/// Audio formatini sarlavha baytlaridan aniqlaydi.
pub fn format_name(d: &[u8]) -> Option<&'static str> {
    if d.len() < 12 {
        return None;
    }
    if &d[..4] == b"RIFF" && &d[8..12] == b"WAVE" {
        Some("WAV")
    } else if &d[..4] == b"FORM" && (&d[8..12] == b"AIFF" || &d[8..12] == b"AIFC") {
        Some("AIFF")
    } else if &d[..4] == b"fLaC" {
        Some("FLAC")
    } else {
        None
    }
}

/// Audio faylni dekodlaydi.
pub fn decode(_name: &str, d: &[u8]) -> Option<Clip> {
    match format_name(d)? {
        "WAV" => wav::decode(d),
        "AIFF" => aiff::decode(d),
        "FLAC" => flac::decode(d),
        _ => None,
    }
}

/// WAV dekoderini tekşiradi (qoʻlda qurilgan WAV bilan).
pub fn self_test() -> Result<(), &'static str> {
    // 8000 Hz, mono, 16-bit, 4 namuna: 0, 16000, -16000, 100
    let samples: [i16; 4] = [0, 16000, -16000, 100];
    let mut w = Vec::new();
    let data_size = samples.len() * 2;
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&((36 + data_size) as u32).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&8000u32.to_le_bytes());
    w.extend_from_slice(&16000u32.to_le_bytes()); // byte rate
    w.extend_from_slice(&2u16.to_le_bytes()); // block align
    w.extend_from_slice(&16u16.to_le_bytes()); // bits
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data_size as u32).to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    let clip = decode("a.wav", &w).ok_or("WAV")?;
    if clip.rate != 8000 || clip.channels != 1 || clip.samples != samples {
        return Err("WAV");
    }
    // FLAC — yöqotişsiz: dekodlangan namunalar aniq mos kelişi kerak
    let fc = decode("a.flac", flac_test::FLAC).ok_or("FLAC")?;
    if fc.rate != 8000 || fc.channels != 1 || fc.samples.len() != flac_test::PCM.len() {
        return Err("FLAC");
    }
    if fc.samples != flac_test::PCM {
        return Err("FLAC");
    }
    Ok(())
}

/// Övoz fayli turimi (kengaytma böyiça)?
pub fn is_audio_ext(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "wav" | "wave" | "aif" | "aiff" | "aifc" | "mp3" | "flac" | "ogg" | "m4a" | "aac" | "opus")
}
