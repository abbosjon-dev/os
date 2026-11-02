//! FLAC dekoderi — noldan. STREAMINFO, kadr sarlavhasi, subkadrlar
//! (CONSTANT/VERBATIM/FIXED/LPC), Rice qoldiqlari, kanal dekorrelyatsiyasi.

use super::Clip;
use alloc::vec::Vec;

struct Bits<'a> {
    d: &'a [u8],
    byte: usize,
    bit: u32, // 0..8, MSB-first
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8]) -> Self {
        Bits { d, byte: 0, bit: 0 }
    }
    fn bit1(&mut self) -> u32 {
        if self.byte >= self.d.len() {
            return 0;
        }
        let b = (self.d[self.byte] >> (7 - self.bit)) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.byte += 1;
        }
        b as u32
    }
    fn read(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.bit1();
        }
        v
    }
    fn read_i(&mut self, n: u32) -> i32 {
        let v = self.read(n);
        if n == 0 {
            0
        } else if v & (1 << (n - 1)) != 0 {
            (v as i32) - (1 << n)
        } else {
            v as i32
        }
    }
    fn unary(&mut self) -> u32 {
        let mut n = 0;
        while self.bit1() == 0 {
            n += 1;
            if self.byte >= self.d.len() {
                break;
            }
        }
        n
    }
    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.byte += 1;
        }
    }
    fn eof(&self) -> bool {
        self.byte >= self.d.len()
    }
}

pub fn decode(d: &[u8]) -> Option<Clip> {
    if d.len() < 42 || &d[..4] != b"fLaC" {
        return None;
    }
    // Metadata bloklari
    let mut pos = 4;
    let (mut rate, mut channels, mut bits_ps) = (0u32, 0u16, 0u16);
    let mut total: u64 = 0;
    loop {
        if pos + 4 > d.len() {
            return None;
        }
        let last = d[pos] & 0x80 != 0;
        let btype = d[pos] & 0x7f;
        let len = (d[pos + 1] as usize) << 16 | (d[pos + 2] as usize) << 8 | d[pos + 3] as usize;
        let body = pos + 4;
        if btype == 0 && body + 18 <= d.len() {
            // STREAMINFO
            rate = (d[body + 10] as u32) << 12 | (d[body + 11] as u32) << 4 | (d[body + 12] as u32) >> 4;
            channels = ((d[body + 12] >> 1) & 7) as u16 + 1;
            bits_ps = ((d[body + 12] & 1) << 4 | d[body + 13] >> 4) as u16 + 1;
            total = ((d[body + 13] as u64 & 0xf) << 32)
                | (d[body + 14] as u64) << 24
                | (d[body + 15] as u64) << 16
                | (d[body + 16] as u64) << 8
                | d[body + 17] as u64;
        }
        pos = body + len;
        if last {
            break;
        }
    }
    if rate == 0 || channels == 0 || channels > 8 || bits_ps == 0 || bits_ps > 32 {
        return None;
    }
    let mut bits = Bits::new(&d[pos..]);
    let ch = channels as usize;
    let mut out: Vec<i16> = Vec::with_capacity((total as usize * ch).min(1 << 26));
    let mut guard = 0u64;
    while !bits.eof() {
        guard += 1;
        if guard > 1_000_000 {
            break;
        }
        match decode_frame(&mut bits, channels, bits_ps) {
            Some(frame) => {
                // interleave + i16 ga keltiramiz
                let bs = frame[0].len();
                for i in 0..bs {
                    for c in 0..ch {
                        out.push(to_i16(frame[c][i], bits_ps));
                    }
                }
                if total > 0 && out.len() as u64 >= total * ch as u64 {
                    break;
                }
            }
            None => break,
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(Clip { rate, channels, samples: out })
}

fn to_i16(v: i32, bps: u16) -> i16 {
    let shift = bps as i32 - 16;
    let r = if shift > 0 { v >> shift } else { v << (-shift) };
    r.clamp(-32768, 32767) as i16
}

const FIXED_COEF: [&[i32]; 5] = [&[], &[1], &[2, -1], &[3, -3, 1], &[4, -6, 4, -1]];

fn decode_frame(bits: &mut Bits, channels: u16, stream_bps: u16) -> Option<Vec<Vec<i32>>> {
    // Kadr sarlavhasi
    let sync = bits.read(14);
    if sync != 0x3ffe {
        return None;
    }
    bits.read(1); // reserved
    let blocking = bits.read(1); // 0 = fixed blocksize
    let bs_code = bits.read(4);
    let sr_code = bits.read(4);
    let ch_asgn = bits.read(4);
    let _ss_code = bits.read(3);
    bits.read(1); // reserved
    // kadr/namuna raqami (UTF-8 kabi)
    read_utf8_number(bits, blocking == 1);
    let block_size = match bs_code {
        1 => 192,
        2..=5 => 576 << (bs_code - 2),
        6 => bits.read(8) as usize + 1,
        7 => bits.read(16) as usize + 1,
        8..=15 => 256 << (bs_code - 8),
        _ => return None,
    };
    match sr_code {
        12 => {
            bits.read(8);
        }
        13 | 14 => {
            bits.read(16);
        }
        _ => {}
    }
    bits.read(8); // CRC8
    if block_size == 0 || block_size > 65536 {
        return None;
    }
    // Kanal moslaşuvi
    let nch = if ch_asgn < 8 { ch_asgn as usize + 1 } else { 2 };
    if nch != channels as usize {
        return None;
    }
    let mut subframes: Vec<Vec<i32>> = Vec::with_capacity(nch);
    for c in 0..nch {
        // yon kanal uçun bps +1
        let bps = match ch_asgn {
            8 if c == 1 => stream_bps + 1, // left/side: side
            9 if c == 0 => stream_bps + 1, // right/side: side
            10 if c == 1 => stream_bps + 1, // mid/side: side
            _ => stream_bps,
        };
        subframes.push(decode_subframe(bits, block_size, bps)?);
    }
    bits.align();
    bits.read(16); // kadr CRC16
    // Kanal dekorrelyatsiyasi
    match ch_asgn {
        8 => {
            // left/side → left, right = left - side
            for i in 0..block_size {
                subframes[1][i] = subframes[0][i] - subframes[1][i];
            }
        }
        9 => {
            // right/side: ch0=side, ch1=right → left = right + side
            for i in 0..block_size {
                subframes[0][i] += subframes[1][i];
            }
        }
        10 => {
            // mid/side
            for i in 0..block_size {
                let mid = subframes[0][i];
                let side = subframes[1][i];
                let m = (mid << 1) | (side & 1);
                subframes[0][i] = (m + side) >> 1;
                subframes[1][i] = (m - side) >> 1;
            }
        }
        _ => {}
    }
    Some(subframes)
}

fn decode_subframe(bits: &mut Bits, bs: usize, bps: u16) -> Option<Vec<i32>> {
    bits.read(1); // zero
    let typ = bits.read(6);
    let wasted = if bits.bit1() == 1 { bits.unary() + 1 } else { 0 };
    let bps = bps as u32 - wasted;
    let mut out;
    if typ == 0 {
        // CONSTANT
        let v = bits.read_i(bps);
        out = alloc::vec![v; bs];
    } else if typ == 1 {
        // VERBATIM
        out = Vec::with_capacity(bs);
        for _ in 0..bs {
            out.push(bits.read_i(bps));
        }
    } else if (8..=12).contains(&typ) {
        // FIXED, order = typ - 8
        let order = (typ - 8) as usize;
        out = Vec::with_capacity(bs);
        for _ in 0..order {
            out.push(bits.read_i(bps));
        }
        read_residual(bits, &mut out, bs, order)?;
        restore_fixed(&mut out, order);
    } else if typ >= 32 {
        // LPC, order = typ - 31
        let order = (typ - 31) as usize;
        out = Vec::with_capacity(bs);
        for _ in 0..order {
            out.push(bits.read_i(bps));
        }
        let prec = bits.read(4) + 1;
        let shift = bits.read_i(5);
        let mut coefs = Vec::with_capacity(order);
        for _ in 0..order {
            coefs.push(bits.read_i(prec));
        }
        read_residual(bits, &mut out, bs, order)?;
        restore_lpc(&mut out, &coefs, shift.max(0) as u32, order);
    } else {
        return None;
    }
    if wasted > 0 {
        for v in &mut out {
            *v <<= wasted;
        }
    }
    Some(out)
}

fn read_residual(bits: &mut Bits, out: &mut Vec<i32>, bs: usize, order: usize) -> Option<()> {
    let method = bits.read(2);
    if method > 1 {
        return None;
    }
    let param_bits = if method == 0 { 4 } else { 5 };
    let escape = if method == 0 { 0xf } else { 0x1f };
    let po = bits.read(4);
    let parts = 1usize << po;
    if bs % parts != 0 {
        return None;
    }
    let part_len = bs / parts;
    for p in 0..parts {
        let count = if p == 0 { part_len - order } else { part_len };
        let rice = bits.read(param_bits);
        if rice == escape {
            let nbits = bits.read(5);
            for _ in 0..count {
                out.push(bits.read_i(nbits));
            }
        } else {
            for _ in 0..count {
                let q = bits.unary();
                let r = bits.read(rice);
                let u = (q << rice) | r;
                // zigzag
                out.push(((u >> 1) as i32) ^ -((u & 1) as i32));
            }
        }
    }
    Some(())
}

fn restore_fixed(s: &mut [i32], order: usize) {
    let coef = FIXED_COEF[order];
    for i in order..s.len() {
        let mut pred = 0i64;
        for (j, &c) in coef.iter().enumerate() {
            pred += c as i64 * s[i - 1 - j] as i64;
        }
        s[i] += pred as i32;
    }
}

fn restore_lpc(s: &mut [i32], coefs: &[i32], shift: u32, order: usize) {
    for i in order..s.len() {
        let mut pred = 0i64;
        for (j, &c) in coefs.iter().enumerate() {
            pred += c as i64 * s[i - 1 - j] as i64;
        }
        s[i] += (pred >> shift) as i32;
    }
}

fn read_utf8_number(bits: &mut Bits, _sample: bool) {
    let first = bits.read(8) as u8;
    let extra = if first & 0x80 == 0 {
        0
    } else if first & 0xe0 == 0xc0 {
        1
    } else if first & 0xf0 == 0xe0 {
        2
    } else if first & 0xf8 == 0xf0 {
        3
    } else if first & 0xfc == 0xf8 {
        4
    } else if first & 0xfe == 0xfc {
        5
    } else {
        6
    };
    for _ in 0..extra {
        bits.read(8);
    }
}
