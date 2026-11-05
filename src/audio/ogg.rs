// OGG konteyner — sahifalarni tahlil qilib paketlarni yigʼadi.
// Bitta mantiqiy oqim (birinçi serial) qöllanadi. Vorbis uçun.

use alloc::vec::Vec;

fn u32le(d: &[u8], o: usize) -> u32 {
    d[o] as u32 | (d[o + 1] as u32) << 8 | (d[o + 2] as u32) << 16 | (d[o + 3] as u32) << 24
}
fn i64le(d: &[u8], o: usize) -> i64 {
    let mut v = 0u64;
    for i in 0..8 {
        v |= (d[o + i] as u64) << (8 * i);
    }
    v as i64
}

pub struct OggStream {
    pub serial: u32,
    pub packets: Vec<Vec<u8>>,
    pub last_granule: i64,
}

/// OGG mi?
pub fn is_ogg(d: &[u8]) -> bool {
    d.len() >= 4 && &d[..4] == b"OggS"
}

/// Birinçi mantiqiy oqimning paketlarini yigʼadi.
pub fn parse(d: &[u8]) -> Option<OggStream> {
    if !is_ogg(d) {
        return None;
    }
    let mut packets: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new(); // davom etayotgan paket
    let mut serial = 0u32;
    let mut have_serial = false;
    let mut last_granule = 0i64;
    let mut o = 0usize;
    while o + 27 <= d.len() && &d[o..o + 4] == b"OggS" {
        // Sahifa sarlavhasi: capture(4) ver(1) htype(1) granule(8)=6..14
        // serial(4)=14..18 seq(4)=18..22 crc(4)=22..26 nseg(1)=26
        let htype = d[o + 5];
        let gran = i64le(d, o + 6);
        let serial_real = u32le(d, o + 14);
        let nseg = d[o + 26] as usize;
        if o + 27 + nseg > d.len() {
            break;
        }
        let segs = &d[o + 27..o + 27 + nseg];
        let body = o + 27 + nseg;
        let total: usize = segs.iter().map(|&s| s as usize).sum();
        if body + total > d.len() {
            break;
        }
        let _ = htype;
        if !have_serial {
            serial = serial_real;
            have_serial = true;
        }
        if gran >= 0 {
            last_granule = gran;
        }
        if serial_real == serial {
            // segmentlar böyiça paketlarni ajratamiz (255 — davom, <255 — tugaş)
            let mut p = body;
            for &s in segs {
                let s = s as usize;
                cur.extend_from_slice(&d[p..p + s]);
                p += s;
                if s < 255 {
                    packets.push(core::mem::take(&mut cur));
                }
            }
        }
        o = o + 27 + nseg + total;
    }
    if !cur.is_empty() {
        packets.push(cur);
    }
    if packets.is_empty() {
        return None;
    }
    Some(OggStream { serial, packets, last_granule })
}
