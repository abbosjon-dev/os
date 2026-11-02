// MP3 (MPEG-1 Audio Layer III) dekoderi — noldan yozilgan.
// ISO raqamli jadvallari mp3_tables.rs da; bu yerda butun dekodlaş mantigʼi:
// sarlavha, yon maʼlumot, bit rezervuar, Huffman, qayta kvantlaş, qayta
// tartiblaş, stereo, antialias, IMDCT, polifaz sintez filtri.
//
// Qöllanadi: MPEG-1 Layer III (44100/48000/32000 Hz, mono/stereo, joint).
// Taniş (recognition) barcha versiya/qatlamlar uçun: format_info().

use super::Clip;
use super::mp3_tables as tbl;
use alloc::vec::Vec;

// ---------------- no_std f32/f64 matematika ----------------

const PI: f64 = 3.14159265358979323846;
const INV_SQRT2: f32 = 0.70710678118654752440;

fn sinf(x: f64) -> f64 {
    // -PI..PI ga keltirish
    let t = x * (1.0 / (2.0 * PI));
    let k = if t >= 0.0 { (t + 0.5) as i64 as f64 } else { (t - 0.5) as i64 as f64 };
    let mut x = x - k * 2.0 * PI;
    if x > PI {
        x -= 2.0 * PI;
    } else if x < -PI {
        x += 2.0 * PI;
    }
    // Taylor (x^15 gaça — [-PI,PI] da yetarli aniqlik)
    let x2 = x * x;
    x * (1.0
        - x2 / 6.0
            * (1.0
                - x2 / 20.0
                    * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0 * (1.0 - x2 / 110.0 * (1.0 - x2 / 156.0))))))
}
fn cosf(x: f64) -> f64 {
    sinf(x + PI / 2.0)
}

/// 2^x (f32). xi butun qism uçun eksponentni bevosita quramiz.
fn exp2f(x: f32) -> f32 {
    if x <= -126.0 {
        return 0.0;
    }
    if x >= 127.0 {
        return f32::from_bits(0x7f7fffff); // ~max
    }
    let xi = libm_floor(x);
    let f = x - xi;
    // 2^f, f in [0,1): minimaks koeffitsiyentlar (5-daraja)
    let p = 1.0
        + f * (0.6931471805
            + f * (0.2402265069
                + f * (0.0555041087 + f * (0.0096181291 + f * 0.0013333558))));
    // 2^xi: f32 eksponent maydonini quramiz
    let e = (xi as i32 + 127) as u32;
    let pw = f32::from_bits(e << 23);
    p * pw
}

fn libm_floor(x: f32) -> f32 {
    let t = x as i32 as f32;
    if t > x {
        t - 1.0
    } else {
        t
    }
}

/// log2(x), x>0.
fn log2f(x: f32) -> f32 {
    if x <= 0.0 {
        return -127.0;
    }
    let bits = x.to_bits();
    let e = ((bits >> 23) & 0xff) as i32 - 127;
    let m = f32::from_bits((bits & 0x007fffff) | 0x3f800000); // [1,2)
    // log2(m), m in [1,2): t=(m-1)/(m+1)
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let s = t * (1.0 + t2 * (1.0 / 3.0 + t2 * (1.0 / 5.0 + t2 * (1.0 / 7.0 + t2 / 9.0))));
    e as f32 + s * 2.8853900818 // 2/ln2
}

/// x^(4/3), x>=0 (qayta kvantlaş uçun).
fn pow43(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    exp2f(1.33333333 * log2f(x))
}

// ---------------- bit öqigiç ----------------

struct Bits<'a> {
    d: &'a [u8],
    pos: usize, // bit indeksi
}
impl<'a> Bits<'a> {
    fn new(d: &'a [u8], bit: usize) -> Bits<'a> {
        Bits { d, pos: bit }
    }
    fn get(&mut self, n: usize) -> u32 {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = self.pos >> 3;
            let bit = 7 - (self.pos & 7);
            let b = if byte < self.d.len() { (self.d[byte] >> bit) & 1 } else { 0 };
            v = (v << 1) | b as u32;
            self.pos += 1;
        }
        v
    }
    fn get1(&mut self) -> u32 {
        self.get(1)
    }
}

// ---------------- sarlavha ----------------

#[derive(Clone, Copy, Default)]
struct Header {
    version: u8, // 3=MPEG1, 2=MPEG2, 0=MPEG2.5
    layer: u8,   // 1,2,3
    protection: u8,
    bitrate_idx: u8,
    srate_idx: u8,
    padding: u8,
    mode: u8, // 0=stereo,1=joint,2=dual,3=mono
    mode_ext: u8,
}

const BR_MPEG1_L3: [u32; 15] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
const SR_MPEG1: [u32; 3] = [44100, 48000, 32000];

fn parse_header(h: u32) -> Option<Header> {
    if (h & 0xffe00000) != 0xffe00000 {
        return None;
    }
    let version = match (h >> 19) & 3 {
        3 => 3,
        2 => 2,
        0 => 0,
        _ => return None, // 1 = reserved
    };
    let layer = match (h >> 17) & 3 {
        1 => 3,
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let bitrate_idx = ((h >> 12) & 0xf) as u8;
    let srate_idx = ((h >> 10) & 3) as u8;
    if bitrate_idx == 0 || bitrate_idx == 15 || srate_idx == 3 {
        return None;
    }
    Some(Header {
        version,
        layer,
        protection: ((h >> 16) & 1) as u8,
        bitrate_idx,
        srate_idx,
        padding: ((h >> 9) & 1) as u8,
        mode: ((h >> 6) & 3) as u8,
        mode_ext: ((h >> 4) & 3) as u8,
    })
}

impl Header {
    fn channels(&self) -> usize {
        if self.mode == 3 {
            1
        } else {
            2
        }
    }
    fn sample_rate(&self) -> u32 {
        let base = SR_MPEG1[self.srate_idx as usize];
        match self.version {
            3 => base,
            2 => base / 2,
            _ => base / 4,
        }
    }
    /// Kadr uzunligi (bayt). Faqat Layer III MPEG-1 to'liq dekod qilinadi,
    /// ammo barcha versiya/qatlamlar uçun uzunlik taniş uçun hisoblanadi.
    fn frame_size(&self) -> usize {
        let br = if self.version == 3 && self.layer == 3 {
            BR_MPEG1_L3[self.bitrate_idx as usize] * 1000
        } else {
            // taxminiy (taniş uçun); to'liq jadval kerak emas
            bitrate_any(self.version, self.layer, self.bitrate_idx) * 1000
        };
        let sr = self.sample_rate();
        if sr == 0 {
            return 0;
        }
        match self.layer {
            1 => ((12 * br / sr + self.padding as u32) * 4) as usize,
            _ => {
                let spf = if self.layer == 3 && self.version != 3 { 72 } else { 144 };
                (spf * br / sr + self.padding as u32) as usize
            }
        }
    }
    fn is_mpeg1_l3(&self) -> bool {
        self.version == 3 && self.layer == 3
    }
    fn side_info_size(&self) -> usize {
        match (self.version, self.channels()) {
            (3, 1) => 17,
            (3, _) => 32,
            (_, 1) => 9,
            (_, _) => 17,
        }
    }
}

fn bitrate_any(version: u8, layer: u8, idx: u8) -> u32 {
    // MPEG1
    const L1: [u32; 15] = [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448];
    const L2: [u32; 15] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384];
    // MPEG2/2.5
    const L1B: [u32; 15] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256];
    const L23B: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let i = idx as usize;
    if version == 3 {
        match layer {
            1 => L1[i],
            2 => L2[i],
            _ => BR_MPEG1_L3[i],
        }
    } else {
        match layer {
            1 => L1B[i],
            _ => L23B[i],
        }
    }
}

// ---------------- yon maʼlumot ----------------

#[derive(Clone, Default)]
struct Gran {
    part2_3_length: u32,
    big_values: u32,
    global_gain: u32,
    scalefac_compress: u32,
    win_switch: u32,
    block_type: u32,
    mixed: u32,
    table_select: [u32; 3],
    subblock_gain: [u32; 3],
    region0_count: u32,
    region1_count: u32,
    preflag: u32,
    scalefac_scale: u32,
    count1table: u32,
    count1: usize,
    scalefac_l: [i32; 23],
    scalefac_s: [[i32; 3]; 13],
}

#[derive(Clone, Default)]
struct SideInfo {
    main_data_begin: usize,
    scfsi: [[u32; 4]; 2],
    gran: [[Gran; 2]; 2], // [gr][ch]
}

// ---------------- dekoder holati ----------------

pub struct Decoder {
    store: Vec<f32>, // overlap-add (hibrid sintez): [ch*32*18 + sb*18 + i]
    v_vec: Vec<f32>, // polifaz sintez: [ch*1024 + k]
    nwin: Vec<f32>,  // 64*32 cos jadvali
    imdct36: Vec<f32>, // 36*18
    imdct12: Vec<f32>, // 12*6
    win: [[f32; 36]; 4], // IMDCT oyna
}

impl Decoder {
    pub fn new() -> Decoder {
        let mut nwin = Vec::with_capacity(64 * 32);
        for i in 0..64 {
            for j in 0..32 {
                nwin.push(cosf(((16 + i) as f64 * (2 * j + 1) as f64) * (PI / 64.0)) as f32);
            }
        }
        let mut imdct36 = Vec::with_capacity(36 * 18);
        for p in 0..36 {
            for m in 0..18 {
                imdct36.push(cosf(PI / 72.0 * (2 * p + 1 + 18) as f64 * (2 * m + 1) as f64) as f32);
            }
        }
        let mut imdct12 = Vec::with_capacity(12 * 6);
        for p in 0..12 {
            for m in 0..6 {
                imdct12.push(cosf(PI / 24.0 * (2 * p + 1 + 6) as f64 * (2 * m + 1) as f64) as f32);
            }
        }
        let mut win = [[0.0f32; 36]; 4];
        for i in 0..36 {
            win[0][i] = sinf(PI / 36.0 * (i as f64 + 0.5)) as f32;
        }
        for i in 0..18 {
            win[1][i] = sinf(PI / 36.0 * (i as f64 + 0.5)) as f32;
        }
        for i in 18..24 {
            win[1][i] = 1.0;
        }
        for i in 24..30 {
            win[1][i] = sinf(PI / 12.0 * (i as f64 + 0.5 - 18.0)) as f32;
        }
        for i in 0..12 {
            win[2][i] = sinf(PI / 12.0 * (i as f64 + 0.5)) as f32;
        }
        for i in 6..12 {
            win[3][i] = sinf(PI / 12.0 * (i as f64 + 0.5 - 6.0)) as f32;
        }
        for i in 12..18 {
            win[3][i] = 1.0;
        }
        for i in 18..36 {
            win[3][i] = sinf(PI / 36.0 * (i as f64 + 0.5)) as f32;
        }
        Decoder {
            store: alloc::vec![0.0f32; 2 * 32 * 18],
            v_vec: alloc::vec![0.0f32; 2 * 1024],
            nwin,
            imdct36,
            imdct12,
            win,
        }
    }
}

// ---------------- Huffman ----------------

fn huff_decode(b: &mut Bits, table: usize) -> (i32, i32, i32, i32) {
    let (off, treelen, linbits) = tbl::HUFF_MAIN[table];
    if treelen == 0 {
        return (0, 0, 0, 0);
    }
    let ht = &tbl::HUFF[off as usize..];
    let mut point = 0usize;
    let mut bitsleft = 32;
    let (mut x, mut y) = (0i32, 0i32);
    loop {
        if (ht[point] & 0xff00) == 0 {
            x = ((ht[point] >> 4) & 0xf) as i32;
            y = (ht[point] & 0xf) as i32;
            break;
        }
        if b.get1() == 1 {
            while (ht[point] & 0xff) >= 250 {
                point += (ht[point] & 0xff) as usize;
            }
            point += (ht[point] & 0xff) as usize;
        } else {
            while (ht[point] >> 8) >= 250 {
                point += (ht[point] >> 8) as usize;
            }
            point += (ht[point] >> 8) as usize;
        }
        bitsleft -= 1;
        if bitsleft == 0 || point >= treelen as usize {
            break;
        }
    }
    if table > 31 {
        // quadruples: v w x y — har biri 1 bit, + işora
        let v = (y >> 3) & 1;
        let w = (y >> 2) & 1;
        let xx = (y >> 1) & 1;
        let yy = y & 1;
        let mut v = v;
        let mut w = w;
        let mut xx = xx;
        let mut yy = yy;
        if v > 0 && b.get1() == 1 {
            v = -v;
        }
        if w > 0 && b.get1() == 1 {
            w = -w;
        }
        if xx > 0 && b.get1() == 1 {
            xx = -xx;
        }
        if yy > 0 && b.get1() == 1 {
            yy = -yy;
        }
        (xx, yy, v, w)
    } else {
        if linbits > 0 && x == 15 {
            x += b.get(linbits as usize) as i32;
        }
        if x > 0 && b.get1() == 1 {
            x = -x;
        }
        if linbits > 0 && y == 15 {
            y += b.get(linbits as usize) as i32;
        }
        if y > 0 && b.get1() == 1 {
            y = -y;
        }
        (x, y, 0, 0)
    }
}

fn read_huffman(b: &mut Bits, g: &mut Gran, hdr: &Header, is: &mut [f32; 576], part2_start: usize) {
    if g.part2_3_length == 0 {
        for v in is.iter_mut() {
            *v = 0.0;
        }
        g.count1 = 0;
        return;
    }
    let bit_end = part2_start + g.part2_3_length as usize;
    let sr = hdr.srate_idx as usize;
    let (r1, r2);
    if g.win_switch == 1 && g.block_type == 2 {
        r1 = 36;
        r2 = 576;
    } else {
        r1 = tbl::SFB_LONG[sr][(g.region0_count + 1) as usize] as usize;
        r2 = tbl::SFB_LONG[sr][(g.region0_count + g.region1_count + 2) as usize] as usize;
    }
    let mut is_pos = 0usize;
    let bv = g.big_values as usize * 2;
    while is_pos < bv {
        let table = if is_pos < r1 {
            g.table_select[0]
        } else if is_pos < r2 {
            g.table_select[1]
        } else {
            g.table_select[2]
        } as usize;
        let (x, y, _, _) = huff_decode(b, table);
        is[is_pos] = x as f32;
        is[is_pos + 1] = y as f32;
        is_pos += 2;
    }
    // count1 soha
    let table = (g.count1table + 32) as usize;
    is_pos = g.big_values as usize * 2;
    while is_pos <= 572 && b.pos <= bit_end {
        let (x, y, v, w) = huff_decode(b, table);
        is[is_pos] = v as f32;
        is_pos += 1;
        if is_pos >= 576 {
            break;
        }
        is[is_pos] = w as f32;
        is_pos += 1;
        if is_pos >= 576 {
            break;
        }
        is[is_pos] = x as f32;
        is_pos += 1;
        if is_pos >= 576 {
            break;
        }
        is[is_pos] = y as f32;
        is_pos += 1;
    }
    if b.pos > bit_end + 1 && is_pos >= 4 {
        is_pos -= 4;
    }
    g.count1 = is_pos;
    for v in is.iter_mut().skip(is_pos) {
        *v = 0.0;
    }
    b.pos = bit_end;
}

// ---------------- yon maʼlumotni öqiş ----------------

fn read_side_info(b: &mut Bits, hdr: &Header) -> SideInfo {
    let mut si = SideInfo::default();
    let nch = hdr.channels();
    si.main_data_begin = b.get(9) as usize;
    if nch == 1 {
        b.get(5);
    } else {
        b.get(3);
    }
    for ch in 0..nch {
        for band in 0..4 {
            si.scfsi[ch][band] = b.get1();
        }
    }
    for gr in 0..2 {
        for ch in 0..nch {
            let g = &mut si.gran[gr][ch];
            g.part2_3_length = b.get(12);
            g.big_values = b.get(9);
            g.global_gain = b.get(8);
            g.scalefac_compress = b.get(4);
            g.win_switch = b.get1();
            if g.win_switch == 1 {
                g.block_type = b.get(2);
                g.mixed = b.get1();
                for r in 0..2 {
                    g.table_select[r] = b.get(5);
                }
                for w in 0..3 {
                    g.subblock_gain[w] = b.get(3);
                }
                g.region0_count = if g.block_type == 2 && g.mixed == 0 { 8 } else { 7 };
                g.region1_count = 20 - g.region0_count;
            } else {
                for r in 0..3 {
                    g.table_select[r] = b.get(5);
                }
                g.region0_count = b.get(4);
                g.region1_count = b.get(3);
                g.block_type = 0;
            }
            g.preflag = b.get1();
            g.scalefac_scale = b.get1();
            g.count1table = b.get1();
        }
    }
    si
}

// ---------------- skalyar koeffitsiyentlar + Huffman (main data) ----------------

fn read_main(b: &mut Bits, si: &mut SideInfo, hdr: &Header, is: &mut [[[f32; 576]; 2]; 2]) {
    let nch = hdr.channels();
    for gr in 0..2 {
        for ch in 0..nch {
            let part2_start = b.pos;
            let (slen1, slen2) = {
                let g = &si.gran[gr][ch];
                let s = tbl::SCALEFAC_SIZES[g.scalefac_compress as usize];
                (s.0 as usize, s.1 as usize)
            };
            let win_switch = si.gran[gr][ch].win_switch;
            let block_type = si.gran[gr][ch].block_type;
            let mixed = si.gran[gr][ch].mixed;
            if win_switch != 0 && block_type == 2 {
                if mixed != 0 {
                    for sfb in 0..8 {
                        si.gran[gr][ch].scalefac_l[sfb] = b.get(slen1) as i32;
                    }
                    for sfb in 3..12 {
                        let nbits = if sfb < 6 { slen1 } else { slen2 };
                        for w in 0..3 {
                            si.gran[gr][ch].scalefac_s[sfb][w] = b.get(nbits) as i32;
                        }
                    }
                } else {
                    for sfb in 0..12 {
                        let nbits = if sfb < 6 { slen1 } else { slen2 };
                        for w in 0..3 {
                            si.gran[gr][ch].scalefac_s[sfb][w] = b.get(nbits) as i32;
                        }
                    }
                }
            } else {
                // uzun bloklar; scfsi orqali granula 0 dan nusxa
                let bands: [(usize, usize, usize); 4] =
                    [(0, 6, slen1), (6, 11, slen1), (11, 16, slen2), (16, 21, slen2)];
                for (bi, &(lo, hi, nbits)) in bands.iter().enumerate() {
                    if si.scfsi[ch][bi] == 0 || gr == 0 {
                        for sfb in lo..hi {
                            si.gran[gr][ch].scalefac_l[sfb] = b.get(nbits) as i32;
                        }
                    } else {
                        for sfb in lo..hi {
                            si.gran[1][ch].scalefac_l[sfb] = si.gran[0][ch].scalefac_l[sfb];
                        }
                    }
                }
            }
            let mut g = si.gran[gr][ch].clone();
            read_huffman(b, &mut g, hdr, &mut is[gr][ch], part2_start);
            si.gran[gr][ch] = g;
        }
    }
}

// ---------------- qayta kvantlaş ----------------

fn requantize(g: &Gran, hdr: &Header, is: &mut [f32; 576]) {
    let sr = hdr.srate_idx as usize;
    let sf_mult = if g.scalefac_scale != 0 { 1.0f32 } else { 0.5f32 };
    let gg = exp2f(0.25 * (g.global_gain as f32 - 210.0));
    let req_long = |is_pos: usize, sfb: usize, is: &mut [f32; 576]| {
        let pf = g.preflag as f32 * tbl::PRETAB[sfb];
        let t1 = exp2f(-(sf_mult * (g.scalefac_l[sfb] as f32 + pf)));
        let v = is[is_pos];
        let t3 = if v < 0.0 { -pow43(-v) } else { pow43(v) };
        is[is_pos] = t1 * gg * t3;
    };
    let req_short = |is_pos: usize, sfb: usize, win: usize, is: &mut [f32; 576]| {
        let t1 = exp2f(-sf_mult * g.scalefac_s[sfb][win] as f32);
        let t2 = exp2f(0.25 * (g.global_gain as f32 - 210.0 - 8.0 * g.subblock_gain[win] as f32));
        let v = is[is_pos];
        let t3 = if v < 0.0 { -pow43(-v) } else { pow43(v) };
        is[is_pos] = t1 * t2 * t3;
    };
    if g.win_switch == 1 && g.block_type == 2 {
        if g.mixed != 0 {
            // dastlabki 2 subband uzun
            let mut sfb = 0usize;
            let mut next = tbl::SFB_LONG[sr][sfb + 1] as usize;
            for i in 0..36 {
                if i == next {
                    sfb += 1;
                    next = tbl::SFB_LONG[sr][sfb + 1] as usize;
                }
                req_long(i, sfb, is);
            }
            let mut sfb = 3usize;
            let mut next = tbl::SFB_SHORT[sr][sfb + 1] as usize * 3;
            let mut wl = (tbl::SFB_SHORT[sr][sfb + 1] - tbl::SFB_SHORT[sr][sfb]) as usize;
            let mut i = 36usize;
            while i < g.count1 {
                if i == next {
                    sfb += 1;
                    next = tbl::SFB_SHORT[sr][sfb + 1] as usize * 3;
                    wl = (tbl::SFB_SHORT[sr][sfb + 1] - tbl::SFB_SHORT[sr][sfb]) as usize;
                }
                for win in 0..3 {
                    for _ in 0..wl {
                        if i >= g.count1 {
                            break;
                        }
                        req_short(i, sfb, win, is);
                        i += 1;
                    }
                }
            }
        } else {
            let mut sfb = 0usize;
            let mut next = tbl::SFB_SHORT[sr][sfb + 1] as usize * 3;
            let mut wl = (tbl::SFB_SHORT[sr][sfb + 1] - tbl::SFB_SHORT[sr][sfb]) as usize;
            let mut i = 0usize;
            while i < g.count1 {
                if i == next {
                    sfb += 1;
                    next = tbl::SFB_SHORT[sr][sfb + 1] as usize * 3;
                    wl = (tbl::SFB_SHORT[sr][sfb + 1] - tbl::SFB_SHORT[sr][sfb]) as usize;
                }
                for win in 0..3 {
                    for _ in 0..wl {
                        if i >= g.count1 {
                            break;
                        }
                        req_short(i, sfb, win, is);
                        i += 1;
                    }
                }
            }
        }
    } else {
        let mut sfb = 0usize;
        let mut next = tbl::SFB_LONG[sr][sfb + 1] as usize;
        for i in 0..g.count1 {
            if i == next {
                sfb += 1;
                next = tbl::SFB_LONG[sr][sfb + 1] as usize;
            }
            req_long(i, sfb, is);
        }
    }
}

// ---------------- qayta tartiblaş (qisqa bloklar) ----------------

fn reorder(g: &Gran, hdr: &Header, is: &mut [f32; 576]) {
    if !(g.win_switch == 1 && g.block_type == 2) {
        return;
    }
    let sr = hdr.srate_idx as usize;
    let mut re = [0.0f32; 576];
    let mut sfb = if g.mixed != 0 { 3usize } else { 0 };
    let mut next = tbl::SFB_SHORT[sr][sfb + 1] as usize * 3;
    let mut wl = (tbl::SFB_SHORT[sr][sfb + 1] - tbl::SFB_SHORT[sr][sfb]) as usize;
    let mut i = if sfb == 0 { 0usize } else { 36 };
    loop {
        if i == next {
            for j in 0..3 * wl {
                is[3 * tbl::SFB_SHORT[sr][sfb] as usize + j] = re[j];
            }
            if i >= g.count1 {
                return;
            }
            sfb += 1;
            next = tbl::SFB_SHORT[sr][sfb + 1] as usize * 3;
            wl = (tbl::SFB_SHORT[sr][sfb + 1] - tbl::SFB_SHORT[sr][sfb]) as usize;
        }
        for win in 0..3 {
            for j in 0..wl {
                re[j * 3 + win] = is[i];
                i += 1;
            }
        }
        if i >= 576 {
            break;
        }
    }
    for j in 0..3 * wl {
        is[3 * tbl::SFB_SHORT[sr][12] as usize + j] = re[j];
    }
}

// ---------------- stereo ----------------

fn stereo(si: &SideInfo, hdr: &Header, isl: &mut [f32; 576], isr: &mut [f32; 576], gr: usize) {
    if hdr.mode != 1 || hdr.mode_ext == 0 {
        return;
    }
    let sr = hdr.srate_idx as usize;
    let c0 = si.gran[gr][0].count1;
    let c1 = si.gran[gr][1].count1;
    // MS stereo
    if hdr.mode_ext & 0x2 != 0 {
        let max_pos = c0.max(c1);
        for i in 0..max_pos {
            let l = (isl[i] + isr[i]) * INV_SQRT2;
            let r = (isl[i] - isr[i]) * INV_SQRT2;
            isl[i] = l;
            isr[i] = r;
        }
    }
    // Intensity stereo
    if hdr.mode_ext & 0x1 != 0 {
        let g0 = &si.gran[gr][0];
        let do_long = |sfb: usize, isl: &mut [f32; 576], isr: &mut [f32; 576]| {
            let is_pos = g0.scalefac_l[sfb];
            if is_pos != 7 {
                let start = tbl::SFB_LONG[sr][sfb] as usize;
                let stop = tbl::SFB_LONG[sr][sfb + 1] as usize;
                let (rl, rr) = is_ratio(is_pos);
                for i in start..stop {
                    let v = isl[i];
                    isl[i] = rl * v;
                    isr[i] = rr * v;
                }
            }
        };
        let do_short = |sfb: usize, isl: &mut [f32; 576], isr: &mut [f32; 576]| {
            let wl = (tbl::SFB_SHORT[sr][sfb + 1] - tbl::SFB_SHORT[sr][sfb]) as usize;
            for win in 0..3 {
                let is_pos = g0.scalefac_s[sfb][win];
                if is_pos != 7 {
                    let start = tbl::SFB_SHORT[sr][sfb] as usize * 3 + wl * win;
                    let stop = start + wl;
                    let (rl, rr) = is_ratio(is_pos);
                    for i in start..stop {
                        let v = isl[i];
                        isl[i] = rl * v;
                        isr[i] = rr * v;
                    }
                }
            }
        };
        if g0.win_switch == 1 && g0.block_type == 2 {
            if g0.mixed != 0 {
                for sfb in 0..8 {
                    if tbl::SFB_LONG[sr][sfb] as usize >= c1 {
                        do_long(sfb, isl, isr);
                    }
                }
                for sfb in 3..12 {
                    if tbl::SFB_SHORT[sr][sfb] as usize * 3 >= c1 {
                        do_short(sfb, isl, isr);
                    }
                }
            } else {
                for sfb in 0..12 {
                    if tbl::SFB_SHORT[sr][sfb] as usize * 3 >= c1 {
                        do_short(sfb, isl, isr);
                    }
                }
            }
        } else {
            for sfb in 0..21 {
                if tbl::SFB_LONG[sr][sfb] as usize >= c1 {
                    do_long(sfb, isl, isr);
                }
            }
        }
    }
}

fn is_ratio(is_pos: i32) -> (f32, f32) {
    if is_pos == 6 {
        (1.0, 0.0)
    } else {
        let r = tbl::IS_RATIOS[is_pos as usize];
        (r / (1.0 + r), 1.0 / (1.0 + r))
    }
}

// ---------------- antialias ----------------

fn antialias(g: &Gran, is: &mut [f32; 576]) {
    if g.win_switch == 1 && g.block_type == 2 && g.mixed == 0 {
        return;
    }
    let sblim = if g.win_switch == 1 && g.block_type == 2 && g.mixed == 1 { 2 } else { 32 };
    for sb in 1..sblim {
        for i in 0..8 {
            let li = 18 * sb - 1 - i;
            let ui = 18 * sb + i;
            let lb = is[li] * tbl::CS[i] - is[ui] * tbl::CA[i];
            let ub = is[ui] * tbl::CS[i] + is[li] * tbl::CA[i];
            is[li] = lb;
            is[ui] = ub;
        }
    }
}

// ---------------- IMDCT + oyna ----------------

fn imdct_win(dec: &Decoder, input: &[f32], out: &mut [f32; 36], block_type: usize) {
    for v in out.iter_mut() {
        *v = 0.0;
    }
    if block_type == 2 {
        for i in 0..3 {
            for p in 0..12 {
                let mut sum = 0.0f32;
                for m in 0..6 {
                    sum += input[i + 3 * m] * dec.imdct12[p * 6 + m];
                }
                out[6 * i + p + 6] += sum * dec.win[2][p];
            }
        }
    } else {
        for p in 0..36 {
            let mut sum = 0.0f32;
            for m in 0..18 {
                sum += input[m] * dec.imdct36[p * 18 + m];
            }
            out[p] = sum * dec.win[block_type][p];
        }
    }
}

fn hybrid_synthesis(dec: &mut Decoder, g: &Gran, ch: usize, is: &mut [f32; 576]) {
    let mut rawout = [0.0f32; 36];
    for sb in 0..32 {
        let bt = if g.win_switch == 1 && g.mixed == 1 && sb < 2 {
            0
        } else {
            g.block_type as usize
        };
        let mut input = [0.0f32; 18];
        input.copy_from_slice(&is[sb * 18..sb * 18 + 18]);
        imdct_win(dec, &input, &mut rawout, bt);
        let base = ch * 32 * 18 + sb * 18;
        for i in 0..18 {
            is[sb * 18 + i] = rawout[i] + dec.store[base + i];
            dec.store[base + i] = rawout[i + 18];
        }
    }
}

fn frequency_inversion(is: &mut [f32; 576]) {
    let mut sb = 1;
    while sb < 32 {
        let mut i = 1;
        while i < 18 {
            is[sb * 18 + i] = -is[sb * 18 + i];
            i += 2;
        }
        sb += 2;
    }
}

fn subband_synthesis(dec: &mut Decoder, ch: usize, is: &[f32; 576], out: &mut [i16]) {
    let mut u = [0.0f32; 512];
    let mut s = [0.0f32; 32];
    for ss in 0..18 {
        // V vektorini siljitamiz
        let v = &mut dec.v_vec[ch * 1024..ch * 1024 + 1024];
        let mut i = 1023;
        while i > 63 {
            v[i] = v[i - 64];
            i -= 1;
        }
        for i in 0..32 {
            s[i] = is[i * 18 + ss];
        }
        for i in 0..64 {
            let mut sum = 0.0f32;
            for j in 0..32 {
                sum += dec.nwin[i * 32 + j] * s[j];
            }
            v[i] = sum;
        }
        for i in 0..8 {
            for j in 0..32 {
                u[(i << 6) + j] = v[(i << 7) + j];
                u[(i << 6) + j + 32] = v[(i << 7) + j + 96];
            }
        }
        for i in 0..512 {
            u[i] *= tbl::DSYNTH[i];
        }
        for i in 0..32 {
            let mut sum = 0.0f32;
            for j in 0..16 {
                sum += u[(j << 5) + i];
            }
            let samp = (sum * 32767.0) as i32;
            let samp = samp.clamp(-32767, 32767) as i16;
            out[32 * ss + i] = samp;
        }
    }
}

// ---------------- yuqori dekod ----------------

/// MP3 baytlaridan to'liq PCM chiqaradi (MPEG-1 Layer III).
/// Qaytadi: (chastota, kanallar, interleaved i16 namunalar).
pub fn decode_pcm(d: &[u8]) -> Option<(u32, u16, Vec<i16>)> {
    let mut pos = skip_id3(d);
    let mut dec = Decoder::new();
    let mut reservoir: Vec<u8> = Vec::new();
    let mut out: Vec<i16> = Vec::new();
    let mut rate = 0u32;
    let mut channels = 0u16;
    let mut frames = 0usize;

    while pos + 4 <= d.len() {
        let h = (d[pos] as u32) << 24
            | (d[pos + 1] as u32) << 16
            | (d[pos + 2] as u32) << 8
            | d[pos + 3] as u32;
        let hdr = match parse_header(h) {
            Some(h) => h,
            None => {
                pos += 1;
                continue;
            }
        };
        let fsize = hdr.frame_size();
        if fsize < 4 || pos + fsize > d.len() {
            // oxirgi/no'to'g'ri kadr
            if fsize < 4 {
                pos += 1;
                continue;
            }
            break;
        }
        if !hdr.is_mpeg1_l3() {
            // boshqa versiya/qatlam: dekod qilinmaydi, o'tkazamiz
            pos += fsize;
            continue;
        }
        // Xing/Info/VBRI boshi — audio emas, o'tkazamiz
        let si_size = hdr.side_info_size();
        if is_xing(&d[pos..pos + fsize], si_size) {
            pos += fsize;
            continue;
        }
        rate = hdr.sample_rate();
        channels = hdr.channels() as u16;
        // main_data o'rni
        let crc = if hdr.protection == 0 { 2 } else { 0 };
        let md_off = pos + 4 + crc + si_size;
        let md_size = fsize.saturating_sub(4 + crc + si_size);
        if md_off + md_size > d.len() {
            break;
        }
        // yon maʼlumot
        let mut sb = Bits::new(&d[pos + 4 + crc..pos + 4 + crc + si_size], 0);
        let mut si = read_side_info(&mut sb, &hdr);

        // bit rezervuar: oldingi baytlardan main_data_begin qadar orqaga
        if reservoir.len() > 600 {
            let drop = reservoir.len() - 600;
            reservoir.drain(0..drop);
        }
        if si.main_data_begin > reservoir.len() {
            // yetarli rezervuar yo'q (oqim boshida) — kadrni o'tkazamiz
            reservoir.extend_from_slice(&d[md_off..md_off + md_size]);
            pos += fsize;
            continue;
        }
        let md_start = reservoir.len() - si.main_data_begin;
        reservoir.extend_from_slice(&d[md_off..md_off + md_size]);

        let mut is = [[[0.0f32; 576]; 2]; 2];
        {
            let mut mb = Bits::new(&reservoir, md_start * 8);
            read_main(&mut mb, &mut si, &hdr, &mut is);
        }
        // har granula uçun DSP
        let nch = hdr.channels();
        for gr in 0..2 {
            for ch in 0..nch {
                let g = si.gran[gr][ch].clone();
                requantize(&g, &hdr, &mut is[gr][ch]);
                reorder(&g, &hdr, &mut is[gr][ch]);
            }
            if nch == 2 {
                let (a, b) = is[gr].split_at_mut(1);
                stereo(&si, &hdr, &mut a[0], &mut b[0], gr);
            }
            let mut pcm = [[0i16; 576]; 2];
            for ch in 0..nch {
                let g = si.gran[gr][ch].clone();
                antialias(&g, &mut is[gr][ch]);
                hybrid_synthesis(&mut dec, &g, ch, &mut is[gr][ch]);
                frequency_inversion(&mut is[gr][ch]);
                let is_ch = is[gr][ch];
                subband_synthesis(&mut dec, ch, &is_ch, &mut pcm[ch]);
            }
            for i in 0..576 {
                if nch == 1 {
                    out.push(pcm[0][i]);
                } else {
                    out.push(pcm[0][i]);
                    out.push(pcm[1][i]);
                }
            }
        }
        frames += 1;
        pos += fsize;
        if frames > 200_000 {
            break;
        }
    }
    if out.is_empty() || rate == 0 {
        return None;
    }
    Some((rate, channels, out))
}

/// ID3v2 sarlavhasini o'tkazadi (bor bo'lsa).
fn skip_id3(d: &[u8]) -> usize {
    if d.len() >= 10 && &d[..3] == b"ID3" {
        let size = ((d[6] as usize) << 21)
            | ((d[7] as usize) << 14)
            | ((d[8] as usize) << 7)
            | (d[9] as usize);
        let foot = if d[5] & 0x10 != 0 { 10 } else { 0 };
        10 + size + foot
    } else {
        0
    }
}

/// Kadr ichida Xing/Info/VBRI boshi bormi (audio emas)?
fn is_xing(frame: &[u8], si_size: usize) -> bool {
    let o = 4 + si_size;
    if o + 4 <= frame.len() {
        let tag = &frame[o..o + 4];
        if tag == b"Xing" || tag == b"Info" {
            return true;
        }
    }
    if frame.len() >= 40 && &frame[36..40] == b"VBRI" {
        return true;
    }
    false
}

// ---------------- taniş (recognition) ----------------

pub struct Mp3Info {
    pub version: &'static str,
    pub layer: u8,
    pub rate: u32,
    pub channels: u16,
    pub bitrate_kbps: u32,
    pub frames: usize,
    pub duration_ms: u32,
}

/// MP3/MPEG-audio faylni taniydi va metama'lumot chiqaradi (dekodsiz).
pub fn format_info(d: &[u8]) -> Option<Mp3Info> {
    let mut pos = skip_id3(d);
    // birinchi yaroqli kadrni qidiramiz
    while pos + 4 <= d.len() {
        let h = (d[pos] as u32) << 24
            | (d[pos + 1] as u32) << 16
            | (d[pos + 2] as u32) << 8
            | d[pos + 3] as u32;
        if let Some(hdr) = parse_header(h) {
            let fsize = hdr.frame_size();
            if fsize >= 4 && pos + fsize <= d.len() {
                // kadrlarni sanaymiz
                let mut frames = 0usize;
                let mut p = pos;
                let spf = samples_per_frame(&hdr);
                let mut total_samples = 0u64;
                while p + 4 <= d.len() {
                    let hh = (d[p] as u32) << 24
                        | (d[p + 1] as u32) << 16
                        | (d[p + 2] as u32) << 8
                        | d[p + 3] as u32;
                    match parse_header(hh) {
                        Some(fh) => {
                            let fs = fh.frame_size();
                            if fs < 4 || p + fs > d.len() {
                                break;
                            }
                            if !is_xing(&d[p..p + fs], fh.side_info_size()) {
                                frames += 1;
                                total_samples += samples_per_frame(&fh) as u64;
                            }
                            p += fs;
                        }
                        None => p += 1,
                    }
                }
                let rate = hdr.sample_rate();
                let dur = if rate > 0 { (total_samples * 1000 / rate as u64) as u32 } else { 0 };
                let _ = spf;
                return Some(Mp3Info {
                    version: match hdr.version {
                        3 => "MPEG-1",
                        2 => "MPEG-2",
                        _ => "MPEG-2.5",
                    },
                    layer: hdr.layer,
                    rate,
                    channels: hdr.channels() as u16,
                    bitrate_kbps: bitrate_any(hdr.version, hdr.layer, hdr.bitrate_idx),
                    frames,
                    duration_ms: dur,
                });
            }
        }
        pos += 1;
        if pos > 200_000 {
            break;
        }
    }
    None
}

fn samples_per_frame(h: &Header) -> u32 {
    match h.layer {
        1 => 384,
        2 => 1152,
        _ => {
            if h.version == 3 {
                1152
            } else {
                576
            }
        }
    }
}

/// MP3 mi? (sarlavha sinxronini topadi).
pub fn is_mp3(d: &[u8]) -> bool {
    format_info(d).is_some()
}

/// Clip ga dekodlaydi (kernel API).
pub fn decode(d: &[u8]) -> Option<Clip> {
    let (rate, channels, samples) = decode_pcm(d)?;
    Some(Clip { rate, channels, samples })
}
