// AVI (RIFF) konteyner tahlilçisi — MJPEG kadrlarini ajratadi.
// hdrl/avih (öl'çam, FPS, kadrlar soni) + strl/strf (fourcc) + movi (kadrlar).

use alloc::vec::Vec;

fn u32le(d: &[u8], o: usize) -> u32 {
    if o + 4 > d.len() {
        return 0;
    }
    d[o] as u32 | (d[o + 1] as u32) << 8 | (d[o + 2] as u32) << 16 | (d[o + 3] as u32) << 24
}

pub struct AviInfo {
    pub w: u32,
    pub h: u32,
    pub us_per_frame: u32, // mikrosoniya/kadr
    pub total_frames: u32,
    pub fourcc: [u8; 4],
    pub movi: (usize, usize), // movi LIST tanasining [boşi, oxiri)
}

/// AVI mi?
pub fn is_avi(d: &[u8]) -> bool {
    d.len() >= 12 && &d[..4] == b"RIFF" && &d[8..12] == b"AVI "
}

pub fn parse(d: &[u8]) -> Option<AviInfo> {
    if !is_avi(d) {
        return None;
    }
    let riff_end = (12 + u32le(d, 4) as usize).min(d.len());
    let (mut w, mut h, mut uspf, mut total) = (0u32, 0u32, 0u32, 0u32);
    let mut fourcc = [0u8; 4];
    let mut movi = (0usize, 0usize);
    let mut o = 12;
    while o + 8 <= riff_end {
        let cid = &d[o..o + 4];
        let sz = u32le(d, o + 4) as usize;
        let body = o + 8;
        if body + sz > d.len() {
            break;
        }
        if cid == b"LIST" && sz >= 4 {
            let typ = &d[body..body + 4];
            if typ == b"hdrl" {
                parse_hdrl(d, body + 4, body + sz, &mut w, &mut h, &mut uspf, &mut total, &mut fourcc);
            } else if typ == b"movi" {
                movi = (body + 4, body + sz);
            }
        }
        o = body + sz + (sz & 1);
    }
    if movi.0 == 0 {
        return None;
    }
    Some(AviInfo { w, h, us_per_frame: uspf, total_frames: total, fourcc, movi })
}

fn parse_hdrl(
    d: &[u8],
    start: usize,
    end: usize,
    w: &mut u32,
    h: &mut u32,
    uspf: &mut u32,
    total: &mut u32,
    fourcc: &mut [u8; 4],
) {
    let mut o = start;
    while o + 8 <= end {
        let cid = &d[o..o + 4];
        let sz = u32le(d, o + 4) as usize;
        let body = o + 8;
        if body + sz > d.len() {
            break;
        }
        if cid == b"avih" {
            *uspf = u32le(d, body);
            *total = u32le(d, body + 16);
            *w = u32le(d, body + 32);
            *h = u32le(d, body + 36);
        } else if cid == b"LIST" && sz >= 4 && &d[body..body + 4] == b"strl" {
            // strl içidan strf (BITMAPINFOHEADER) ni topamiz
            let mut p = body + 4;
            while p + 8 <= body + sz {
                let scid = &d[p..p + 4];
                let ssz = u32le(d, p + 4) as usize;
                let sbody = p + 8;
                if sbody + ssz > d.len() {
                    break;
                }
                if scid == b"strf" && ssz >= 20 {
                    // biCompression fourcc @ +16
                    fourcc.copy_from_slice(&d[sbody + 16..sbody + 20]);
                }
                p = sbody + ssz + (ssz & 1);
            }
        }
        o = body + sz + (sz & 1);
    }
}

/// movi içidagi video kadr maʼlumot böl'aklarini qaytaradi (JPEG baytlari).
pub fn frame_slices<'a>(d: &'a [u8], info: &AviInfo) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    collect(d, info.movi.0, info.movi.1, &mut out);
    out
}

fn collect<'a>(d: &'a [u8], start: usize, end: usize, out: &mut Vec<&'a [u8]>) {
    let mut o = start;
    while o + 8 <= end {
        let cid = &d[o..o + 4];
        let sz = u32le(d, o + 4) as usize;
        let body = o + 8;
        if body + sz > d.len() {
            break;
        }
        if cid == b"LIST" && sz >= 4 && &d[body..body + 4] == b"rec " {
            // ba'zi AVI lar kadrlarni 'rec ' LIST ga guruhlaydi
            collect(d, body + 4, body + sz, out);
        } else if cid.len() == 4 && (&cid[2..4] == b"dc" || &cid[2..4] == b"db") {
            // '##dc'/'##db' — siqilgan/siqilmagan video kadr
            if sz >= 2 {
                out.push(&d[body..body + sz]);
            }
        }
        o = body + sz + (sz & 1);
        if out.len() > 100_000 {
            break;
        }
    }
}
