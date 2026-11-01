//! ZaminFS — diskdagi şifrlangan fayl tizimi (1-versiya).
//!
//! Dizayn:
//!   - foydalanuvçi fayllari (uy papkalari) bitta tasvirga yiğiladi va
//!     ChaCha20-Poly1305 bilan şifrlanadi; kalit — hisobning asosiy kaliti;
//!   - har bir yozuvda yangi tasodifiy nonce (bir xil nonce heç qaçon takrorlanmaydi);
//!   - diskda ikki nusxa (A/B) va avlod raqami: yangi nusxa eski nusxa ustiga emas,
//!     ikkinçi joyga yoziladi, sarlavha eng oxirida. Yoziş yarmida tok öçsa ham
//!     oxirgi butun nusxa saqlanib qoladi;
//!   - sarlavha (sehrli söz, avlod, uzunlik) AEAD qöşimça maʼlumoti sifatida
//!     himoyalangan: bitta bayt özgartirilsa ham nusxa rad etiladi.
//!
//! Keyingi versiyada — bloklarga asoslangan tuzilma (katta fayllar uçun).

use crate::crypto::aead;
use crate::drivers::virtio_blk::{self as disk, SECTOR};
use crate::random;
use alloc::vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const FS_START: u64 = 64;
const MAGIC: &[u8; 8] = b"ZAMINFS1";

/// Oxirgi muvaffaqiyatli nusxaning avlodi va joyi (0 = A, 1 = B).
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACTIVE_SLOT: AtomicU64 = AtomicU64::new(1);
/// Diskdagi nusxalarni oçib bölmagan bölsa — ularni ustidan yozmaymiz
/// (maʼlumotni tiklaş imkoniyati yöqolmasin). `tiklaş` bilan tozalanadi.
static WRITE_BLOCKED: AtomicBool = AtomicBool::new(false);
/// Bir vaqtda faqat bitta saqlaş (masalan, fon oqimi va `saqla` turli yadrolarda).
static SYNC: crate::sync::Mutex<()> = crate::sync::Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZfsError {
    NoDisk,
    /// Nusxa bor, lekin kalit mos emas yoki maʼlumot buzilgan
    Corrupt,
    TooBig,
    Io,
}

struct Header {
    generation: u64,
    len: u64,
    nonce: [u8; 12],
    tag: [u8; 16],
    aad: [u8; 24],
}

fn slot_sectors() -> u64 {
    disk::capacity_sectors().saturating_sub(FS_START) / 2
}

fn slot_start(slot: u64) -> u64 {
    FS_START + slot * slot_sectors()
}

fn read_header(slot: u64) -> Option<Header> {
    let mut s = [0u8; SECTOR];
    disk::read(slot_start(slot), &mut s).ok()?;
    if &s[..8] != MAGIC {
        return None;
    }
    Some(Header {
        generation: u64::from_le_bytes(s[8..16].try_into().ok()?),
        len: u64::from_le_bytes(s[16..24].try_into().ok()?),
        nonce: s[24..36].try_into().ok()?,
        tag: s[36..52].try_into().ok()?,
        aad: s[..24].try_into().ok()?,
    })
}

/// Diskdagi eng yangi butun nusxani oçib, fayllarni tiklaydi.
/// Natija: `Some(yozuvlar soni)`, yoki `None` — disk böş (hali saqlanmagan).
pub fn load(key: &[u8; 32]) -> Result<Option<usize>, ZfsError> {
    if !disk::present() {
        return Err(ZfsError::NoDisk);
    }
    let mut found = [(0u64, read_header(0)), (1u64, read_header(1))];
    found.sort_by_key(|(_, h)| core::cmp::Reverse(h.as_ref().map_or(0, |h| h.generation)));
    let mut any = false;
    for (slot, h) in found {
        let Some(h) = h else { continue };
        any = true;
        let max = (slot_sectors() - 1) * SECTOR as u64;
        if h.len > max {
            continue;
        }
        let mut data = vec![0u8; (h.len as usize).div_ceil(SECTOR) * SECTOR];
        if disk::read(slot_start(slot) + 1, &mut data).is_err() {
            continue;
        }
        data.truncate(h.len as usize);
        if !aead::open(key, &h.nonce, &h.aad, &mut data, &h.tag) {
            continue; // buzilgan yoki boşqa kalit — ikkinçi nusxani sinaymiz
        }
        let n = super::import(&data).map_err(|_| ZfsError::Corrupt)?;
        crate::crypto::zeroize(&mut data);
        GENERATION.store(h.generation, Ordering::Relaxed);
        ACTIVE_SLOT.store(slot, Ordering::Relaxed);
        return Ok(Some(n));
    }
    if any {
        WRITE_BLOCKED.store(true, Ordering::Relaxed);
        Err(ZfsError::Corrupt)
    } else {
        Ok(None)
    }
}

/// Fayllarni diskka şifrlab yozadi. Natija: yozilgan baytlar (şifrlangan tasvir).
pub fn sync(key: &[u8; 32]) -> Result<usize, ZfsError> {
    if !disk::present() {
        return Err(ZfsError::NoDisk);
    }
    if WRITE_BLOCKED.load(Ordering::Relaxed) {
        return Err(ZfsError::Corrupt);
    }
    let _guard = SYNC.lock();
    let mut data = super::export();
    let max = (slot_sectors().saturating_sub(1)) as usize * SECTOR;
    if data.len() > max {
        return Err(ZfsError::TooBig);
    }
    let generation = GENERATION.load(Ordering::Relaxed) + 1;
    let slot = 1 - ACTIVE_SLOT.load(Ordering::Relaxed); // eski nusxa ustiga yozamiz
    let mut header = [0u8; SECTOR];
    header[..8].copy_from_slice(MAGIC);
    header[8..16].copy_from_slice(&generation.to_le_bytes());
    header[16..24].copy_from_slice(&(data.len() as u64).to_le_bytes());
    let nonce = random::bytes::<12>();
    let aad: [u8; 24] = header[..24].try_into().unwrap();
    let tag = aead::seal(key, &nonce, &aad, &mut data);
    header[24..36].copy_from_slice(&nonce);
    header[36..52].copy_from_slice(&tag);

    let len = data.len();
    data.resize(len.div_ceil(SECTOR) * SECTOR, 0);
    // Avval maʼlumot, keyin sarlavha: sarlavha yozilmagunça eski nusxa amalda qoladi.
    disk::write(slot_start(slot) + 1, &data).map_err(|_| ZfsError::Io)?;
    disk::write(slot_start(slot), &header).map_err(|_| ZfsError::Io)?;
    GENERATION.store(generation, Ordering::Relaxed);
    ACTIVE_SLOT.store(slot, Ordering::Relaxed);
    Ok(len)
}

/// Ikkala nusxa sarlavhasini öçiradi.
pub fn wipe() -> bool {
    if !disk::present() || slot_sectors() == 0 {
        return false;
    }
    GENERATION.store(0, Ordering::Relaxed);
    ACTIVE_SLOT.store(1, Ordering::Relaxed);
    WRITE_BLOCKED.store(false, Ordering::Relaxed);
    disk::write(slot_start(0), &[0u8; SECTOR]).is_ok() && disk::write(slot_start(1), &[0u8; SECTOR]).is_ok()
}

/// (avlod, faol nusxa) — `xavfsizlik` buyruği uçun.
pub fn state() -> (u64, u64) {
    (GENERATION.load(Ordering::Relaxed), ACTIVE_SLOT.load(Ordering::Relaxed))
}
