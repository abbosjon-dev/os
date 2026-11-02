//! JPEG (baseline, Huffman) dekoderi — noldan.
//! DQT/DHT/SOF0/SOS, DC/AC Huffman, dekvantlaş, teskari zigzag, IDCT (8x8),
//! YCbCr→RGB, xroma upsampling (replikatsiya). Progressive qöllanmaydi.

use super::{rgb, Image};
use alloc::vec::Vec;

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21,
    28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61,
    54, 47, 55, 62, 63,
];

#[derive(Default, Clone)]
struct Huff {
    count: [u16; 17],
    symbol: Vec<u8>,
}

impl Huff {
    fn decode(&self, br: &mut BitReader) -> Option<u8> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..=16 {
            code |= br.bit()? as i32;
            let count = self.count[len] as i32;
            if code - first < count {
                return self.symbol.get((index + code - first) as usize).copied();
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }
}

struct BitReader<'a> {
    d: &'a [u8],
    pos: usize,
    cur: u32,
    bits: u32,
    marker: u8,
}

impl<'a> BitReader<'a> {
    fn new(d: &'a [u8], pos: usize) -> Self {
        BitReader { d, pos, cur: 0, bits: 0, marker: 0 }
    }
    /// Bitta bit (MSB-first), bayt-stuffing (0xFF00) bilan.
    fn bit(&mut self) -> Option<u32> {
        if self.bits == 0 {
            if self.pos >= self.d.len() {
                return None;
            }
            let mut b = self.d[self.pos];
            self.pos += 1;
            if b == 0xff {
                let n = *self.d.get(self.pos)?;
                if n == 0 {
                    self.pos += 1; // stuffed
                } else {
                    self.marker = n; // marker (RST yoki EOI)
                    b = 0;
                }
            }
            self.cur = b as u32;
            self.bits = 8;
        }
        self.bits -= 1;
        Some((self.cur >> self.bits) & 1)
    }
    fn receive(&mut self, n: u32) -> Option<i32> {
        let mut v = 0i32;
        for _ in 0..n {
            v = (v << 1) | self.bit()? as i32;
        }
        Some(v)
    }
    fn restart(&mut self) {
        self.bits = 0;
        self.marker = 0;
        // RSTn markerini ötkazamiz
        if self.pos + 1 < self.d.len() && self.d[self.pos] == 0xff && (0xd0..=0xd7).contains(&self.d[self.pos + 1]) {
            self.pos += 2;
        }
    }
}

fn extend(v: i32, n: u32) -> i32 {
    if n == 0 {
        0
    } else if v < (1 << (n - 1)) {
        v - (1 << n) + 1
    } else {
        v
    }
}

#[derive(Default, Clone)]
struct Comp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize, // kvant jadvali
    td: usize, // DC Huffman
    ta: usize, // AC Huffman
    pred: i32,
    plane: Vec<u8>,
    pw: usize,
    ph: usize,
}

fn idct8x8(blk: &mut [f32; 64]) {
    // Oldindan hisoblangan kosinuslar
    // s[x][u] = C(u)*cos((2x+1)*u*pi/16)/2
    const C: [[f32; 8]; 8] = cos_table();
    let mut tmp = [0f32; 64];
    // Qatorlar
    for y in 0..8 {
        for x in 0..8 {
            let mut s = 0f32;
            for u in 0..8 {
                s += C[x][u] * blk[y * 8 + u];
            }
            tmp[y * 8 + x] = s;
        }
    }
    // Ustunlar
    for x in 0..8 {
        for y in 0..8 {
            let mut s = 0f32;
            for u in 0..8 {
                s += C[y][u] * tmp[u * 8 + x];
            }
            blk[y * 8 + x] = s;
        }
    }
}

const fn cos_table() -> [[f32; 8]; 8] {
    // const kontekstda f32 cos yöq — qoʻlda hisoblangan qiymatlar (cos(k*pi/16)).
    // c[k] = cos(k*pi/16)
    let c: [f32; 9] = [
        1.0, 0.98078528, 0.92387953, 0.83146961, 0.70710678, 0.55557023, 0.38268343, 0.19509032, 0.0,
    ];
    let mut t = [[0f32; 8]; 8];
    let mut x = 0;
    while x < 8 {
        let mut u = 0;
        while u < 8 {
            // cos((2x+1)*u*pi/16): (2x+1)*u ni 0..31 ga keltirib, c[] va işoradan foydalanamiz
            let k = ((2 * x + 1) * u) % 32;
            let (idx, sign) = if k <= 8 {
                (k, 1.0)
            } else if k <= 16 {
                (16 - k, -1.0)
            } else if k <= 24 {
                (k - 16, -1.0)
            } else {
                (32 - k, 1.0)
            };
            let cu = if u == 0 { 0.70710678f32 } else { 1.0 };
            t[x][u] = sign * c[idx] * cu * 0.5;
            u += 1;
        }
        x += 1;
    }
    t
}

pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 4 || d[0] != 0xff || d[1] != 0xd8 {
        return None;
    }
    let mut pos = 2;
    let mut qtab: [[u16; 64]; 4] = [[0; 64]; 4];
    let mut hdc: [Huff; 4] = Default::default();
    let mut hac: [Huff; 4] = Default::default();
    let (mut width, mut height) = (0usize, 0usize);
    let mut comps: Vec<Comp> = Vec::new();
    let mut restart_interval = 0usize;

    while pos + 4 <= d.len() {
        if d[pos] != 0xff {
            pos += 1;
            continue;
        }
        let marker = d[pos + 1];
        pos += 2;
        if marker == 0xd9 {
            break; // EOI
        }
        if (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
            continue;
        }
        let seg_len = ((d[pos] as usize) << 8 | d[pos + 1] as usize).max(2);
        let seg = &d[pos + 2..(pos + seg_len).min(d.len())];
        match marker {
            0xdb => {
                // DQT
                let mut p = 0;
                while p < seg.len() {
                    let prec = seg[p] >> 4;
                    let id = (seg[p] & 15) as usize;
                    p += 1;
                    if id >= 4 {
                        return None;
                    }
                    for i in 0..64 {
                        qtab[id][i] = if prec == 0 {
                            let v = *seg.get(p)? as u16;
                            p += 1;
                            v
                        } else {
                            let v = (*seg.get(p)? as u16) << 8 | *seg.get(p + 1)? as u16;
                            p += 2;
                            v
                        };
                    }
                }
            }
            0xc0 | 0xc1 => {
                // SOF0/1 (baseline)
                height = (seg[1] as usize) << 8 | seg[2] as usize;
                width = (seg[3] as usize) << 8 | seg[4] as usize;
                let nc = seg[5] as usize;
                if nc > 4 || width == 0 || height == 0 || width > 8192 || height > 8192 {
                    return None;
                }
                for i in 0..nc {
                    let o = 6 + i * 3;
                    comps.push(Comp {
                        id: seg[o],
                        h: (seg[o + 1] >> 4) as usize,
                        v: (seg[o + 1] & 15) as usize,
                        tq: seg[o + 2] as usize,
                        ..Default::default()
                    });
                }
            }
            0xc2 => return None, // progressive — qöllanmaydi
            0xc4 => {
                // DHT
                let mut p = 0;
                while p < seg.len() {
                    let class = seg[p] >> 4;
                    let id = (seg[p] & 15) as usize;
                    p += 1;
                    if id >= 4 {
                        return None;
                    }
                    let mut h = Huff::default();
                    let mut total = 0usize;
                    for l in 1..=16 {
                        h.count[l] = seg[p] as u16;
                        total += seg[p] as usize;
                        p += 1;
                    }
                    for _ in 0..total {
                        h.symbol.push(*seg.get(p)?);
                        p += 1;
                    }
                    if class == 0 {
                        hdc[id] = h;
                    } else {
                        hac[id] = h;
                    }
                }
            }
            0xdd => {
                restart_interval = (seg[0] as usize) << 8 | seg[1] as usize;
            }
            0xda => {
                // SOS
                let ns = seg[0] as usize;
                for i in 0..ns {
                    let cid = seg[1 + i * 2];
                    let t = seg[2 + i * 2];
                    if let Some(c) = comps.iter_mut().find(|c| c.id == cid) {
                        c.td = (t >> 4) as usize;
                        c.ta = (t & 15) as usize;
                    }
                }
                let scan_start = pos + 2 + 1 + ns * 2 + 3;
                return decode_scan(d, scan_start, width, height, &mut comps, &qtab, &hdc, &hac, restart_interval);
            }
            _ => {}
        }
        pos += seg_len;
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn decode_scan(
    d: &[u8], start: usize, width: usize, height: usize, comps: &mut [Comp], qtab: &[[u16; 64]; 4], hdc: &[Huff; 4],
    hac: &[Huff; 4], restart_interval: usize,
) -> Option<Image> {
    let max_h = comps.iter().map(|c| c.h).max()?;
    let max_v = comps.iter().map(|c| c.v).max()?;
    if max_h == 0 || max_v == 0 {
        return None;
    }
    let mcu_w = 8 * max_h;
    let mcu_h = 8 * max_v;
    let mcus_x = width.div_ceil(mcu_w);
    let mcus_y = height.div_ceil(mcu_h);
    // Komponent tekisliklari
    for c in comps.iter_mut() {
        c.pw = mcus_x * c.h * 8;
        c.ph = mcus_y * c.v * 8;
        c.plane = alloc::vec![0u8; c.pw * c.ph];
        c.pred = 0;
    }
    let mut br = BitReader::new(d, start);
    let mut mcu_count = 0usize;
    for my in 0..mcus_y {
        for mx in 0..mcus_x {
            if restart_interval != 0 && mcu_count != 0 && mcu_count % restart_interval == 0 {
                br.restart();
                for c in comps.iter_mut() {
                    c.pred = 0;
                }
            }
            for ci in 0..comps.len() {
                let (h, v, tq, td, ta) = {
                    let c = &comps[ci];
                    (c.h, c.v, c.tq, c.td, c.ta)
                };
                for by in 0..v {
                    for bx in 0..h {
                        let mut block = [0f32; 64];
                        decode_block(&mut br, &mut comps[ci].pred, &hdc[td], &hac[ta], &qtab[tq], &mut block)?;
                        idct8x8(&mut block);
                        // Tekislikka joylaymiz
                        let px0 = (mx * h + bx) * 8;
                        let py0 = (my * v + by) * 8;
                        let c = &mut comps[ci];
                        for yy in 0..8 {
                            for xx in 0..8 {
                                let val = (block[yy * 8 + xx] + 128.0).clamp(0.0, 255.0) as u8;
                                let px = px0 + xx;
                                let py = py0 + yy;
                                if px < c.pw && py < c.ph {
                                    c.plane[py * c.pw + px] = val;
                                }
                            }
                        }
                    }
                }
            }
            mcu_count += 1;
        }
    }
    // Rangga aylantiramiz
    let mut img = Image::new(width, height);
    let ncomp = comps.len();
    for y in 0..height {
        for x in 0..width {
            if ncomp == 1 {
                let g = sample_comp(&comps[0], x, y, max_h, max_v);
                img.px[y * width + x] = rgb(g, g, g);
            } else {
                let yy = sample_comp(&comps[0], x, y, max_h, max_v) as f32;
                let cb = sample_comp(&comps[1], x, y, max_h, max_v) as f32 - 128.0;
                let cr = sample_comp(&comps[2], x, y, max_h, max_v) as f32 - 128.0;
                let r = (yy + 1.402 * cr).clamp(0.0, 255.0) as u8;
                let g = (yy - 0.344136 * cb - 0.714136 * cr).clamp(0.0, 255.0) as u8;
                let b = (yy + 1.772 * cb).clamp(0.0, 255.0) as u8;
                img.px[y * width + x] = rgb(r, g, b);
            }
        }
    }
    Some(img)
}

fn sample_comp(c: &Comp, x: usize, y: usize, max_h: usize, max_v: usize) -> u8 {
    let sx = x * c.h / max_h;
    let sy = y * c.v / max_v;
    c.plane.get(sy.min(c.ph.saturating_sub(1)) * c.pw + sx.min(c.pw.saturating_sub(1))).copied().unwrap_or(0)
}

fn decode_block(br: &mut BitReader, pred: &mut i32, dc: &Huff, ac: &Huff, q: &[u16; 64], out: &mut [f32; 64]) -> Option<()> {
    let t = dc.decode(br)? as u32;
    let diff = extend(br.receive(t)?, t);
    *pred += diff;
    out[0] = (*pred * q[0] as i32) as f32;
    let mut k = 1;
    while k < 64 {
        let rs = ac.decode(br)?;
        let r = (rs >> 4) as usize;
        let s = (rs & 15) as u32;
        if s == 0 {
            if r == 15 {
                k += 16;
                continue;
            }
            break; // EOB
        }
        k += r;
        if k >= 64 {
            break;
        }
        let val = extend(br.receive(s)?, s);
        out[ZIGZAG[k]] = (val * q[k] as i32) as f32;
        k += 1;
    }
    Some(())
}
