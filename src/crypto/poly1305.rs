//! Poly1305 xabar autentifikatsiya kodi (RFC 8439). 26-bitli böliklar bilan hisoblanadi.

const MASK: u32 = 0x3ff_ffff;

pub struct Poly1305 {
    r: [u32; 5],
    h: [u32; 5],
    pad: [u32; 4],
    buf: [u8; 16],
    len: usize,
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

impl Poly1305 {
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            r: [
                le32(&key[0..]) & 0x3ff_ffff,
                (le32(&key[3..]) >> 2) & 0x3ff_ff03,
                (le32(&key[6..]) >> 4) & 0x3ff_c0ff,
                (le32(&key[9..]) >> 6) & 0x3f0_3fff,
                (le32(&key[12..]) >> 8) & 0x00f_ffff,
            ],
            h: [0; 5],
            pad: [le32(&key[16..]), le32(&key[20..]), le32(&key[24..]), le32(&key[28..])],
            buf: [0; 16],
            len: 0,
        }
    }

    fn block(&mut self, m: &[u8; 16], hibit: u32) {
        let [r0, r1, r2, r3, r4] = self.r.map(|x| x as u64);
        let (s1, s2, s3, s4) = (r1 * 5, r2 * 5, r3 * 5, r4 * 5);
        let h = &mut self.h;
        h[0] += le32(&m[0..]) & MASK;
        h[1] += (le32(&m[3..]) >> 2) & MASK;
        h[2] += (le32(&m[6..]) >> 4) & MASK;
        h[3] += (le32(&m[9..]) >> 6) & MASK;
        h[4] += (le32(&m[12..]) >> 8) | hibit;
        let [h0, h1, h2, h3, h4] = h.map(|x| x as u64);

        let d0 = h0 * r0 + h1 * s4 + h2 * s3 + h3 * s2 + h4 * s1;
        let mut d1 = h0 * r1 + h1 * r0 + h2 * s4 + h3 * s3 + h4 * s2;
        let mut d2 = h0 * r2 + h1 * r1 + h2 * r0 + h3 * s4 + h4 * s3;
        let mut d3 = h0 * r3 + h1 * r2 + h2 * r1 + h3 * r0 + h4 * s4;
        let mut d4 = h0 * r4 + h1 * r3 + h2 * r2 + h3 * r1 + h4 * r0;

        let mut c = d0 >> 26;
        h[0] = d0 as u32 & MASK;
        d1 += c;
        c = d1 >> 26;
        h[1] = d1 as u32 & MASK;
        d2 += c;
        c = d2 >> 26;
        h[2] = d2 as u32 & MASK;
        d3 += c;
        c = d3 >> 26;
        h[3] = d3 as u32 & MASK;
        d4 += c;
        c = d4 >> 26;
        h[4] = d4 as u32 & MASK;
        h[0] += (c * 5) as u32;
        let c = h[0] >> 26;
        h[0] &= MASK;
        h[1] += c;
    }

    pub fn update(&mut self, mut data: &[u8]) {
        if self.len > 0 {
            let n = (16 - self.len).min(data.len());
            self.buf[self.len..self.len + n].copy_from_slice(&data[..n]);
            self.len += n;
            data = &data[n..];
            if self.len < 16 {
                return;
            }
            let b = self.buf;
            self.block(&b, 1 << 24);
            self.len = 0;
        }
        while data.len() >= 16 {
            let b: [u8; 16] = data[..16].try_into().unwrap();
            self.block(&b, 1 << 24);
            data = &data[16..];
        }
        self.buf[..data.len()].copy_from_slice(data);
        self.len = data.len();
    }

    pub fn finish(mut self) -> [u8; 16] {
        if self.len > 0 {
            let mut b = [0u8; 16];
            b[..self.len].copy_from_slice(&self.buf[..self.len]);
            b[self.len] = 1;
            self.block(&b, 0);
        }
        let h = &mut self.h;
        // Töliq ötkazmalar
        let mut c = h[1] >> 26;
        h[1] &= MASK;
        for i in 2..5 {
            h[i] += c;
            c = h[i] >> 26;
            h[i] &= MASK;
        }
        h[0] += c * 5;
        c = h[0] >> 26;
        h[0] &= MASK;
        h[1] += c;

        // g = h + 5 - 2^130; agar g >= 0 bölsa — h örniga g
        let mut g = [0u32; 5];
        g[0] = h[0].wrapping_add(5);
        c = g[0] >> 26;
        g[0] &= MASK;
        for i in 1..4 {
            g[i] = h[i].wrapping_add(c);
            c = g[i] >> 26;
            g[i] &= MASK;
        }
        g[4] = h[4].wrapping_add(c).wrapping_sub(1 << 26);
        let mask = (g[4] >> 31).wrapping_sub(1); // g manfiy bölmasa — hammasi 1
        for i in 0..5 {
            h[i] = (h[i] & !mask) | (g[i] & mask);
        }

        let w0 = h[0] | (h[1] << 26);
        let w1 = (h[1] >> 6) | (h[2] << 20);
        let w2 = (h[2] >> 12) | (h[3] << 14);
        let w3 = (h[3] >> 18) | (h[4] << 8);
        let mut out = [0u8; 16];
        let mut f: u64 = 0;
        for (i, (w, p)) in [w0, w1, w2, w3].iter().zip(self.pad.iter()).enumerate() {
            f = (*w as u64) + (*p as u64) + (f >> 32);
            out[4 * i..4 * i + 4].copy_from_slice(&(f as u32).to_le_bytes());
        }
        out
    }
}
