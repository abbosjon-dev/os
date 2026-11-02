//! DEFLATE (RFC 1951) ni ochiş — noldan. PNG (zlib) va GZIP uçun poydevor.

use alloc::vec::Vec;

const MAXBITS: usize = 15;

struct BitReader<'a> {
    data: &'a [u8],
    byte: usize,
    bit: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, byte: 0, bit: 0 }
    }
    #[inline]
    fn get_bit(&mut self) -> u32 {
        if self.byte >= self.data.len() {
            return 0;
        }
        let b = (self.data[self.byte] >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.byte += 1;
        }
        b as u32
    }
    /// LSB-first `n` bit.
    fn get_bits(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for i in 0..n {
            v |= self.get_bit() << i;
        }
        v
    }
    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.byte += 1;
        }
    }
}

/// Kanonik Huffman jadvali (puff.c uslubi).
struct Huff {
    count: [u16; MAXBITS + 1],
    symbol: Vec<u16>,
}

impl Huff {
    fn build(lengths: &[u8]) -> Huff {
        let mut count = [0u16; MAXBITS + 1];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        // har uzunlik uçun boşlangʼiç ofset
        let mut offs = [0u16; MAXBITS + 2];
        for len in 1..=MAXBITS {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = alloc::vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Huff { count, symbol }
    }

    fn decode(&self, br: &mut BitReader) -> i32 {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..=MAXBITS {
            code |= br.get_bit() as i32;
            let count = self.count[len] as i32;
            if code - first < count {
                return self.symbol[(index + code - first) as usize] as i32;
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        -1
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

fn fixed_tables() -> (Huff, Huff) {
    let mut lit = [0u8; 288];
    for (i, l) in lit.iter_mut().enumerate() {
        *l = if i < 144 {
            8
        } else if i < 256 {
            9
        } else if i < 280 {
            7
        } else {
            8
        };
    }
    let dist = [5u8; 30];
    (Huff::build(&lit), Huff::build(&dist))
}

fn inflate_block(br: &mut BitReader, out: &mut Vec<u8>, lit: &Huff, dist: &Huff, limit: usize) -> bool {
    loop {
        let sym = lit.decode(br);
        if sym < 0 {
            return false;
        }
        if sym == 256 {
            return true; // blok oxiri
        }
        if sym < 256 {
            out.push(sym as u8);
        } else {
            let s = (sym - 257) as usize;
            if s >= 29 {
                return false;
            }
            let len = LEN_BASE[s] as usize + br.get_bits(LEN_EXTRA[s] as u32) as usize;
            let dsym = dist.decode(br);
            if dsym < 0 || dsym as usize >= 30 {
                return false;
            }
            let ds = dsym as usize;
            let d = DIST_BASE[ds] as usize + br.get_bits(DIST_EXTRA[ds] as u32) as usize;
            if d == 0 || d > out.len() {
                return false;
            }
            let start = out.len() - d;
            for i in 0..len {
                out.push(out[start + i]);
            }
        }
        if out.len() > limit {
            return false; // himoya: juda katta
        }
    }
}

/// Xom DEFLATE oqimini ochadi.
pub fn inflate(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut br = BitReader::new(data);
    let mut out = Vec::new();
    loop {
        let last = br.get_bit();
        let btype = br.get_bits(2);
        match btype {
            0 => {
                // saqlangan (siqilmagan)
                br.align();
                if br.byte + 4 > data.len() {
                    return None;
                }
                let len = data[br.byte] as usize | (data[br.byte + 1] as usize) << 8;
                br.byte += 4; // len + nlen
                if br.byte + len > data.len() {
                    return None;
                }
                out.extend_from_slice(&data[br.byte..br.byte + len]);
                br.byte += len;
            }
            1 => {
                let (lit, dist) = fixed_tables();
                if !inflate_block(&mut br, &mut out, &lit, &dist, limit) {
                    return None;
                }
            }
            2 => {
                let hlit = br.get_bits(5) as usize + 257;
                let hdist = br.get_bits(5) as usize + 1;
                let hclen = br.get_bits(4) as usize + 4;
                const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl_lens = [0u8; 19];
                for i in 0..hclen {
                    cl_lens[ORDER[i]] = br.get_bits(3) as u8;
                }
                let cl_huff = Huff::build(&cl_lens);
                let mut lengths = alloc::vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < lengths.len() {
                    let sym = cl_huff.decode(&mut br);
                    if sym < 0 {
                        return None;
                    }
                    match sym {
                        0..=15 => {
                            lengths[i] = sym as u8;
                            i += 1;
                        }
                        16 => {
                            if i == 0 {
                                return None;
                            }
                            let prev = lengths[i - 1];
                            let rep = br.get_bits(2) as usize + 3;
                            for _ in 0..rep {
                                if i < lengths.len() {
                                    lengths[i] = prev;
                                    i += 1;
                                }
                            }
                        }
                        17 => {
                            let rep = br.get_bits(3) as usize + 3;
                            i += rep;
                        }
                        18 => {
                            let rep = br.get_bits(7) as usize + 11;
                            i += rep;
                        }
                        _ => return None,
                    }
                }
                if i != lengths.len() {
                    return None;
                }
                let lit = Huff::build(&lengths[..hlit]);
                let dist = Huff::build(&lengths[hlit..]);
                if !inflate_block(&mut br, &mut out, &lit, &dist, limit) {
                    return None;
                }
            }
            _ => return None,
        }
        if last == 1 {
            break;
        }
        if out.len() > limit {
            return None;
        }
    }
    Some(out)
}

/// zlib oqimi (RFC 1950): 2 baytli sarlavhani ötkazib, DEFLATE ni ochadi.
pub fn zlib(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    if data.len() < 2 {
        return None;
    }
    inflate(&data[2..], limit)
}
