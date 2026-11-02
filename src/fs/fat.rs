//! FAT16/FAT32 fayl tizimi: öqiş va yoziş. Ikkinçi diskdan.
//! Qisqa (8.3) nomlar; uzun nomlar (VFAT) eʼtiborsiz qoldiriladi.
//! Yoziş ildiz papkadagi fayllar uçun (yaratiş/qayta yoziş).

use crate::drivers::virtio_blk as blk;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const SEC: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FatError {
    NoDisk,
    BadName,
    Full,       // boʻsh klaster yoki ildiz papkada joy yöq
    Io,
}

struct Bpb {
    spc: u32,         // sektor/klaster
    reserved: u32,    // zaxira sektorlar
    nfats: u32,       // FAT nusxalari
    root_entries: u32,
    fatsz: u32,       // FAT hajmi (sektor)
    is_fat32: bool,
    root_cluster: u32, // faqat FAT32
}


fn rd_sector(sector: u64) -> Option<[u8; SEC]> {
    let mut b = [0u8; SEC];
    blk::fat_read(sector, &mut b).ok()?;
    Some(b)
}

fn parse_bpb() -> Option<Bpb> {
    let b = rd_sector(0)?;
    if u16::from_le_bytes([b[11], b[12]]) as usize != SEC {
        return None;
    }
    let root_entries = u16::from_le_bytes([b[17], b[18]]) as u32;
    let fatsz16 = u16::from_le_bytes([b[22], b[23]]) as u32;
    let (fatsz, is_fat32, root_cluster) = if fatsz16 == 0 {
        (u32::from_le_bytes([b[36], b[37], b[38], b[39]]), true, u32::from_le_bytes([b[44], b[45], b[46], b[47]]))
    } else {
        (fatsz16, false, 0)
    };
    Some(Bpb {
        spc: b[13] as u32,
        reserved: u16::from_le_bytes([b[14], b[15]]) as u32,
        nfats: b[16] as u32,
        root_entries,
        fatsz,
        is_fat32,
        root_cluster,
    })
}

impl Bpb {
    fn fat_start(&self) -> u32 {
        self.reserved
    }
    fn root_start(&self) -> u32 {
        self.reserved + self.nfats * self.fatsz
    }
    fn root_sectors(&self) -> u32 {
        (self.root_entries * 32).div_ceil(SEC as u32)
    }
    fn data_start(&self) -> u32 {
        self.root_start() + self.root_sectors()
    }
    fn cluster_sector(&self, cl: u32) -> u64 {
        (self.data_start() + (cl - 2) * self.spc) as u64
    }

    /// FAT dagi keyingi klaster.
    fn next_cluster(&self, cl: u32) -> Option<u32> {
        if self.is_fat32 {
            let off = cl * 4;
            let sec = self.fat_start() as u64 + (off / SEC as u32) as u64;
            let b = rd_sector(sec)?;
            let i = (off % SEC as u32) as usize;
            let v = u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) & 0x0FFF_FFFF;
            if v >= 0x0FFF_FFF8 { None } else { Some(v) }
        } else {
            let off = cl * 2;
            let sec = self.fat_start() as u64 + (off / SEC as u32) as u64;
            let b = rd_sector(sec)?;
            let i = (off % SEC as u32) as usize;
            let v = u16::from_le_bytes([b[i], b[i + 1]]) as u32;
            if v >= 0xFFF8 { None } else { Some(v) }
        }
    }

    /// Klaster zanjirini öqiydi (eng köpi `size` bayt).
    fn read_chain(&self, first: u32, size: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(size);
        let mut cl = first;
        let clu_bytes = self.spc as usize * SEC;
        let mut guard = 0;
        while cl >= 2 && out.len() < size && guard < 1 << 20 {
            for k in 0..self.spc {
                if let Some(b) = rd_sector(self.cluster_sector(cl) + k as u64) {
                    out.extend_from_slice(&b);
                }
            }
            guard += 1;
            match self.next_cluster(cl) {
                Some(n) => cl = n,
                None => break,
            }
        }
        let _ = clu_bytes;
        out.truncate(size);
        out
    }
}

/// 8.3 nomdan "README.TXT" körinişi.
fn name_83(e: &[u8]) -> Option<String> {
    if e[0] == 0 || e[0] == 0xe5 || e[0] == 0x2e {
        return None; // böş / öçirilgan / "." ".."
    }
    if e[11] & 0x0f == 0x0f || e[11] & 0x08 != 0 {
        return None; // uzun nom qismi yoki hajm nomi (volume label)
    }
    let base = core::str::from_utf8(&e[0..8]).ok()?.trim_end();
    let ext = core::str::from_utf8(&e[8..11]).ok()?.trim_end();
    if base.is_empty() {
        return None;
    }
    let mut n = String::from(base);
    if !ext.is_empty() {
        n.push('.');
        n.push_str(ext);
    }
    Some(n)
}

/// Ildiz papkadagi fayllarni (nom, maʼlumot) qaytaradi. Faqat öqiş.
pub fn read_root() -> Option<Vec<(String, Vec<u8>)>> {
    let bpb = parse_bpb()?;
    let mut files = Vec::new();
    // Ildiz papka sektorlari (FAT16) yoki klaster zanjiri (FAT32)
    let entries: Vec<u8> = if bpb.is_fat32 {
        bpb.read_chain(bpb.root_cluster, 1 << 20)
    } else {
        let mut v = Vec::new();
        for s in 0..bpb.root_sectors() {
            if let Some(b) = rd_sector(bpb.root_start() as u64 + s as u64) {
                v.extend_from_slice(&b);
            }
        }
        v
    };
    for e in entries.chunks(32) {
        if e.len() < 32 || e[0] == 0 {
            break;
        }
        let attr = e[11];
        if attr & 0x10 != 0 {
            continue; // papka — hozirça faqat ildiz
        }
        let Some(name) = name_83(e) else { continue };
        let first = u16::from_le_bytes([e[26], e[27]]) as u32 | if bpb.is_fat32 { (u16::from_le_bytes([e[20], e[21]]) as u32) << 16 } else { 0 };
        let size = u32::from_le_bytes([e[28], e[29], e[30], e[31]]) as usize;
        let data = if size > 0 && first >= 2 { bpb.read_chain(first, size) } else { vec![] };
        files.push((name, data));
    }
    Some(files)
}

// ---------------------------------------------------------------------------
// Yoziş
// ---------------------------------------------------------------------------

fn wr_sector(sector: u64, b: &[u8; SEC]) -> Result<(), FatError> {
    blk::fat_write(sector, b).map_err(|_| FatError::Io)
}

/// "readme.txt" → 11 baytli 8.3 (katta harf, böşliq bilan töldirilgan).
/// Nom moslaşmasa — `None`.
fn to_83(name: &str) -> Option<[u8; 11]> {
    let name = name.trim();
    let (base, ext) = match name.rsplit_once('.') {
        Some((b, e)) => (b, e),
        None => (name, ""),
    };
    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return None;
    }
    let ok = |s: &str| s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'~'));
    if !ok(base) || !ok(ext) {
        return None;
    }
    let mut out = [b' '; 11];
    for (i, c) in base.bytes().enumerate() {
        out[i] = c.to_ascii_uppercase();
    }
    for (i, c) in ext.bytes().enumerate() {
        out[8 + i] = c.to_ascii_uppercase();
    }
    Some(out)
}

impl Bpb {
    /// FAT dagi xom qiymat (EOC/boʻsh ajratilmagan holda).
    fn fat_raw(&self, cl: u32) -> Option<u32> {
        let (off, width) = if self.is_fat32 { (cl * 4, 4) } else { (cl * 2, 2) };
        let sec = self.fat_start() as u64 + (off / SEC as u32) as u64;
        let b = rd_sector(sec)?;
        let i = (off % SEC as u32) as usize;
        Some(if width == 4 {
            u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) & 0x0FFF_FFFF
        } else {
            u16::from_le_bytes([b[i], b[i + 1]]) as u32
        })
    }

    /// FAT yozuvini örnatadi (barça FAT nusxalarida).
    fn set_fat(&self, cl: u32, val: u32) -> Result<(), FatError> {
        let (off, width) = if self.is_fat32 { (cl * 4, 4usize) } else { (cl * 2, 2usize) };
        for copy in 0..self.nfats {
            let sec = self.fat_start() as u64 + (copy * self.fatsz) as u64 + (off / SEC as u32) as u64;
            let mut b = rd_sector(sec).ok_or(FatError::Io)?;
            let i = (off % SEC as u32) as usize;
            if width == 4 {
                let keep = u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) & 0xF000_0000;
                let v = keep | (val & 0x0FFF_FFFF);
                b[i..i + 4].copy_from_slice(&v.to_le_bytes());
            } else {
                b[i..i + 2].copy_from_slice(&(val as u16).to_le_bytes());
            }
            wr_sector(sec, &b)?;
        }
        Ok(())
    }

    fn eoc(&self) -> u32 {
        if self.is_fat32 { 0x0FFF_FFFF } else { 0xFFFF }
    }

    fn total_clusters(&self) -> u32 {
        let data_sectors = blk::fat_capacity_sectors() as u32 - self.data_start();
        data_sectors / self.spc + 2
    }

    /// Boʻsh klaster qidiradi (2 dan boşlab). Topmasa — `None`.
    fn free_cluster(&self, after: u32) -> Option<u32> {
        let total = self.total_clusters();
        for cl in after.max(2)..total {
            if self.fat_raw(cl) == Some(0) {
                return Some(cl);
            }
        }
        None
    }

    /// Klaster zanjirini böşatadi (öçirilgan fayl).
    fn free_chain(&self, first: u32) {
        let mut cl = first;
        let mut guard = 0;
        while cl >= 2 && guard < (1 << 20) {
            let next = self.next_cluster(cl);
            let _ = self.set_fat(cl, 0);
            guard += 1;
            match next {
                Some(n) => cl = n,
                None => break,
            }
        }
    }

    /// Maʼlumotni yangi klaster zanjiriga yozadi. Birinçi klasterni qaytaradi (0 — boʻsh fayl).
    fn write_data(&self, data: &[u8]) -> Result<u32, FatError> {
        if data.is_empty() {
            return Ok(0);
        }
        let clu_bytes = self.spc as usize * SEC;
        let n = data.len().div_ceil(clu_bytes);
        // Klasterlarni ajratamiz
        let mut clusters = Vec::with_capacity(n);
        let mut search = 2;
        for _ in 0..n {
            let cl = self.free_cluster(search).ok_or(FatError::Full)?;
            // Vaqtincha band qilib qöyamiz (EOC) — keyingi qidiruv uni ötkazib yuboradi
            self.set_fat(cl, self.eoc())?;
            clusters.push(cl);
            search = cl + 1;
        }
        // Zanjirni bogʻlaymiz
        for i in 0..n {
            let val = if i + 1 < n { clusters[i + 1] } else { self.eoc() };
            self.set_fat(clusters[i], val)?;
        }
        // Maʼlumotni yozamiz (oxirgi klaster nollar bilan töldiriladi)
        for (i, &cl) in clusters.iter().enumerate() {
            let base = self.cluster_sector(cl);
            for s in 0..self.spc {
                let mut sect = [0u8; SEC];
                let start = i * clu_bytes + s as usize * SEC;
                if start < data.len() {
                    let end = (start + SEC).min(data.len());
                    sect[..end - start].copy_from_slice(&data[start..end]);
                }
                wr_sector(base + s as u64, &sect)?;
            }
        }
        Ok(clusters[0])
    }

    /// Ildiz papka yozuvlari joylaşgan sektorlar röyxati.
    fn root_dir_sectors(&self) -> Vec<u64> {
        if self.is_fat32 {
            let mut secs = Vec::new();
            let mut cl = self.root_cluster;
            let mut guard = 0;
            while cl >= 2 && guard < (1 << 16) {
                for k in 0..self.spc {
                    secs.push(self.cluster_sector(cl) + k as u64);
                }
                guard += 1;
                match self.next_cluster(cl) {
                    Some(n) => cl = n,
                    None => break,
                }
            }
            secs
        } else {
            (0..self.root_sectors()).map(|s| self.root_start() as u64 + s as u64).collect()
        }
    }
}

/// Ildiz papkaga fayl yozadi (yaratadi yoki qayta yozadi). FAT16/FAT32.
pub fn write_root_file(name: &str, data: &[u8]) -> Result<(), FatError> {
    if !blk::fat_present() {
        return Err(FatError::NoDisk);
    }
    let bpb = parse_bpb().ok_or(FatError::NoDisk)?;
    let name83 = to_83(name).ok_or(FatError::BadName)?;

    // Avval maʼlumotni yozib, yangi zanjir birinçi klasterini olamiz
    let first = bpb.write_data(data)?;

    // Ildiz papkada moslaşuvçi yoki boʻsh yozuv qidiramiz
    let secs = bpb.root_dir_sectors();
    let mut free: Option<(u64, usize)> = None; // (sektor, içidagi örin)
    for &sec in &secs {
        let mut b = rd_sector(sec).ok_or(FatError::Io)?;
        for i in (0..SEC).step_by(32) {
            let e = &b[i..i + 32];
            if e[0] == 0 || e[0] == 0xe5 {
                if free.is_none() {
                    free = Some((sec, i));
                }
                if e[0] == 0 {
                    // Bundan keyin hammasi boʻsh — moslaşuv yöq, boʻsh öringa yozamiz
                    return place_entry(&bpb, free, &name83, first, data.len() as u32, sec, &mut b);
                }
                continue;
            }
            if e[11] & 0x0f == 0x0f {
                continue; // uzun nom qismi
            }
            if e[0..11] == name83 {
                // Mavjud fayl — eski zanjirni böşatamiz va joyida yangilaymiz
                let old = u16::from_le_bytes([e[26], e[27]]) as u32
                    | if bpb.is_fat32 { (u16::from_le_bytes([e[20], e[21]]) as u32) << 16 } else { 0 };
                if old >= 2 {
                    bpb.free_chain(old);
                }
                write_dir_entry(&mut b[i..i + 32], &name83, first, data.len() as u32, bpb.is_fat32);
                return wr_sector(sec, &b);
            }
        }
    }
    // Boʻsh öringa (0xe5) yozamiz
    if let Some((sec, _)) = free {
        let mut b = rd_sector(sec).ok_or(FatError::Io)?;
        return place_entry(&bpb, free, &name83, first, data.len() as u32, sec, &mut b);
    }
    // Zanjirni böşatamiz — joy topilmadi
    if first >= 2 {
        bpb.free_chain(first);
    }
    Err(FatError::Full)
}

fn place_entry(bpb: &Bpb, free: Option<(u64, usize)>, name83: &[u8; 11], first: u32, size: u32, cur_sec: u64, cur: &mut [u8; SEC]) -> Result<(), FatError> {
    let (sec, off) = free.ok_or(FatError::Full)?;
    if sec == cur_sec {
        write_dir_entry(&mut cur[off..off + 32], name83, first, size, bpb.is_fat32);
        wr_sector(sec, cur)
    } else {
        let mut b = rd_sector(sec).ok_or(FatError::Io)?;
        write_dir_entry(&mut b[off..off + 32], name83, first, size, bpb.is_fat32);
        wr_sector(sec, &b)
    }
}

fn write_dir_entry(e: &mut [u8], name83: &[u8; 11], first: u32, size: u32, is_fat32: bool) {
    for x in e.iter_mut() {
        *x = 0;
    }
    e[0..11].copy_from_slice(name83);
    e[11] = 0x20; // arxiv (oddiy fayl)
    e[26..28].copy_from_slice(&(first as u16).to_le_bytes()); // first cluster low
    if is_fat32 {
        e[20..22].copy_from_slice(&((first >> 16) as u16).to_le_bytes()); // first cluster high
    }
    e[28..32].copy_from_slice(&size.to_le_bytes());
}
