// Matroska / WebM (EBML) taniş. To'liq EBML tahlili murakkab; bu yerda
// sarlavha sinxroni va DocType ("matroska"/"webm") aniqlanadi.

/// EBML (MKV/WebM) mi? (0x1A45DFA3 sarlavha).
pub fn is_mkv(d: &[u8]) -> bool {
    d.len() >= 4 && d[0] == 0x1a && d[1] == 0x45 && d[2] == 0xdf && d[3] == 0xa3
}

/// WebM mi? (DocType = "webm").
pub fn is_webm(d: &[u8]) -> bool {
    is_mkv(d) && contains(&d[..d.len().min(256)], b"webm")
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    (0..=hay.len() - needle.len()).any(|i| &hay[i..i + needle.len()] == needle)
}
