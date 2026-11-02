// MP4 / MOV (ISO BMFF) taniş va metama'lumot (öl'çam, davomiylik).
// Box (atom) daraxtini kezadi. Dekod yöq (H.264/HEVC noldan amaliy emas),
// ammo konteyner va kodek taniladi.

fn be32(d: &[u8], o: usize) -> u64 {
    if o + 4 > d.len() {
        return 0;
    }
    (d[o] as u64) << 24 | (d[o + 1] as u64) << 16 | (d[o + 2] as u64) << 8 | d[o + 3] as u64
}
fn be64(d: &[u8], o: usize) -> u64 {
    be32(d, o) << 32 | be32(d, o + 4)
}

pub struct Mp4Info {
    pub is_mov: bool,
    pub w: u32,
    pub h: u32,
    pub duration_ms: u32,
    pub codec: [u8; 4],
}

/// MP4/MOV mi? (birinçi box turi böyiça).
pub fn is_mp4(d: &[u8]) -> bool {
    if d.len() < 12 {
        return false;
    }
    let typ = &d[4..8];
    matches!(typ, b"ftyp" | b"moov" | b"mdat" | b"free" | b"skip" | b"wide" | b"pnot" | b"styp")
}

pub fn info(d: &[u8]) -> Option<Mp4Info> {
    if !is_mp4(d) {
        return None;
    }
    let mut is_mov = false;
    if &d[4..8] == b"ftyp" && d.len() >= 12 {
        is_mov = &d[8..12] == b"qt  ";
    }
    let (mut w, mut h, mut dur_ms, mut codec) = (0u32, 0u32, 0u32, [0u8; 4]);
    walk(d, 0, d.len(), &mut w, &mut h, &mut dur_ms, &mut codec, 0);
    Some(Mp4Info { is_mov, w, h, duration_ms: dur_ms, codec })
}

fn walk(d: &[u8], start: usize, end: usize, w: &mut u32, h: &mut u32, dur: &mut u32, codec: &mut [u8; 4], depth: u8) {
    if depth > 8 {
        return;
    }
    let mut o = start;
    while o + 8 <= end {
        let mut size = be32(d, o) as usize;
        let typ = &d[o + 4..o + 8];
        let mut body = o + 8;
        if size == 1 {
            // 64-bit kattalik
            size = be64(d, o + 8) as usize;
            body = o + 16;
        } else if size == 0 {
            size = end - o;
        }
        if size < 8 || o + size > end {
            break;
        }
        match typ {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" => {
                walk(d, body, o + size, w, h, dur, codec, depth + 1)
            }
            b"mvhd" => {
                // version @body, timescale/duration joylaşuvi versiyaga bogʼliq
                let ver = d.get(body).copied().unwrap_or(0);
                let (ts, du) = if ver == 1 {
                    (be32(d, body + 20), be64(d, body + 24))
                } else {
                    (be32(d, body + 12), be32(d, body + 16) as u64)
                };
                if ts > 0 {
                    *dur = (du * 1000 / ts as u64) as u32;
                }
            }
            b"tkhd" => {
                let ver = d.get(body).copied().unwrap_or(0);
                let base = if ver == 1 { body + 32 } else { body + 20 };
                // oxirgi 8 bayt: width,height (16.16 qoʼzgʼaluvçan nuqta)
                let wo = base + 52;
                let ww = be32(d, wo) >> 16;
                let hh = be32(d, wo + 4) >> 16;
                if ww > 0 && hh > 0 {
                    *w = ww as u32;
                    *h = hh as u32;
                }
            }
            b"stsd" => {
                // birinçi tavsif: box kattaligi(4)+format(4) @ body+8
                if body + 16 <= d.len() {
                    codec.copy_from_slice(&d[body + 12..body + 16]);
                }
            }
            _ => {}
        }
        o += size;
    }
}
