//! CDR (CorelDRAW) — yopiq/hujjatsiz RIFF formati. Toʻliq dekodlaş amalda
//! mumkin emas; shuning uçun içiga joylangan raster önköriniş (PNG/JPEG/BMP)
//! bölsa — öşani çiqaramiz. Boʻlmasa — None (koʻruvçi xabar beradi).

use super::Image;

pub fn decode(d: &[u8]) -> Option<Image> {
    // RIFF ... CDR tekşiruvi (ixtiyoriy — kengaytma böyiça ham kelishi mumkin)
    // Faylni skanlab, joylangan PNG yoki JPEG önkörinişni topamiz.
    let mut best: Option<Image> = None;
    let mut i = 0;
    while i + 8 < d.len() {
        if &d[i..i + 8] == b"\x89PNG\r\n\x1a\n" {
            if let Some(img) = super::png::decode(&d[i..]) {
                if better(&best, &img) {
                    best = Some(img);
                }
            }
            i += 8;
        } else if d[i] == 0xff && d[i + 1] == 0xd8 && d[i + 2] == 0xff {
            if let Some(img) = super::jpeg::decode(&d[i..]) {
                if better(&best, &img) {
                    best = Some(img);
                }
            }
            i += 2;
        } else {
            i += 1;
        }
        // haddan ziyod uzoq qidirmaymiz
        if i > 8 * 1024 * 1024 {
            break;
        }
    }
    best
}

fn better(cur: &Option<Image>, img: &Image) -> bool {
    img.w >= 16 && img.h >= 16 && cur.as_ref().map(|c| img.w * img.h > c.w * c.h).unwrap_or(true)
}
