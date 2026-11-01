//! ZaminFS 2 — diskdagi şifrlangan, bloklarga asoslangan fayl tizimi.
//!
//! Tuzilma (disk sektor 64 dan boşlab, 4 KiB bloklar):
//!   blok 0, 1  — superblok A/B (navbatma-navbat yoziladi, avlod raqami bilan)
//!   qolgani    — metadata va maʼlumot bloklari
//!
//! Himoya — autentifikatsiyalangan daraxt:
//!   superblok (AEAD, sarlavha qöşimça maʼlumot) → metadata bloklari röyxati va ularning nonceʼlari
//!   metadata (fayllar, papkalar, egasi, ruxsatlar, vaqtlar) → har bir maʼlumot blokining raqami va nonceʼi
//!   maʼlumot bloki: [teg 16][şifrlangan 4080 bayt], AAD = tur + blok raqami
//! Bitta bayt özgarsa, blok boşqa joyga köçirilsa yoki eski nusxasi qaytarilsa — aniqlanadi.
//!
//! Copy-on-write: bloklar heç qaçon joyida qayta yozilmaydi. Yangi bloklar yoziladi, keyin
//! superblok almaştiriladi. Eski bloklar faqat keyingi muvaffaqiyatli saqlaşdan keyin böşatiladi,
//! şuning uçun tok qaysi payt öçmasin, oxirgi butun holat (yoki undan oldingisi) saqlanib qoladi.

use crate::crypto::{aead, hmac::hmac_sha256, zeroize};
use crate::drivers::virtio_blk::{self as disk};
use crate::random;
use crate::sync::Mutex;
use alloc::vec;
use alloc::vec::Vec;

pub const BLOCK: usize = 4096;
/// Bitta blokdagi foydali maʼlumot (teg 16 bayt)
pub const BLOCK_DATA: usize = BLOCK - 16;
const SECTORS_PER_BLOCK: u64 = (BLOCK / disk::SECTOR) as u64;
const FS_START: u64 = 64;
const MAGIC: &[u8; 8] = b"ZAMINFS2";
const HEADER: usize = 32; // sehrli söz 8 + avlod 8 + uzunlik 4 + nonce 12
const SUPER_BODY_MAX: usize = BLOCK - HEADER - 16;
/// Blok yöq (nollardan iborat "teşik")
pub const NO_BLOCK: u32 = u32::MAX;

pub type Nonce = [u8; 12];

#[derive(Clone, Copy)]
pub struct BlockRef {
    pub block: u32,
    pub nonce: Nonce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZfsError {
    NoDisk,
    /// Superblok bor, lekin oçilmadi (boşqa kalit yoki buzilgan)
    Corrupt,
    TooBig,
    Full,
    Io,
}

struct State {
    key: Option<[u8; 32]>,
    total: u32,
    bitmap: Vec<u64>,
    /// Oxirgi saqlangan holatga tegişli, lekin endi keraksiz bloklar —
    /// keyingi muvaffaqiyatli saqlaşdan keyin böşatiladi.
    pending_free: Vec<u32>,
    generation: u64,
    active_slot: u32,
    meta_blocks: Vec<u32>,
    write_blocked: bool,
    blocks_written: u64,
}

static ST: Mutex<State> = Mutex::new(State {
    key: None,
    total: 0,
    bitmap: Vec::new(),
    pending_free: Vec::new(),
    generation: 0,
    active_slot: 1,
    meta_blocks: Vec::new(),
    write_blocked: false,
    blocks_written: 0,
});

impl State {
    fn is_used(&self, b: u32) -> bool {
        self.bitmap[b as usize / 64] & (1 << (b % 64)) != 0
    }

    fn set_used(&mut self, b: u32, used: bool) {
        let w = &mut self.bitmap[b as usize / 64];
        if used {
            *w |= 1 << (b % 64);
        } else {
            *w &= !(1 << (b % 64));
        }
    }

    fn reset(&mut self, key: Option<[u8; 32]>) {
        if let Some(k) = self.key.as_mut() {
            zeroize(k);
        }
        self.key = key;
        self.total = ((disk::capacity_sectors().saturating_sub(FS_START)) / SECTORS_PER_BLOCK) as u32;
        self.bitmap = vec![0; (self.total as usize).div_ceil(64)];
        self.pending_free.clear();
        self.generation = 0;
        self.active_slot = 1;
        self.meta_blocks.clear();
        self.write_blocked = false;
        if self.total >= 2 {
            self.set_used(0, true);
            self.set_used(1, true);
        }
    }

    fn alloc(&mut self) -> Result<u32, ZfsError> {
        for (wi, w) in self.bitmap.iter().enumerate() {
            if *w != u64::MAX {
                let b = (wi * 64) as u32 + w.trailing_ones();
                if b < self.total {
                    self.set_used(b, true);
                    return Ok(b);
                }
            }
        }
        Err(ZfsError::Full)
    }
}

fn fs_key(master: &[u8; 32]) -> [u8; 32] {
    hmac_sha256(master, b"zaminfs2-blok-kaliti")
}

fn block_sector(b: u32) -> u64 {
    FS_START + b as u64 * SECTORS_PER_BLOCK
}

fn aad(kind: u8, block: u32) -> [u8; 9] {
    let mut a = [0u8; 9];
    a[0] = kind;
    a[1..5].copy_from_slice(&block.to_le_bytes());
    a[5..].copy_from_slice(b"ZFS2");
    a
}

const KIND_DATA: u8 = 1;
const KIND_META: u8 = 2;

fn seal_write(key: &[u8; 32], block: u32, kind: u8, plain: &[u8]) -> Result<Nonce, ZfsError> {
    let mut buf = vec![0u8; BLOCK];
    buf[16..16 + plain.len()].copy_from_slice(plain);
    let nonce = random::bytes::<12>();
    let tag = aead::seal(key, &nonce, &aad(kind, block), &mut buf[16..]);
    buf[..16].copy_from_slice(&tag);
    disk::write(block_sector(block), &buf).map_err(|_| ZfsError::Io)?;
    Ok(nonce)
}

fn read_open(key: &[u8; 32], r: BlockRef, kind: u8) -> Result<Vec<u8>, ZfsError> {
    let mut buf = vec![0u8; BLOCK];
    disk::read(block_sector(r.block), &mut buf).map_err(|_| ZfsError::Io)?;
    let tag: [u8; 16] = buf[..16].try_into().unwrap();
    let mut data = buf.split_off(16);
    if !aead::open(key, &r.nonce, &aad(kind, r.block), &mut data, &tag) {
        return Err(ZfsError::Corrupt);
    }
    Ok(data)
}

/// Kalit bor va disk tayyor.
pub fn ready() -> bool {
    disk::present() && ST.lock().key.is_some()
}

/// Yangi (böş) fayl tizimi: kalit örnatiladi; diskka birinçi saqlaşda yoziladi.
pub fn format(master: &[u8; 32]) {
    ST.lock().reset(Some(fs_key(master)));
}

/// Superblokni öqiydi va tekşiradi. Natija: (avlod, metadata uzunligi, metadata bloklari).
fn read_super(key: &[u8; 32], slot: u32) -> Option<Result<(u64, usize, Vec<BlockRef>), ZfsError>> {
    let mut buf = vec![0u8; BLOCK];
    disk::read(block_sector(slot), &mut buf).ok()?;
    if &buf[..8] != MAGIC {
        return None;
    }
    let generation = u64::from_le_bytes(buf[8..16].try_into().unwrap());
    let len = u32::from_le_bytes(buf[16..20].try_into().unwrap()) as usize;
    let nonce: Nonce = buf[20..32].try_into().unwrap();
    let tag: [u8; 16] = buf[32..48].try_into().unwrap();
    if len > SUPER_BODY_MAX {
        return Some(Err(ZfsError::Corrupt));
    }
    let header: [u8; HEADER] = buf[..HEADER].try_into().unwrap();
    let mut body = buf[48..48 + len].to_vec();
    if !aead::open(key, &nonce, &header, &mut body, &tag) {
        return Some(Err(ZfsError::Corrupt));
    }
    let meta_len = u64::from_le_bytes(body.get(4..12)?.try_into().ok()?) as usize;
    let count = u32::from_le_bytes(body.get(12..16)?.try_into().ok()?) as usize;
    let mut refs = Vec::with_capacity(count);
    for i in 0..count {
        let e = body.get(16 + i * 16..32 + i * 16)?;
        refs.push(BlockRef { block: u32::from_le_bytes(e[..4].try_into().ok()?), nonce: e[4..16].try_into().ok()? });
    }
    Some(Ok((generation, meta_len, refs)))
}

/// Diskdagi eng yangi butun holatni topadi. Natija: metadata (`None` — disk böş).
/// Maʼlumot bloklari `mark_used` orqali (metadata öqilganda) band deb belgilanadi.
pub fn mount(master: &[u8; 32]) -> Result<Option<Vec<u8>>, ZfsError> {
    if !disk::present() {
        return Err(ZfsError::NoDisk);
    }
    let key = fs_key(master);
    let mut st = ST.lock();
    st.reset(Some(key));
    let mut candidates: Vec<(u32, (u64, usize, Vec<BlockRef>))> = Vec::new();
    let mut any = false;
    for slot in 0..2 {
        match read_super(&key, slot) {
            None => {}
            Some(Err(_)) => any = true,
            Some(Ok(sb)) => {
                any = true;
                candidates.push((slot, sb));
            }
        }
    }
    candidates.sort_by_key(|(_, sb)| core::cmp::Reverse(sb.0));
    for (slot, (generation, meta_len, refs)) in candidates {
        let mut meta = Vec::with_capacity(refs.len() * BLOCK_DATA);
        let mut ok = true;
        for r in &refs {
            if r.block >= st.total {
                ok = false;
                break;
            }
            match read_open(&key, *r, KIND_META) {
                Ok(d) => meta.extend_from_slice(&d),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || meta.len() < meta_len {
            continue; // bu nusxa buzilgan — oldingisini sinaymiz
        }
        meta.truncate(meta_len);
        st.generation = generation;
        st.active_slot = slot;
        st.meta_blocks = refs.iter().map(|r| r.block).collect();
        for r in refs {
            st.set_used(r.block, true);
        }
        return Ok(Some(meta));
    }
    if any {
        st.write_blocked = true;
        return Err(ZfsError::Corrupt);
    }
    Ok(None)
}

/// Metadata buzilgan bölsa: diskka yoziş töxtatiladi (maʼlumot ustidan yozilmasin).
pub fn block_writes() {
    ST.lock().write_blocked = true;
}

/// Metadata öqilganda: blok band.
pub fn mark_used(block: u32) -> bool {
    let mut st = ST.lock();
    if block >= st.total || st.is_used(block) {
        return false; // diskdan taşqarida yoki ikki marta işlatilgan — buzilgan metadata
    }
    st.set_used(block, true);
    true
}

/// Maʼlumot blokini öqiydi va tekşiradi (natija — `BLOCK_DATA` bayt).
pub fn read_data(r: BlockRef) -> Result<Vec<u8>, ZfsError> {
    let key = ST.lock().key.ok_or(ZfsError::NoDisk)?;
    read_open(&key, r, KIND_DATA)
}

/// Yangi blokka şifrlab yozadi (eski blok ustiga emas).
pub fn write_data(plain: &[u8]) -> Result<BlockRef, ZfsError> {
    let (key, block) = {
        let mut st = ST.lock();
        if st.write_blocked {
            return Err(ZfsError::Corrupt);
        }
        let key = st.key.ok_or(ZfsError::NoDisk)?;
        (key, st.alloc()?)
    };
    match seal_write(&key, block, KIND_DATA, plain) {
        Ok(nonce) => {
            ST.lock().blocks_written += 1;
            Ok(BlockRef { block, nonce })
        }
        Err(e) => {
            ST.lock().set_used(block, false);
            Err(e)
        }
    }
}

/// Blok endi keraksiz — keyingi saqlaşdan keyin böşatiladi.
pub fn release(block: u32) {
    if block != NO_BLOCK {
        ST.lock().pending_free.push(block);
    }
}

/// Metadata ni yozib, superblokni almaştiradi. Natija: şu saqlaşda yozilgan bloklar soni.
pub fn commit(meta: &[u8]) -> Result<u64, ZfsError> {
    let (key, written_before) = {
        let st = ST.lock();
        if st.write_blocked {
            return Err(ZfsError::Corrupt);
        }
        (st.key.ok_or(ZfsError::NoDisk)?, st.blocks_written)
    };
    let max_meta_blocks = (SUPER_BODY_MAX - 16) / 16;
    let chunks: Vec<&[u8]> = meta.chunks(BLOCK_DATA).collect();
    if chunks.len() > max_meta_blocks {
        return Err(ZfsError::TooBig);
    }
    let mut refs = Vec::with_capacity(chunks.len());
    for c in chunks {
        let block = ST.lock().alloc()?;
        let nonce = seal_write(&key, block, KIND_META, c)?;
        refs.push(BlockRef { block, nonce });
    }

    let st = ST.lock();
    let mut body = Vec::with_capacity(16 + refs.len() * 16);
    body.extend_from_slice(&st.total.to_le_bytes());
    body.extend_from_slice(&(meta.len() as u64).to_le_bytes());
    body.extend_from_slice(&(refs.len() as u32).to_le_bytes());
    for r in &refs {
        body.extend_from_slice(&r.block.to_le_bytes());
        body.extend_from_slice(&r.nonce);
    }
    let generation = st.generation + 1;
    let slot = 1 - st.active_slot;
    let mut buf = vec![0u8; BLOCK];
    buf[..8].copy_from_slice(MAGIC);
    buf[8..16].copy_from_slice(&generation.to_le_bytes());
    buf[16..20].copy_from_slice(&(body.len() as u32).to_le_bytes());
    let nonce = random::bytes::<12>();
    buf[20..32].copy_from_slice(&nonce);
    let header: [u8; HEADER] = buf[..HEADER].try_into().unwrap();
    let tag = aead::seal(&key, &nonce, &header, &mut body);
    buf[32..48].copy_from_slice(&tag);
    buf[48..48 + body.len()].copy_from_slice(&body);
    drop(st);
    disk::write(block_sector(slot), &buf).map_err(|_| ZfsError::Io)?;

    // Saqlandi. Endi eski metadata va almaştirilgan bloklarni böşatiş mumkin.
    let mut st = ST.lock();
    let old_meta = core::mem::take(&mut st.meta_blocks);
    let pending = core::mem::take(&mut st.pending_free);
    for b in old_meta.into_iter().chain(pending) {
        if b < st.total {
            st.set_used(b, false);
        }
    }
    st.meta_blocks = refs.iter().map(|r| r.block).collect();
    st.generation = generation;
    st.active_slot = slot;
    st.blocks_written += refs.len() as u64 + 1;
    Ok(st.blocks_written - written_before)
}

/// Ikkala superblokni öçiradi va kalitni unutadi ("kripto-öçiriş").
pub fn wipe() -> bool {
    ST.lock().reset(None);
    if !disk::present() {
        return false;
    }
    let zero = vec![0u8; BLOCK];
    disk::write(block_sector(0), &zero).is_ok() && disk::write(block_sector(1), &zero).is_ok()
}

/// (avlod, faol nusxa, band bloklar, jami bloklar)
pub fn state() -> (u64, u32, u32, u32) {
    let st = ST.lock();
    let used = st.bitmap.iter().map(|w| w.count_ones()).sum();
    (st.generation, st.active_slot, used, st.total)
}
