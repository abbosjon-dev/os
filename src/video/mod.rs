// Video: konteyner taniş (AVI, MP4/MOV, MKV/WebM) va MJPEG dekodlaş.
// MJPEG kadrlari mavjud JPEG dekoderi orqali tasvirlarga aylantiriladi;
// GUI ularni ketma-ket körsatadi (xuddi GIF/APNG kabi). H.264/HEVC/VP9
// kabi siqilgan kodeklar taniladi, ammo hozircha dekod qilinmaydi.

#![allow(dead_code)]

use crate::image::Image;
use alloc::vec::Vec;

mod avi;
mod mkv;
mod mp4;
mod sample_avi;

pub use sample_avi::AVI as SAMPLE_AVI;

/// Video konteyner turini sarlavha böyiça aniqlaydi.
pub fn format_name(d: &[u8]) -> Option<&'static str> {
    if avi::is_avi(d) {
        Some("AVI")
    } else if mp4::is_mp4(d) {
        if d.len() >= 12 && &d[4..8] == b"ftyp" && &d[8..12] == b"qt  " {
            Some("MOV")
        } else {
            Some("MP4")
        }
    } else if mkv::is_webm(d) {
        Some("WebM")
    } else if mkv::is_mkv(d) {
        Some("MKV")
    } else {
        None
    }
}

/// Birlaşgan video metama'lumoti.
pub struct VideoInfo {
    pub container: &'static str,
    pub codec: &'static str,
    pub w: u32,
    pub h: u32,
    pub frames: u32,
    pub fps_x100: u32, // FPS * 100 (kasr uçun)
    pub duration_ms: u32,
    pub decodable: bool,
}

fn fourcc_name(f: &[u8; 4]) -> &'static str {
    match f {
        b"MJPG" | b"mjpg" | b"mjpa" | b"jpeg" | b"AVRn" | b"MJ2C" => "MJPEG",
        b"DIB " | [0, 0, 0, 0] => "RAW",
        b"avc1" | b"h264" | b"H264" => "H.264",
        b"hev1" | b"hvc1" => "HEVC",
        b"mp4v" | b"MP4V" | b"xvid" | b"XVID" | b"DIVX" => "MPEG-4",
        b"vp08" | b"VP80" => "VP8",
        b"vp09" | b"VP90" => "VP9",
        b"av01" => "AV1",
        _ => "?",
    }
}

pub fn info(d: &[u8]) -> Option<VideoInfo> {
    match format_name(d)? {
        "AVI" => {
            let a = avi::parse(d)?;
            let codec = fourcc_name(&a.fourcc);
            let fps = if a.us_per_frame > 0 { 100_000_000 / a.us_per_frame } else { 0 };
            let dur = if a.us_per_frame > 0 {
                (a.total_frames as u64 * a.us_per_frame as u64 / 1000) as u32
            } else {
                0
            };
            Some(VideoInfo {
                container: "AVI",
                codec,
                w: a.w,
                h: a.h,
                frames: a.total_frames,
                fps_x100: fps,
                duration_ms: dur,
                decodable: codec == "MJPEG",
            })
        }
        c @ ("MP4" | "MOV") => {
            let m = mp4::info(d)?;
            let codec = fourcc_name(&m.codec);
            Some(VideoInfo {
                container: c,
                codec,
                w: m.w,
                h: m.h,
                frames: 0,
                fps_x100: 0,
                duration_ms: m.duration_ms,
                decodable: codec == "MJPEG",
            })
        }
        c @ ("MKV" | "WebM") => Some(VideoInfo {
            container: c,
            codec: "?",
            w: 0,
            h: 0,
            frames: 0,
            fps_x100: 0,
            duration_ms: 0,
            decodable: false,
        }),
        _ => None,
    }
}

/// Video kadrlarini dekodlaydi (hozircha MJPEG/AVI). (tasvir, kechikiş_ms).
pub fn decode_frames(d: &[u8]) -> Option<Vec<(Image, u32)>> {
    if !avi::is_avi(d) {
        return None;
    }
    let a = avi::parse(d)?;
    let codec = fourcc_name(&a.fourcc);
    if codec != "MJPEG" {
        return None;
    }
    let delay = (a.us_per_frame / 1000).max(20);
    let mut out: Vec<(Image, u32)> = Vec::new();
    for fr in avi::frame_slices(d, &a) {
        if fr.len() >= 3 && fr[0] == 0xff && fr[1] == 0xd8 {
            if let Some(img) = crate::image::decode(fr) {
                out.push((img, delay));
            }
        }
        if out.len() >= 1200 {
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Video fayl turimi (kengaytma böyiça)?
pub fn is_video_ext(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(
        ext.as_str(),
        "avi" | "mp4" | "m4v" | "mov" | "qt" | "mkv" | "webm" | "mjpg" | "mjpeg" | "ogv" | "wmv" | "flv" | "3gp"
    )
}

/// Video quyi tizimini tekşiradi (o'rnatilgan MJPEG/AVI bilan).
pub fn self_test() -> Result<(), &'static str> {
    let d = &sample_avi::AVI;
    let vi = info(d).ok_or("AVI info")?;
    if vi.container != "AVI" || vi.codec != "MJPEG" {
        return Err("AVI/MJPEG taniş");
    }
    if vi.w == 0 || vi.h == 0 || vi.frames == 0 {
        return Err("AVI metama'lumot");
    }
    let frames = decode_frames(d).ok_or("MJPEG dekod")?;
    if frames.len() < 2 {
        return Err("MJPEG kadrlar");
    }
    // Kadr öl'çami sarlavhaga mos kelişi kerak
    let (img, _) = &frames[0];
    if img.w != vi.w as usize || img.h != vi.h as usize {
        return Err("kadr öl'çami");
    }
    Ok(())
}
