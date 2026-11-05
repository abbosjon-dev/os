//! Rasm dekoderlari — noldan yozilgan (ochiq kutubxonasiz).
//! Qöllanadigan formatlar: PNG, JPEG (baseline), GIF, BMP, QOI, PPM/PGM.
//! Natija — RGB piksellar (`0x00RRGGBB`).

#![allow(dead_code)]

use alloc::vec::Vec;

mod apng;
mod apng_test;
mod bmp;
mod cdr;
mod dxf;
mod eps;
mod gif;
mod ico;
mod inflate;
mod jpeg;
mod pcx;
mod pdf;
mod png;
pub mod png_enc;
mod qoi;
mod sample;
mod sample_gif;
mod sample_svg;
mod svg;
mod testdata;
mod text;
mod tga;
pub mod vector;
mod wmf;

pub use sample::SAMPLE_PNG;
pub use sample_gif::SAMPLE_GIF;
pub use sample_svg::SAMPLE_SVG;

/// Dekodlangan rasm.
#[derive(Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>, // RGB, qator-qator (yuqoridan pastga)
}

impl Image {
    fn new(w: usize, h: usize) -> Image {
        Image { w, h, px: alloc::vec![0u32; w * h] }
    }
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> u32 {
        if x < self.w && y < self.h {
            self.px[y * self.w + x]
        } else {
            0
        }
    }
}

/// Rasm formatini sarlavha baytlaridan aniqlaydi (ishonçli "magic" bölganlar).
pub fn format_name(data: &[u8]) -> Option<&'static str> {
    if data.len() < 8 {
        return None;
    }
    if &data[..8] == b"\x89PNG\r\n\x1a\n" {
        Some("PNG") // APNG ham şu yerda (birinçi kadr)
    } else if data[0] == 0xff && data[1] == 0xd8 {
        Some("JPEG")
    } else if &data[..6] == b"GIF87a" || &data[..6] == b"GIF89a" {
        Some("GIF")
    } else if &data[..2] == b"BM" {
        Some("BMP")
    } else if &data[..4] == b"qoif" {
        Some("QOI")
    } else if &data[..5.min(data.len())] == b"%PDF-" {
        Some("PDF")
    } else if data.len() >= 4 && &data[..4] == b"%!PS" {
        Some("EPS")
    } else if data.len() >= 11 && &data[..4] == b"\xc5\xd0\xd3\xc6" {
        Some("EPS") // EPSF binar sarlavha (DOS EPS)
    } else if is_svg(data) {
        Some("SVG")
    } else if data[0] == b'P' && (b'1'..=b'6').contains(&data[1]) {
        Some("PNM")
    } else if &data[..4] == b"\xd7\xcd\xc6\x9a" {
        Some("WMF") // placeable WMF
    } else if data.len() >= 44 && &data[..4] == b"\x01\x00\x00\x00" && &data[40..44] == b" EMF" {
        Some("EMF")
    } else if data[0] == 0x0a && data[2] <= 1 {
        Some("PCX")
    } else if data[0] == 0 && data[1] == 0 && (data[2] == 1 || data[2] == 2) && data[3] == 0 && {
        // ICO/CUR: tasvirlar soni 1..=255 (TGA type-2 dan ajratish uçun — unda bu 0)
        let count = data[4] as usize | (data[5] as usize) << 8;
        (1..=255).contains(&count)
    } {
        Some("ICO")
    } else {
        None
    }
}

/// Baytlardan rasmni dekodlaydi (format "magic" orqali aniqlanadi).
pub fn decode(data: &[u8]) -> Option<Image> {
    match format_name(data)? {
        "PNG" => png::decode(data),
        "JPEG" => jpeg::decode(data),
        "GIF" => gif::decode(data),
        "BMP" => bmp::decode(data),
        "QOI" => qoi::decode(data),
        "PNM" => bmp::decode_pnm(data),
        "PCX" => pcx::decode(data),
        "ICO" => ico::decode(data),
        "SVG" => svg::decode(data),
        "EPS" => eps::decode(data),
        "PDF" => pdf::decode(data),
        "WMF" | "EMF" => wmf::decode(data),
        _ => None,
    }
}

/// SVG mi? (XML sarlavhasi yoki <svg yorlig'i boshida).
fn is_svg(data: &[u8]) -> bool {
    let head = &data[..data.len().min(512)];
    let s = core::str::from_utf8(head).unwrap_or("");
    let t = s.trim_start();
    t.starts_with("<?xml") && s.contains("<svg") || t.starts_with("<svg")
}

/// Animatsiya kadrlari: (rasm, kechikiş_ms). Birdan koʻp kadr — harakatli rasm.
/// Hozirça GIF (va APNG). Boşqalar uçun — bitta kadr yoki None.
pub fn decode_frames(name: &str, data: &[u8]) -> Option<Vec<(Image, u32)>> {
    let is_gif = format_name(data) == Some("GIF") || name.rsplit_once('.').map(|(_, e)| e.eq_ignore_ascii_case("gif")).unwrap_or(false);
    if is_gif {
        return gif::decode_all(data);
    }
    // APNG (harakatli PNG)?
    if format_name(data) == Some("PNG") && apng::is_apng(data) {
        return apng::decode_all(data);
    }
    None
}

/// Fayl nomini (kengaytma) hisobga olib dekodlaydi — TGA kabi "magic"siz
/// formatlar uçun. Avval magic, keyin kengaytma böyiça urinadi.
pub fn decode_file(name: &str, data: &[u8]) -> Option<Image> {
    if let Some(img) = decode(data) {
        return Some(img);
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "tga" | "icb" | "vda" | "vst" => tga::decode(data),
        "pcx" => pcx::decode(data),
        "ico" | "cur" => ico::decode(data),
        "svg" => svg::decode(data),
        "dxf" => dxf::decode(data),
        "eps" | "ps" | "ai" => eps::decode(data),
        "wmf" | "emf" => wmf::decode(data),
        "cdr" => cdr::decode(data),
        _ => None,
    }
}

#[inline]
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Dekoderlarni sinov rasmlari bilan tekşiradi (16x16, 4 kvadrant).
/// Natija: tekşirilgan formatlar soni yoki xato format nomi.
pub fn self_test() -> Result<usize, &'static str> {
    // Kutilgan kvadrant ranglari
    let want = |x: usize, y: usize| -> u32 {
        match (x < 8, y < 8) {
            (true, true) => rgb(220, 30, 30),
            (false, true) => rgb(30, 200, 60),
            (true, false) => rgb(40, 80, 230),
            (false, false) => rgb(240, 200, 20),
        }
    };
    let check = |img: &Image| -> bool {
        if img.w != 16 || img.h != 16 {
            return false;
        }
        // Har kvadrant markazidan bitta piksel
        for &(x, y) in &[(4usize, 4usize), (12, 4), (4, 12), (12, 12)] {
            if img.get(x, y) != want(x, y) {
                return false;
            }
        }
        true
    };
    let cases: [(&str, &[u8]); 5] = [
        ("PNG", testdata::PNG),
        ("GIF", testdata::GIF),
        ("BMP", testdata::BMP),
        ("PNM", testdata::PPM),
        ("QOI", testdata::QOI),
    ];
    let mut n = 0;
    for (name, bytes) in cases {
        match decode(bytes) {
            Some(img) if check(&img) => n += 1,
            _ => return Err(name),
        }
    }
    // PNG kodlovçisi: kodlab, öz dekoderimiz bilan qaytarib öqiymiz (aniq mos)
    {
        let mut px = alloc::vec::Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                px.push(want(x, y));
            }
        }
        let enc = png_enc::encode(16, 16, &px);
        match decode(&enc) {
            Some(img) if check(&img) && img.px == px => n += 1,
            _ => return Err("PNG kodlovçi"),
        }
    }
    // TGA, PCX, ICO — yöqotişsiz, aniq rang (kengaytmali nom bilan)
    for (name, bytes) in [("a.tga", testdata::TGA), ("a.pcx", testdata::PCX), ("a.ico", testdata::ICO)] {
        match decode_file(name, bytes) {
            Some(img) if check(&img) => n += 1,
            _ => return Err(name),
        }
    }
    // JPEG — yöqotişli: ölçam va taxminiy rang (chidamlilik bilan)
    let near = |img: &Image| -> bool {
        if img.w != 16 || img.h != 16 {
            return false;
        }
        for &(x, y) in &[(4usize, 4usize), (12, 4), (4, 12), (12, 12)] {
            let (g, w) = (img.get(x, y), want(x, y));
            let diff = |sh: u32| ((g >> sh & 0xff) as i32 - (w >> sh & 0xff) as i32).abs();
            if diff(16) > 60 || diff(8) > 60 || diff(0) > 60 {
                return false;
            }
        }
        true
    };
    match decode(testdata::JPEG) {
        Some(img) if near(&img) => n += 1,
        _ => return Err("JPEG"),
    }
    // SVG — vektor; nisbatan joylaşgan rang nuqtalarini tekşiramiz
    let svg = br##"<svg viewBox="0 0 16 16"><rect x="0" y="0" width="8" height="8" fill="#dc1e1e"/><rect x="8" y="0" width="8" height="8" fill="#1ec83c"/><rect x="0" y="8" width="8" height="8" fill="#2850e6"/><rect x="8" y="8" width="8" height="8" fill="#f0c814"/></svg>"##;
    let rp = |img: &Image, fx: f32, fy: f32| img.get((img.w as f32 * fx) as usize, (img.h as f32 * fy) as usize);
    match decode(svg) {
        Some(img)
            if rp(&img, 0.25, 0.25) == rgb(220, 30, 30)
                && rp(&img, 0.75, 0.25) == rgb(30, 200, 60)
                && rp(&img, 0.25, 0.75) == rgb(40, 80, 230)
                && rp(&img, 0.75, 0.75) == rgb(240, 200, 20) =>
        {
            n += 1
        }
        _ => return Err("SVG"),
    }
    // EPS/PostScript — 4 kvadrant toʻldiriş (y-up), chidamlilik bilan
    let eps = b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 16 16\n0.863 0.118 0.118 setrgbcolor\n0 8 moveto 8 8 lineto 8 16 lineto 0 16 lineto closepath fill\n0.118 0.784 0.235 setrgbcolor\n8 8 moveto 16 8 lineto 16 16 lineto 8 16 lineto closepath fill\n0.157 0.314 0.902 setrgbcolor\n0 0 moveto 8 0 lineto 8 8 lineto 0 8 lineto closepath fill\n0.941 0.784 0.078 setrgbcolor\n8 0 moveto 16 0 lineto 16 8 lineto 8 8 lineto closepath fill\nshowpage\n";
    let close = |img: &Image, fx: f32, fy: f32, want: u32| -> bool {
        let g = rp(img, fx, fy);
        let diff = |sh: u32| ((g >> sh & 0xff) as i32 - (want >> sh & 0xff) as i32).abs();
        diff(16) < 12 && diff(8) < 12 && diff(0) < 12
    };
    match decode(eps) {
        Some(img)
            if close(&img, 0.25, 0.25, rgb(220, 30, 30))
                && close(&img, 0.75, 0.25, rgb(30, 200, 60))
                && close(&img, 0.25, 0.75, rgb(40, 80, 230))
                && close(&img, 0.75, 0.75, rgb(240, 200, 20)) =>
        {
            n += 1
        }
        _ => return Err("EPS"),
    }
    // PDF — 4 kvadrant vektor (re/f/rg), chidamlilik bilan
    match decode(testdata::PDF) {
        None => return Err("PDF-none"),
        Some(img) => {
            if close(&img, 0.25, 0.25, rgb(220, 30, 30))
                && close(&img, 0.75, 0.25, rgb(30, 200, 60))
                && close(&img, 0.25, 0.75, rgb(40, 80, 230))
                && close(&img, 0.75, 0.75, rgb(240, 200, 20))
            {
                n += 1;
            } else {
                return Err("PDF-color");
            }
        }
    }
    // DXF — chiziqli (CAD): dekod bölib, qora piksellar (chiziqlar) borligini tekşiramiz
    let dxf = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n0\n20\n0\n11\n100\n21\n0\n0\nLINE\n10\n0\n20\n0\n11\n0\n21\n100\n0\nCIRCLE\n10\n50\n20\n50\n40\n40\n0\nENDSEC\n0\nEOF\n";
    match decode_file("a.dxf", dxf) {
        Some(img) if img.px.iter().any(|&p| (p >> 16 & 0xff) < 0x60) => n += 1,
        _ => return Err("DXF"),
    }
    // WMF — qizil töğriburçak (GDI): markazda qizil piksel bölişi kerak
    match decode(testdata::WMF) {
        Some(img) if close(&img, 0.5, 0.5, rgb(255, 0, 0)) => n += 1,
        _ => return Err("WMF"),
    }
    // APNG — harakatli PNG: bir neçta kadr, kadrlar farqli
    match decode_frames("a.apng", apng_test::APNG) {
        Some(fr) if fr.len() >= 2 && fr[0].0.px != fr[fr.len() / 2].0.px => n += 1,
        _ => return Err("APNG"),
    }
    Ok(n)
}
