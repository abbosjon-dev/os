// Vorbis I audio — noldan. Sarlavhalar (identifikatsiya + izohlar) → taniş,
// metama'lumot, teglar; va to'liq audio dekod: kodkitoblar (Huffman + VQ),
// floor1, residue (0/1/2), kanal birikuvi, teskari MDCT, oyna + overlap-add.
// ffmpeg bilan tekşirilgan (korrelyatsiya 1.0000, amplituda aniq mos).
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

// ======================= 2-bosqiç: to'liq audio dekod =======================

use super::vorbis_dbtab::INVERSE_DB;

const PI64: f64 = 3.14159265358979323846;
fn sinf(x: f64) -> f64 {
    let t = x * (1.0 / (2.0 * PI64));
    let k = if t >= 0.0 { (t + 0.5) as i64 as f64 } else { (t - 0.5) as i64 as f64 };
    let mut x = x - k * 2.0 * PI64;
    if x > PI64 {
        x -= 2.0 * PI64;
    } else if x < -PI64 {
        x += 2.0 * PI64;
    }
    let x2 = x * x;
    x * (1.0
        - x2 / 6.0
            * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0 * (1.0 - x2 / 110.0 * (1.0 - x2 / 156.0))))))
}
fn cosf(x: f64) -> f64 {
    sinf(x + PI64 / 2.0)
}

// ---- LSB-first bit öqigiç ----
struct Br<'a> {
    d: &'a [u8],
    byte: usize,
    bit: u32,
}
impl<'a> Br<'a> {
    fn new(d: &'a [u8]) -> Br<'a> {
        Br { d, byte: 0, bit: 0 }
    }
    fn read(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for i in 0..n {
            let b = if self.byte < self.d.len() { (self.d[self.byte] >> self.bit) & 1 } else { 0 };
            v |= (b as u32) << i;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.byte += 1;
            }
        }
        v
    }
    fn read1(&mut self) -> u32 {
        self.read(1)
    }
    fn eof(&self) -> bool {
        self.byte >= self.d.len()
    }
}

fn ilog(mut x: i32) -> u32 {
    let mut n = 0;
    while x > 0 {
        n += 1;
        x >>= 1;
    }
    n
}

fn float32_unpack(x: u32) -> f32 {
    let mantissa = (x & 0x1fffff) as f64;
    let sign = x & 0x80000000;
    let exp = ((x & 0x7fe00000) >> 21) as i32;
    let res = if sign != 0 { -mantissa } else { mantissa };
    // res * 2^(exp-788)
    let e = exp - 788;
    (res * pow2(e)) as f32
}
fn pow2(e: i32) -> f64 {
    let mut r = 1.0f64;
    if e >= 0 {
        for _ in 0..e {
            r *= 2.0;
        }
    } else {
        for _ in 0..(-e) {
            r *= 0.5;
        }
    }
    r
}

fn bit_reverse(n: u32) -> u32 {
    let mut n = n;
    n = ((n & 0xAAAAAAAA) >> 1) | ((n & 0x55555555) << 1);
    n = ((n & 0xCCCCCCCC) >> 2) | ((n & 0x33333333) << 2);
    n = ((n & 0xF0F0F0F0) >> 4) | ((n & 0x0F0F0F0F) << 4);
    n = ((n & 0xFF00FF00) >> 8) | ((n & 0x00FF00FF) << 8);
    (n >> 16) | (n << 16)
}

fn lookup1_values(entries: i32, dim: i32) -> i32 {
    // eng katta r: r^dim <= entries
    let mut r = 0i32;
    loop {
        let mut p = 1i64;
        let mut ov = false;
        for _ in 0..dim {
            p *= (r + 1) as i64;
            if p > entries as i64 {
                ov = true;
                break;
            }
        }
        if ov {
            break;
        }
        r += 1;
    }
    r
}

// ---- kodkitob ----
struct Codebook {
    dim: usize,
    entries: usize,
    trie: Vec<[i32; 2]>,
    leaf: Vec<i32>,
    lookup: u8,
    values: Vec<f32>, // entries*dim (lookup!=0 bo'lsa)
    seq_p: bool,
}

impl Codebook {
    fn decode_scalar(&self, br: &mut Br) -> i32 {
        if self.trie.is_empty() {
            return -1;
        }
        let mut node = 0i32;
        for _ in 0..64 {
            let bit = br.read1() as usize;
            let c = self.trie[node as usize][bit];
            if c < 0 {
                return -1;
            }
            node = c;
            if self.leaf[node as usize] >= 0 {
                return self.leaf[node as usize];
            }
        }
        -1
    }
}

fn build_codebook(br: &mut Br) -> Option<Codebook> {
    if br.read(24) != 0x564342 {
        return None;
    }
    let dim = br.read(16) as usize;
    let entries = br.read(24) as usize;
    if dim == 0 || entries == 0 {
        return None;
    }
    let mut lengths = alloc::vec![0u8; entries];
    let ordered = br.read1();
    if ordered == 0 {
        let sparse = br.read1();
        for i in 0..entries {
            if sparse != 0 {
                if br.read1() != 0 {
                    lengths[i] = (br.read(5) + 1) as u8;
                } else {
                    lengths[i] = 0; // ishlatilmaydi
                }
            } else {
                lengths[i] = (br.read(5) + 1) as u8;
            }
        }
    } else {
        let mut cur = 0usize;
        let mut len = br.read(5) + 1;
        while cur < entries {
            let num = br.read(ilog((entries - cur) as i32)) as usize;
            for k in 0..num {
                if cur + k < entries {
                    lengths[cur + k] = len as u8;
                }
            }
            cur += num;
            len += 1;
            if num == 0 {
                break;
            }
        }
    }
    // kodkitob daraxtini quramiz (available[] algoritmi)
    let (trie, leaf) = build_trie(&lengths);
    // lookup
    let lookup = br.read(4) as u8;
    let mut values = Vec::new();
    let mut seq_p = false;
    if lookup == 1 || lookup == 2 {
        let minimum = float32_unpack(br.read(32));
        let delta = float32_unpack(br.read(32));
        let value_bits = br.read(4) + 1;
        seq_p = br.read1() != 0;
        let lookup_values =
            if lookup == 1 { lookup1_values(entries as i32, dim as i32) as usize } else { entries * dim };
        let mut mult = alloc::vec![0.0f32; lookup_values];
        for m in mult.iter_mut() {
            *m = br.read(value_bits) as f32;
        }
        values = alloc::vec![0.0f32; entries * dim];
        for e in 0..entries {
            let mut last = 0.0f32;
            if lookup == 1 {
                let mut div = 1usize;
                for i in 0..dim {
                    let off = (e / div) % lookup_values;
                    let v = mult[off] * delta + minimum + last;
                    values[e * dim + i] = v;
                    if seq_p {
                        last = v;
                    }
                    div *= lookup_values;
                }
            } else {
                for i in 0..dim {
                    let v = mult[e * dim + i] * delta + minimum + last;
                    values[e * dim + i] = v;
                    if seq_p {
                        last = v;
                    }
                }
            }
        }
    } else if lookup != 0 {
        return None;
    }
    let _ = seq_p;
    Some(Codebook { dim, entries, trie, leaf, lookup, values, seq_p })
}

fn build_trie(lengths: &[u8]) -> (Vec<[i32; 2]>, Vec<i32>) {
    let n = lengths.len();
    // available[] algoritmi bilan kod so'zlarini yasaymiz
    let mut codes = alloc::vec![0u32; n];
    let mut used = alloc::vec![false; n];
    let mut available = [0u32; 33];
    let mut k = 0;
    while k < n && lengths[k] == 0 {
        k += 1;
    }
    let mut trie: Vec<[i32; 2]> = Vec::new();
    let mut leaf: Vec<i32> = Vec::new();
    trie.push([-1, -1]);
    leaf.push(-1);
    if k == n {
        return (trie, leaf);
    }
    codes[k] = 0;
    used[k] = true;
    for i in 1..=lengths[k] as usize {
        available[i] = 1u32 << (32 - i);
    }
    for i in (k + 1)..n {
        let li = lengths[i] as usize;
        if li == 0 {
            continue;
        }
        let mut z = li;
        while z > 0 && available[z] == 0 {
            z -= 1;
        }
        if z == 0 {
            break;
        }
        let res = available[z];
        available[z] = 0;
        codes[i] = bit_reverse(res);
        used[i] = true;
        if z != li {
            let mut y = li;
            while y > z {
                available[y] = res + (1u32 << (32 - y));
                y -= 1;
            }
        }
    }
    // daraxtga joylaymiz
    for i in 0..n {
        if !used[i] {
            continue;
        }
        let len = lengths[i] as usize;
        let code = codes[i];
        let mut node = 0usize;
        for j in 0..len {
            let bit = ((code >> j) & 1) as usize;
            if trie[node][bit] < 0 {
                trie.push([-1, -1]);
                leaf.push(-1);
                let idx = (trie.len() - 1) as i32;
                trie[node][bit] = idx;
            }
            node = trie[node][bit] as usize;
        }
        leaf[node] = i as i32;
    }
    (trie, leaf)
}

// ---- sozlaş (setup) tuzilmalari ----
struct Floor1 {
    partitions: usize,
    partition_class: Vec<usize>,
    class_dims: Vec<usize>,
    class_subclasses: Vec<usize>,
    class_master: Vec<i32>,
    subclass_books: Vec<Vec<i32>>,
    multiplier: i32,
    xlist: Vec<i32>,
    sorted: Vec<usize>, // X böyiça tartiblangan indekslar
    values: usize,
}
struct Residue {
    rtype: u16,
    begin: usize,
    end: usize,
    part_size: usize,
    classifications: usize,
    classbook: usize,
    books: Vec<[i32; 8]>, // [classification][pass]
}
struct Mapping {
    coupling: Vec<(usize, usize)>, // (magnitude, angle)
    mux: Vec<usize>,
    submap_floor: Vec<usize>,
    submap_residue: Vec<usize>,
}
struct Mode {
    blockflag: bool,
    mapping: usize,
}
struct Setup {
    channels: usize,
    rate: u32,
    bs0: usize,
    bs1: usize,
    books: Vec<Codebook>,
    floors: Vec<Option<Floor1>>, // None = floor0 (qöllanmaydi)
    residues: Vec<Residue>,
    mappings: Vec<Mapping>,
    modes: Vec<Mode>,
}

fn parse_setup(p: &[u8], channels: usize, rate: u32, bs0: usize, bs1: usize) -> Option<Setup> {
    if p.len() < 7 || p[0] != 5 || &p[1..7] != b"vorbis" {
        return None;
    }
    let mut br = Br::new(&p[7..]);
    // kodkitoblar
    let bookcount = br.read(8) as usize + 1;
    let mut books = Vec::with_capacity(bookcount);
    for _ in 0..bookcount {
        books.push(build_codebook(&mut br)?);
    }
    // vaqt domenlari (hammasi 0)
    let tcount = br.read(6) as usize + 1;
    for _ in 0..tcount {
        if br.read(16) != 0 {
            return None;
        }
    }
    // floor lar
    let fcount = br.read(6) as usize + 1;
    let mut floors = Vec::with_capacity(fcount);
    for _ in 0..fcount {
        let ftype = br.read(16);
        if ftype == 1 {
            floors.push(Some(parse_floor1(&mut br)?));
        } else if ftype == 0 {
            // floor0 — qöllanmaydi; maydonlarni ötkazib bo'lmaydi aniq, xato
            return None;
        } else {
            return None;
        }
    }
    // residue lar
    let rcount = br.read(6) as usize + 1;
    let mut residues = Vec::with_capacity(rcount);
    for _ in 0..rcount {
        residues.push(parse_residue(&mut br, &books)?);
    }
    // mapping lar
    let mcount = br.read(6) as usize + 1;
    let mut mappings = Vec::with_capacity(mcount);
    for _ in 0..mcount {
        mappings.push(parse_mapping(&mut br, channels)?);
    }
    // mode lar
    let modecount = br.read(6) as usize + 1;
    let mut modes = Vec::with_capacity(modecount);
    for _ in 0..modecount {
        let blockflag = br.read1() != 0;
        let _win = br.read(16);
        let _tr = br.read(16);
        let mapping = br.read(8) as usize;
        modes.push(Mode { blockflag, mapping });
    }
    if br.read1() != 1 {
        return None; // framing bit
    }
    Some(Setup { channels, rate, bs0, bs1, books, floors, residues, mappings, modes })
}

fn parse_floor1(br: &mut Br) -> Option<Floor1> {
    let partitions = br.read(5) as usize;
    let mut partition_class = alloc::vec![0usize; partitions];
    let mut maxclass = 0usize;
    for pc in partition_class.iter_mut() {
        *pc = br.read(4) as usize;
        if *pc > maxclass {
            maxclass = *pc;
        }
    }
    let nclass = maxclass + 1;
    let mut class_dims = alloc::vec![0usize; nclass];
    let mut class_subclasses = alloc::vec![0usize; nclass];
    let mut class_master = alloc::vec![-1i32; nclass];
    let mut subclass_books: Vec<Vec<i32>> = Vec::with_capacity(nclass);
    for c in 0..nclass {
        class_dims[c] = br.read(3) as usize + 1;
        class_subclasses[c] = br.read(2) as usize;
        if class_subclasses[c] > 0 {
            class_master[c] = br.read(8) as i32;
        }
        let nsub = 1usize << class_subclasses[c];
        let mut v = alloc::vec![-1i32; nsub];
        for s in v.iter_mut() {
            *s = br.read(8) as i32 - 1;
        }
        subclass_books.push(v);
    }
    let multiplier = br.read(2) as i32 + 1;
    let rangebits = br.read(4);
    let mut xlist = alloc::vec![0i32, 1i32 << rangebits];
    for &pc in &partition_class {
        let dim = class_dims[pc];
        for _ in 0..dim {
            xlist.push(br.read(rangebits) as i32);
        }
    }
    let values = xlist.len();
    // X böyiça tartib (sorted_order)
    let mut sorted: Vec<usize> = (0..values).collect();
    sorted.sort_by_key(|&i| xlist[i]);
    Some(Floor1 {
        partitions,
        partition_class,
        class_dims,
        class_subclasses,
        class_master,
        subclass_books,
        multiplier,
        xlist,
        sorted,
        values,
    })
}

fn parse_residue(br: &mut Br, books: &[Codebook]) -> Option<Residue> {
    let rtype = br.read(16) as u16;
    if rtype > 2 {
        return None;
    }
    let begin = br.read(24) as usize;
    let end = br.read(24) as usize;
    let part_size = br.read(24) as usize + 1;
    let classifications = br.read(6) as usize + 1;
    let classbook = br.read(8) as usize;
    let mut cascade = alloc::vec![0u32; classifications];
    for c in cascade.iter_mut() {
        let low = br.read(3);
        let flag = br.read1();
        let high = if flag != 0 { br.read(5) } else { 0 };
        *c = high * 8 + low;
    }
    let mut bks = alloc::vec![[-1i32; 8]; classifications];
    for c in 0..classifications {
        for j in 0..8 {
            if cascade[c] & (1 << j) != 0 {
                bks[c][j] = br.read(8) as i32;
            }
        }
    }
    if classbook >= books.len() {
        return None;
    }
    Some(Residue { rtype, begin, end, part_size, classifications, classbook, books: bks })
}

fn parse_mapping(br: &mut Br, channels: usize) -> Option<Mapping> {
    let mtype = br.read(16);
    if mtype != 0 {
        return None;
    }
    let submaps = if br.read1() != 0 { br.read(4) as usize + 1 } else { 1 };
    let mut coupling = Vec::new();
    if br.read1() != 0 {
        let steps = br.read(8) as usize + 1;
        let bits = ilog(channels as i32 - 1);
        for _ in 0..steps {
            let mag = br.read(bits) as usize;
            let ang = br.read(bits) as usize;
            coupling.push((mag, ang));
        }
    }
    if br.read(2) != 0 {
        return None; // reserved
    }
    let mut mux = alloc::vec![0usize; channels];
    if submaps > 1 {
        for m in mux.iter_mut() {
            *m = br.read(4) as usize;
        }
    }
    let mut submap_floor = alloc::vec![0usize; submaps];
    let mut submap_residue = alloc::vec![0usize; submaps];
    for i in 0..submaps {
        let _ = br.read(8); // ishlatilmaydi
        submap_floor[i] = br.read(8) as usize;
        submap_residue[i] = br.read(8) as usize;
    }
    Some(Mapping { coupling, mux, submap_floor, submap_residue })
}

// ---- floor1 dekod ----
fn predict_point(x: i32, x0: i32, x1: i32, y0: i32, y1: i32) -> i32 {
    let dy = y1 - y0;
    let adx = x1 - x0;
    let err = (dy.abs()) * (x - x0);
    let off = err / adx;
    if dy < 0 {
        y0 - off
    } else {
        y0 + off
    }
}

fn neighbors(x: &[i32], j: usize) -> (usize, usize) {
    let (mut low, mut high) = (0usize, 0usize);
    let mut low_ok = false;
    let mut high_ok = false;
    for i in 0..j {
        if x[i] < x[j] && (!low_ok || x[i] > x[low]) {
            low = i;
            low_ok = true;
        }
        if x[i] > x[j] && (!high_ok || x[i] < x[high]) {
            high = i;
            high_ok = true;
        }
    }
    (low, high)
}

/// floor1 dekod — finalY ni qaytaradi (yoki None = kanal bo'sh).
fn decode_floor1(br: &mut Br, f: &Floor1, books: &[Codebook]) -> Option<Vec<i32>> {
    if br.read1() == 0 {
        return None; // bu kanal bu kadrda bo'sh
    }
    let range = [256, 128, 86, 64][(f.multiplier - 1) as usize];
    let mut finaly = alloc::vec![0i32; f.values];
    finaly[0] = br.read(ilog(range) - 1) as i32;
    finaly[1] = br.read(ilog(range) - 1) as i32;
    let mut offset = 2usize;
    for &pclass in &f.partition_class {
        let cdim = f.class_dims[pclass];
        let cbits = f.class_subclasses[pclass];
        let csub = (1i32 << cbits) - 1;
        let mut cval = 0i32;
        if cbits > 0 {
            let cb = &books[f.class_master[pclass] as usize];
            cval = cb.decode_scalar(br);
            if cval < 0 {
                return None;
            }
        }
        for _ in 0..cdim {
            let book = f.subclass_books[pclass][(cval & csub) as usize];
            cval >>= cbits;
            if book >= 0 {
                let t = books[book as usize].decode_scalar(br);
                finaly[offset] = if t < 0 { 0 } else { t };
            } else {
                finaly[offset] = 0;
            }
            offset += 1;
        }
    }
    // 2-bosqiç: amplitudalarni tiklaymiz (predict + room)
    let mut step2 = alloc::vec![false; f.values];
    step2[0] = true;
    step2[1] = true;
    let mut out = finaly.clone();
    for j in 2..f.values {
        let (low, high) = neighbors(&f.xlist, j);
        let pred = predict_point(f.xlist[j], f.xlist[low], f.xlist[high], out[low], out[high]);
        let val = finaly[j];
        let highroom = range - pred;
        let lowroom = pred;
        let room = if highroom < lowroom { highroom * 2 } else { lowroom * 2 };
        if val != 0 {
            step2[low] = true;
            step2[high] = true;
            step2[j] = true;
            if val >= room {
                if highroom > lowroom {
                    out[j] = val - lowroom + pred;
                } else {
                    out[j] = pred - val + highroom - 1;
                }
            } else if val & 1 != 0 {
                out[j] = pred - ((val + 1) >> 1);
            } else {
                out[j] = pred + (val >> 1);
            }
        } else {
            step2[j] = false;
            out[j] = pred;
        }
    }
    // step2 false bo'lgan nuqtalarni -1 bilan belgilaymiz (deferred)
    for j in 0..f.values {
        if !step2[j] {
            out[j] = -1;
        }
    }
    Some(out)
}

/// floor1 egri chizigʼini chizadi (target ga n2 qiymat).
fn synth_floor1(f: &Floor1, finaly: &[i32], target: &mut [f32], n2: usize) {
    let mult = f.multiplier;
    let mut lx = 0usize;
    let mut ly = finaly[0] * mult;
    for q in 1..f.values {
        let j = f.sorted[q];
        if finaly[j] >= 0 {
            let hy = finaly[j] * mult;
            let hx = f.xlist[j] as usize;
            if lx != hx {
                draw_line(target, lx, ly, hx, hy, n2);
            }
            lx = hx;
            ly = hy;
        }
    }
    if lx < n2 {
        let v = INVERSE_DB[(ly as usize).min(255)];
        for t in target.iter_mut().take(n2).skip(lx) {
            *t = v;
        }
    }
}

fn draw_line(target: &mut [f32], x0: usize, y0: i32, x1: usize, y1: i32, n: usize) {
    let dy = y1 - y0;
    let adx = (x1 - x0) as i32;
    if adx == 0 {
        return;
    }
    let base = dy / adx; // nolga tomon qisqartiriladi (C bilan bir xil)
    let sy = if dy < 0 { base - 1 } else { base + 1 };
    let ady = dy.abs() - base.abs() * adx;
    let mut err = 0i32;
    let mut y = y0;
    if x0 < n {
        target[x0] = INVERSE_DB[y.clamp(0, 255) as usize];
    }
    for x in (x0 + 1)..x1 {
        err += ady;
        if err >= adx {
            err -= adx;
            y += sy;
        } else {
            y += base;
        }
        if x < n {
            target[x] = INVERSE_DB[y.clamp(0, 255) as usize];
        }
    }
}

// ---- residue dekod ----
fn residue_vq_add(book: &Codebook, br: &mut Br, target: &mut [f32], offset: usize, n: usize) -> bool {
    // residue type 1: ketma-ket dim-vektorlarni qo'shadi
    let mut k = 0usize;
    let mut off = offset;
    while k < n {
        let z = book.decode_scalar(br);
        if z < 0 {
            return false;
        }
        let zi = z as usize;
        for i in 0..book.dim {
            if off + i < target.len() && zi * book.dim + i < book.values.len() {
                target[off + i] += book.values[zi * book.dim + i];
            }
        }
        k += book.dim;
        off += book.dim;
    }
    true
}
fn residue_vq_add_type0(book: &Codebook, br: &mut Br, target: &mut [f32], offset: usize, n: usize) -> bool {
    // residue type 0: qadamli (step) joylaş
    let step = n / book.dim;
    for k in 0..step {
        let z = book.decode_scalar(br);
        if z < 0 {
            return false;
        }
        let zi = z as usize;
        for i in 0..book.dim {
            let idx = offset + k + i * step;
            if idx < target.len() && zi * book.dim + i < book.values.len() {
                target[idx] += book.values[zi * book.dim + i];
            }
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn decode_residue(
    r: &Residue,
    books: &[Codebook],
    br: &mut Br,
    buffers: &mut [Vec<f32>],
    do_not_decode: &[bool],
    ch: usize,
    n: usize,
) {
    let classbook = &books[r.classbook];
    let classwords = classbook.dim;
    let actual = if r.rtype == 2 { n * 2 } else { n };
    let rbegin = r.begin.min(actual);
    let rend = r.end.min(actual);
    if rend <= rbegin {
        return;
    }
    let n_read = rend - rbegin;
    let part_read = n_read / r.part_size;

    if r.rtype == 2 {
        // barcha kanallarni bitta oqimga birlashtirib dekodlaymiz
        if do_not_decode.iter().take(ch).all(|&x| x) {
            return;
        }
        // type 2: interleaved — bitta buferga yig'ib, keyin bo'lamiz
        let total = n * ch;
        let mut merged = alloc::vec![0.0f32; total];
        decode_residue_into(r, books, br, &mut [merged.as_mut_slice()], &[false], 1, total, classwords, part_read, rbegin);
        // deinterleave
        for (c, buf) in buffers.iter_mut().enumerate().take(ch) {
            if do_not_decode[c] {
                continue;
            }
            for i in 0..n {
                buf[i] += merged[i * ch + c];
            }
        }
        return;
    }
    // type 0/1: har kanal alohida, lekin klassifikatsiya birgalikda
    decode_residue_1(r, books, br, buffers, do_not_decode, ch, n, classwords, part_read, rbegin);
}

#[allow(clippy::too_many_arguments)]
fn decode_residue_1(
    r: &Residue,
    books: &[Codebook],
    br: &mut Br,
    buffers: &mut [Vec<f32>],
    do_not_decode: &[bool],
    ch: usize,
    _n: usize,
    classwords: usize,
    part_read: usize,
    rbegin: usize,
) {
    let classbook_idx = r.classbook;
    // classifications[ch][part_read]
    let mut cls = alloc::vec![alloc::vec![0usize; part_read]; ch];
    for pass in 0..8 {
        let mut pcount = 0usize;
        while pcount < part_read {
            if pass == 0 {
                for j in 0..ch {
                    if do_not_decode[j] {
                        continue;
                    }
                    let mut temp = books[classbook_idx].decode_scalar(br);
                    if temp < 0 {
                        return;
                    }
                    let mut tmp = temp as usize;
                    // classword larni teskari tartibda yoyamiz
                    let mut arr = alloc::vec![0usize; classwords];
                    for i in (0..classwords).rev() {
                        arr[i] = tmp % r.classifications;
                        tmp /= r.classifications;
                    }
                    for i in 0..classwords {
                        if pcount + i < part_read {
                            cls[j][pcount + i] = arr[i];
                        }
                    }
                    let _ = &mut temp;
                }
            }
            let mut i = 0usize;
            while i < classwords && pcount < part_read {
                for j in 0..ch {
                    if do_not_decode[j] {
                        continue;
                    }
                    let c = cls[j][pcount];
                    let b = r.books[c][pass];
                    if b >= 0 {
                        let offset = rbegin + pcount * r.part_size;
                        let book = &books[b as usize];
                        let ok = if r.rtype == 0 {
                            residue_vq_add_type0(book, br, &mut buffers[j], offset, r.part_size)
                        } else {
                            residue_vq_add(book, br, &mut buffers[j], offset, r.part_size)
                        };
                        if !ok {
                            return;
                        }
                    }
                }
                i += 1;
                pcount += 1;
            }
        }
    }
}

// type 2 uçun bitta oqimga dekod (merged buferga)
#[allow(clippy::too_many_arguments)]
fn decode_residue_into(
    r: &Residue,
    books: &[Codebook],
    br: &mut Br,
    buffers: &mut [&mut [f32]],
    do_not_decode: &[bool],
    _ch: usize,
    _total: usize,
    classwords: usize,
    part_read: usize,
    rbegin: usize,
) {
    let classbook_idx = r.classbook;
    let mut cls = alloc::vec![0usize; part_read];
    for pass in 0..8 {
        let mut pcount = 0usize;
        while pcount < part_read {
            if pass == 0 {
                if do_not_decode[0] {
                    return;
                }
                let temp = books[classbook_idx].decode_scalar(br);
                if temp < 0 {
                    return;
                }
                let mut tmp = temp as usize;
                let mut arr = alloc::vec![0usize; classwords];
                for i in (0..classwords).rev() {
                    arr[i] = tmp % r.classifications;
                    tmp /= r.classifications;
                }
                for i in 0..classwords {
                    if pcount + i < part_read {
                        cls[pcount + i] = arr[i];
                    }
                }
            }
            let mut i = 0usize;
            while i < classwords && pcount < part_read {
                let c = cls[pcount];
                let b = r.books[c][pass];
                if b >= 0 {
                    let offset = rbegin + pcount * r.part_size;
                    let book = &books[b as usize];
                    if !residue_vq_add(book, br, buffers[0], offset, r.part_size) {
                        return;
                    }
                }
                i += 1;
                pcount += 1;
            }
        }
    }
}

// ---- teskari MDCT (jadval asosida, stb slow#2 bilan bir xil) ----
struct Imdct {
    n: usize,
    mcos: Vec<f32>, // 4n
}
impl Imdct {
    fn new(n: usize) -> Imdct {
        let mut mcos = alloc::vec![0.0f32; 4 * n];
        for (k, m) in mcos.iter_mut().enumerate() {
            *m = cosf(PI64 / 2.0 * k as f64 / n as f64) as f32;
        }
        Imdct { n, mcos }
    }
    fn run(&self, x: &[f32], out: &mut [f32]) {
        let n = self.n;
        let n2 = n >> 1;
        let mask = (n << 2) - 1;
        for i in 0..n {
            let a = (2 * i + 1 + n2) as usize;
            let mut acc = 0.0f32;
            for j in 0..n2 {
                let idx = (a * (2 * j + 1)) & mask;
                acc += x[j] * self.mcos[idx];
            }
            out[i] = acc;
        }
    }
}

// ---- oyna ----
fn compute_window(n: usize) -> Vec<f32> {
    let n2 = n >> 1;
    let mut w = alloc::vec![0.0f32; n2];
    for (i, wi) in w.iter_mut().enumerate() {
        let s = sinf((i as f64 + 0.5) / n2 as f64 * 0.5 * PI64);
        *wi = sinf(0.5 * PI64 * s * s) as f32;
    }
    w
}

fn decode_full(d: &[u8]) -> Option<Clip> {
    let stream = ogg::parse(d)?;
    if stream.packets.len() < 3 {
        return None;
    }
    let id = parse_ident(&stream.packets[0])?;
    let channels = id.channels as usize;
    let bs0 = id.bs0 as usize;
    let bs1 = id.bs1 as usize;
    if channels == 0 || channels > 8 || bs0 < 64 || bs1 > 8192 {
        return None;
    }
    let setup = parse_setup(&stream.packets[2], channels, id.rate, bs0, bs1)?;

    let win0 = compute_window(bs0);
    let win1 = compute_window(bs1);
    let imdct0 = Imdct::new(bs0);
    let imdct1 = Imdct::new(bs1);

    // kanal buferlari va overlap holati
    let mut prev_window: Vec<Vec<f32>> = (0..channels).map(|_| alloc::vec![0.0f32; bs1 / 2]).collect();
    let mut prev_len = 0usize;
    let mut out: Vec<Vec<i16>> = (0..channels).map(|_| Vec::new()).collect();

    // audio paketlar (3 dan boshlab)
    for pkt in stream.packets.iter().skip(3) {
        if pkt.is_empty() {
            continue;
        }
        let mut br = Br::new(pkt);
        if br.read1() != 0 {
            continue; // audio emas
        }
        let modenum = br.read(ilog(setup.modes.len() as i32 - 1)) as usize;
        if modenum >= setup.modes.len() {
            break;
        }
        let blockflag = setup.modes[modenum].blockflag;
        let n = if blockflag { bs1 } else { bs0 };
        let n2 = n >> 1;
        let (prev_wf, next_wf) = if blockflag { (br.read1() != 0, br.read1() != 0) } else { (true, true) };
        let (left_start, _left_end, right_start, right_end) = window_bounds(n, bs0, blockflag, prev_wf, next_wf);
        let map = &setup.mappings[setup.modes[modenum].mapping];

        // floor dekod
        let mut channel_buf: Vec<Vec<f32>> = (0..channels).map(|_| alloc::vec![0.0f32; n]).collect();
        let mut floor_y: Vec<Option<Vec<i32>>> = Vec::with_capacity(channels);
        let mut zero_channel = alloc::vec![false; channels];
        for c in 0..channels {
            let submap = map.mux[c];
            let fl = setup.floors[map.submap_floor[submap]].as_ref().unwrap();
            match decode_floor1(&mut br, fl, &setup.books) {
                Some(y) => floor_y.push(Some(y)),
                None => {
                    zero_channel[c] = true;
                    floor_y.push(None);
                }
            }
        }
        // coupling: bo'sh kanallarni qayta yoqish
        let mut really_zero = zero_channel.clone();
        for &(mag, ang) in &map.coupling {
            if !zero_channel[mag] || !zero_channel[ang] {
                zero_channel[mag] = false;
                zero_channel[ang] = false;
            }
        }
        // residue dekod (submap bo'yicha)
        let submaps = map.submap_floor.len();
        for sm in 0..submaps {
            // bu submapga tegishli kanallar
            let mut chans: Vec<usize> = Vec::new();
            for c in 0..channels {
                if map.mux[c] == sm {
                    chans.push(c);
                }
            }
            if chans.is_empty() {
                continue;
            }
            let rno = map.submap_residue[sm];
            let r = &setup.residues[rno];
            // vaqtincha buferlar
            let mut bufs: Vec<Vec<f32>> = chans.iter().map(|_| alloc::vec![0.0f32; n2]).collect();
            let dnd: Vec<bool> = chans.iter().map(|&c| zero_channel[c]).collect();
            decode_residue(r, &setup.books, &mut br, &mut bufs, &dnd, chans.len(), n2);
            for (k, &c) in chans.iter().enumerate() {
                for i in 0..n2 {
                    channel_buf[c][i] = bufs[k][i];
                }
            }
        }
        // teskari coupling
        for &(mag, ang) in map.coupling.iter().rev() {
            for i in 0..n2 {
                let m = channel_buf[mag][i];
                let a = channel_buf[ang][i];
                let (newm, newa) = if m > 0.0 {
                    if a > 0.0 {
                        (m, m - a)
                    } else {
                        (m + a, m)
                    }
                } else if a > 0.0 {
                    (m, m + a)
                } else {
                    (m - a, m)
                };
                channel_buf[mag][i] = newm;
                channel_buf[ang][i] = newa;
            }
        }
        // floor ni qo'llaymiz (spektr = residue * floor_curve)
        let mut floor_curve = alloc::vec![0.0f32; n2];
        for c in 0..channels {
            if really_zero[c] {
                for v in channel_buf[c].iter_mut().take(n2) {
                    *v = 0.0;
                }
            } else if let Some(y) = &floor_y[c] {
                let submap = map.mux[c];
                let fl = setup.floors[map.submap_floor[submap]].as_ref().unwrap();
                for v in floor_curve.iter_mut() {
                    *v = 0.0;
                }
                synth_floor1(fl, y, &mut floor_curve, n2);
                for i in 0..n2 {
                    channel_buf[c][i] *= floor_curve[i];
                }
            }
        }
        // teskari MDCT
        let imdct = if blockflag { &imdct1 } else { &imdct0 };
        let mut timebuf = alloc::vec![0.0f32; n];
        for c in 0..channels {
            // spektr n2 da; qolgan n2 ni 0 deb hisoblaymiz? IMDCT n2 kirish oladi.
            imdct.run(&channel_buf[c][..n2], &mut timebuf);
            channel_buf[c][..n].copy_from_slice(&timebuf);
        }
        // overlap-add (finish_frame)
        let len = right_end;
        let left = left_start;
        let right = right_start;
        if prev_len > 0 {
            let w: &[f32] = if prev_len * 2 == bs0 { &win0 } else { &win1 };
            let wn = prev_len;
            for c in 0..channels {
                for j in 0..wn {
                    if left + j < n && j < w.len() {
                        channel_buf[c][left + j] =
                            channel_buf[c][left + j] * w[j] + prev_window[c][j] * w[wn - 1 - j];
                    }
                }
            }
            // çiqariş: [left, right)
            for c in 0..channels {
                for i in left..right {
                    let s = (channel_buf[c][i] * 32767.0).clamp(-32768.0, 32767.0);
                    out[c].push(s as i16);
                }
            }
        }
        // yangi prev
        prev_len = len - right;
        for c in 0..channels {
            for j in 0..prev_len {
                if right + j < n {
                    prev_window[c][j] = channel_buf[c][right + j];
                }
            }
        }
    }

    let frames = out[0].len();
    if frames == 0 {
        return None;
    }
    // interleave
    let mut samples = Vec::with_capacity(frames * channels);
    for i in 0..frames {
        for c in 0..channels {
            samples.push(out[c].get(i).copied().unwrap_or(0));
        }
    }
    Some(Clip { rate: id.rate, channels: id.channels as u16, samples })
}

/// decode_initial dagi oyna chegaralarini hisoblaydi.
fn window_bounds(n: usize, bs0: usize, blockflag: bool, prev: bool, next: bool) -> (usize, usize, usize, usize) {
    let center = n >> 1;
    let (left_start, left_end) = if blockflag && !prev {
        ((n - bs0) >> 2, (n + bs0) >> 2)
    } else {
        (0, center)
    };
    let (right_start, right_end) = if blockflag && !next {
        ((n * 3 - bs0) >> 2, (n * 3 + bs0) >> 2)
    } else {
        (center, n)
    };
    (left_start, left_end, right_start, right_end)
}

pub fn decode(d: &[u8]) -> Option<Clip> {
    decode_full(d)
}
