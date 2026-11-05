//! PNG kodlovçisi — noldan: qator filtrlari (None/Sub/Up — eng yaxşisi tanlanadi),
//! zlib/deflate (qatʼiy Huffman + LZ77 xesh zanjiri, 32 KB oyna), CRC32, Adler-32.

use alloc::vec;
use alloc::vec::Vec;

fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (n, e) in t.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    t
}

pub fn crc32(t: &[u32; 256], parts: &[&[u8]]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for p in parts {
        for &b in *p {
            c = t[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
        }
    }
    c ^ 0xffff_ffff
}

fn adler32(d: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in d.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, n: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Huffman kodi (MSB birinçi) — teskari tartibda yoziladi.
    fn huff(&mut self, code: u32, len: u32) {
        let mut r = 0;
        for i in 0..len {
            r |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(r, len);
    }
    fn sym(&mut self, s: u32) {
        match s {
            0..=143 => self.huff(0x30 + s, 8),
            144..=255 => self.huff(0x190 + s - 144, 9),
            256..=279 => self.huff(s - 256, 7),
            _ => self.huff(0xc0 + s - 280, 8),
        }
    }
    fn flush(&mut self) {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.acc = 0;
        self.n = 0;
    }
}

const LBASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

const WIN: usize = 1 << 15;

fn hash(d: &[u8], i: usize) -> usize {
    (((d[i] as usize) << 10) ^ ((d[i + 1] as usize) << 5) ^ d[i + 2] as usize) & (WIN - 1)
}

/// zlib oqimi (deflate, bitta qatʼiy-Huffman bloki).
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut b = Bits { out: Vec::with_capacity(data.len() / 2 + 64), acc: 0, n: 0 };
    b.out.extend_from_slice(&[0x78, 0x01]);
    b.put(1, 1); // BFINAL
    b.put(1, 2); // BTYPE = 01 (qatʼiy Huffman)
    let n = data.len();
    let mut head = vec![u32::MAX; WIN];
    let mut chain = vec![u32::MAX; WIN];
    let insert = |head: &mut Vec<u32>, chain: &mut Vec<u32>, p: usize| {
        if p + 3 <= n {
            let h = hash(data, p);
            chain[p & (WIN - 1)] = head[h];
            head[h] = p as u32;
        }
    };
    let mut i = 0;
    while i < n {
        let (mut best, mut dist) = (0usize, 0usize);
        if i + 3 <= n {
            let mut c = head[hash(data, i)];
            let mut tries = 24;
            while c != u32::MAX && tries > 0 {
                let cu = c as usize;
                if i - cu > WIN - 1 {
                    break;
                }
                let mut l = 0;
                while l < 258 && i + l < n && data[cu + l] == data[i + l] {
                    l += 1;
                }
                if l > best {
                    best = l;
                    dist = i - cu;
                    if l == 258 {
                        break;
                    }
                }
                c = chain[cu & (WIN - 1)];
                tries -= 1;
            }
        }
        if best >= 3 {
            let k = LBASE.iter().rposition(|&x| x as usize <= best).unwrap();
            b.sym(257 + k as u32);
            b.put((best - LBASE[k] as usize) as u32, LEXT[k] as u32);
            let k = DBASE.iter().rposition(|&x| x as usize <= dist).unwrap();
            b.huff(k as u32, 5);
            b.put((dist - DBASE[k] as usize) as u32, DEXT[k] as u32);
            for p in i..i + best {
                insert(&mut head, &mut chain, p);
            }
            i += best;
        } else {
            b.sym(data[i] as u32);
            insert(&mut head, &mut chain, i);
            i += 1;
        }
    }
    b.sym(256);
    b.flush();
    b.out.extend_from_slice(&adler32(data).to_be_bytes());
    b.out
}

fn filt(f: u8, cur: &[u8], prev: &[u8], i: usize) -> u8 {
    match f {
        0 => cur[i],
        1 => cur[i].wrapping_sub(if i >= 3 { cur[i - 3] } else { 0 }),
        _ => cur[i].wrapping_sub(prev[i]),
    }
}

/// 0xRRGGBB piksellardan PNG fayl (RGB, 8 bit).
pub fn encode(w: usize, h: usize, px: &[u32]) -> Vec<u8> {
    let stride = w * 3;
    let mut raw = Vec::with_capacity((stride + 1) * h);
    let mut prev = vec![0u8; stride];
    let mut cur = vec![0u8; stride];
    for y in 0..h {
        for x in 0..w {
            let p = px[y * w + x];
            cur[x * 3] = (p >> 16) as u8;
            cur[x * 3 + 1] = (p >> 8) as u8;
            cur[x * 3 + 2] = p as u8;
        }
        let mut best = (u64::MAX, 0u8);
        for f in 0..3u8 {
            let s: u64 = (0..stride).map(|i| (filt(f, &cur, &prev, i) as i8).unsigned_abs() as u64).sum();
            if s < best.0 {
                best = (s, f);
            }
        }
        raw.push(best.1);
        for i in 0..stride {
            raw.push(filt(best.1, &cur, &prev, i));
        }
        core::mem::swap(&mut prev, &mut cur);
    }
    let t = crc_table();
    let mut out = Vec::from(&b"\x89PNG\r\n\x1a\n"[..]);
    let mut chunk = |typ: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(typ);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc32(&t, &[typ, data]).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &zlib(&raw));
    chunk(b"IEND", &[]);
    out
}
