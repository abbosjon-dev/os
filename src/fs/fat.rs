//! FAT16/FAT32 fayl tizimini öqiş (faqat öqiş). Ikkinçi diskdan.
//! Qisqa (8.3) nomlar; uzun nomlar (VFAT) eʼtiborsiz qoldiriladi.

use crate::drivers::virtio_blk as blk;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const SEC: usize = 512;

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
