// Vorbis I audio — noldan. 1-bosqiç: sarlavhalar (identifikatsiya + izohlar)
// → taniş, metama'lumot, teglar. 2-bosqiç (quyida): to'liq audio dekod.
//
// Vorbis bitlarni kiçik-tartibda (LSB-first) joylaydi.

use super::ogg;
use super::Clip;
use alloc::string::String;
use alloc::vec::Vec;

fn u32le(d: &[u8], o: usize) -> u32 {
    if o + 4 > d.len() {
        return 0;
    }
    d[o] as u32 | (d[o + 1] as u32) << 8 | (d[o + 2] as u32) << 16 | (d[o + 3] as u32) << 24
}

pub struct Ident {
    pub channels: u8,
    pub rate: u32,
    pub bitrate_nominal: i32,
    pub bs0: u16, // qisqa blok
    pub bs1: u16, // uzun blok
}

/// Identifikatsiya sarlavhasi (paket turi 1).
pub fn parse_ident(p: &[u8]) -> Option<Ident> {
    if p.len() < 30 || p[0] != 1 || &p[1..7] != b"vorbis" {
        return None;
    }
    let channels = p[11];
    let rate = u32le(p, 12);
    let bitrate_nominal = u32le(p, 20) as i32;
    let bsbyte = p[28];
    let bs0 = 1u16 << (bsbyte & 0x0f);
    let bs1 = 1u16 << (bsbyte >> 4);
    if channels == 0 || rate == 0 {
        return None;
    }
    Some(Ident { channels, rate, bitrate_nominal, bs0, bs1 })
}

/// Izoh sarlavhasi (paket turi 3) — vendor va teglar (KEY=value).
pub fn parse_comments(p: &[u8]) -> (String, Vec<(String, String)>) {
    let mut vendor = String::new();
    let mut tags = Vec::new();
    if p.len() < 7 || p[0] != 3 || &p[1..7] != b"vorbis" {
        return (vendor, tags);
    }
    let mut o = 7;
    let vlen = u32le(p, o) as usize;
    o += 4;
    if o + vlen > p.len() {
        return (vendor, tags);
    }
    vendor = String::from_utf8_lossy(&p[o..o + vlen]).into_owned();
    o += vlen;
    if o + 4 > p.len() {
        return (vendor, tags);
    }
    let n = u32le(p, o) as usize;
    o += 4;
    for _ in 0..n.min(256) {
        if o + 4 > p.len() {
            break;
        }
        let l = u32le(p, o) as usize;
        o += 4;
        if o + l > p.len() {
            break;
        }
        let s = String::from_utf8_lossy(&p[o..o + l]);
        o += l;
        if let Some((k, v)) = s.split_once('=') {
            tags.push((k.to_ascii_uppercase(), String::from(v)));
        }
    }
    (vendor, tags)
}

pub struct VorbisInfo {
    pub channels: u8,
    pub rate: u32,
    pub bitrate_nominal: i32,
    pub bs0: u16,
    pub bs1: u16,
    pub vendor: String,
    pub tags: Vec<(String, String)>,
    pub duration_ms: u32,
}

impl VorbisInfo {
    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// OGG baytlaridan Vorbis metama'lumotini chiqaradi.
pub fn info(d: &[u8]) -> Option<VorbisInfo> {
    let stream = ogg::parse(d)?;
    let id = parse_ident(stream.packets.first()?)?;
    let (vendor, tags) = if stream.packets.len() > 1 {
        parse_comments(&stream.packets[1])
    } else {
        (String::new(), Vec::new())
    };
    let duration_ms = if id.rate > 0 && stream.last_granule > 0 {
        (stream.last_granule as u64 * 1000 / id.rate as u64) as u32
    } else {
        0
    };
    Some(VorbisInfo {
        channels: id.channels,
        rate: id.rate,
        bitrate_nominal: id.bitrate_nominal,
        bs0: id.bs0,
        bs1: id.bs1,
        vendor,
        tags,
        duration_ms,
    })
}

/// OGG/Vorbis mi?
pub fn is_vorbis(d: &[u8]) -> bool {
    if !ogg::is_ogg(d) {
        return false;
    }
    // birinçi paket \x01vorbis bilan boşlanişi kerak
    if let Some(s) = ogg::parse(d) {
        if let Some(p) = s.packets.first() {
            return p.len() >= 7 && p[0] == 1 && &p[1..7] == b"vorbis";
        }
    }
    false
}

// 2-bosqiç: to'liq audio dekod (keyingi commit da amalga oşiriladi).
pub fn decode(_d: &[u8]) -> Option<Clip> {
    None
}
