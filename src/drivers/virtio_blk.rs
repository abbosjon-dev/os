//! virtio-blk disk drayveri. DMA buferlari identity xaritalangan RAMʼda (PA == VA).
//!
//! Söröv yuborilgaç vazifa uxlaydi; disk tugatganda uziliş uni uyğotadi — protsessor
//! boşqa işlar bilan band böla oladi. Qurilma uxlaydigan qulf (`Mutex`) bilan himoyalangan.
//! Rejalaştiruvçi işga tuşmagan paytda (yuklaniş) javob aylanib kutiladi.

use super::virtio::{DEVICE_BLOCK, Transport};
use crate::memory::{PAGE_SIZE, frame};
use crate::sync::Mutex;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

pub const SECTOR: usize = 512;
const REQ_IN: u32 = 0;
const REQ_OUT: u32 = 1;
/// Bitta sörövda eng köp sektor (bitta DMA sahifasi).
const MAX_SECTORS: usize = PAGE_SIZE / SECTOR;

#[derive(Debug)]
#[allow(dead_code)] // `{:?}` orqali körsatiladi
pub enum DiskError {
    NoDisk,
    OutOfRange,
    Io,
}

struct VirtioBlk {
    t: Transport,
    capacity: u64,
    /// DMA sahifasi: söröv sarlavhasi (0) va holat bayti (16)
    req: usize,
    /// Maʼlumot uçun DMA sahifasi
    data: usize,
    /// Bölim boşi (GPT dagi Zamin bölimi; butun disk bölsa 0) — barça sektor raqamlariga qöşiladi
    base: u64,
}

static DISK: Mutex<Option<VirtioBlk>> = Mutex::new(None);
/// Uzilişlar işlovçisi uçun (u qulfni ola olmaydi)
static DISK_BASE: AtomicUsize = AtomicUsize::new(0);
static DISK_IRQ: AtomicU32 = AtomicU32::new(0);
static CAPACITY: AtomicU64 = AtomicU64::new(0);
/// Tizim diski qaysi indeksda (FAT diski boşqasini oladi)
static SYS_INDEX: AtomicU32 = AtomicU32::new(0);
/// GPT Zamin bölimi: (raqami 1.., boşlaniş LBA) — butun disk bölsa raqam 0
static PART_NUM: AtomicU32 = AtomicU32::new(0);
static PART_BASE: AtomicU64 = AtomicU64::new(0);
/// Disk uzilişlari soni (tekşiruv uçun)
static IRQ_COUNT: AtomicU64 = AtomicU64::new(0);
/// `task::wait_event` kaliti
const DISK_EVENT: usize = 0xd15c;

/// Blok qurilmasining bitta sektorini öqiydi (sinflaş uçun, vaqtinça transport bilan).
fn peek_sector(t: &mut Transport, scratch: usize, lba: u64) -> [u8; SECTOR] {
    unsafe {
        write_volatile(scratch as *mut u32, 0); // REQ_IN
        write_volatile((scratch + 4) as *mut u32, 0);
        write_volatile((scratch + 8) as *mut u64, lba);
        write_volatile((scratch + 16) as *mut u8, 0xff);
    }
    let dbuf = scratch + 512;
    t.submit(&[(scratch, 16, false), (dbuf, SECTOR, true), (scratch + 16, 1, true)]);
    let mut b = [0u8; SECTOR];
    if unsafe { read_volatile((scratch + 16) as *const u8) } == 0 {
        unsafe { core::ptr::copy_nonoverlapping(dbuf as *const u8, b.as_mut_ptr(), SECTOR) };
    }
    b
}

/// Zamin maʼlumot bölimining GPT tur GUID i: 7A616D69-6E00-4F53-9A4D-494E44534B31
/// (diskda "aralaş endian" köriniş: birinçi uç maydon kiçik endian).
pub const ZAMIN_PART_GUID: [u8; 16] = [
    0x69, 0x6d, 0x61, 0x7a, 0x00, 0x6e, 0x53, 0x4f, 0x9a, 0x4d, 0x49, 0x4e, 0x44, 0x53, 0x4b, 0x31,
];

fn crc32_update(mut c: u32, data: &[u8]) -> u32 {
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    c
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn le64(b: &[u8], o: usize) -> u64 {
    le32(b, o) as u64 | (le32(b, o + 4) as u64) << 32
}

#[derive(Debug, PartialEq, Eq)]
pub enum Gpt {
    /// GPT yöq (sektor 1 da "EFI PART" yöq yoki nazorat summasi xato)
    None,
    /// GPT bor, lekin Zamin bölimi yöq (masalan, boşqa tizim diski) — tegmaymiz
    Foreign,
    /// Zamin bölimi: raqami (1..), birinçi LBA, sektorlar soni
    Zamin(u32, u64, u64),
}

/// GPT sarlavhasini (sektor 1) va bölimlar jadvalini öqib, Zamin bölimini qidiradi.
/// `read(lba)` — bitta sektor öqiydi. Sarlavha va jadval CRC32 lari tekşiriladi.
pub fn parse_gpt(capacity: u64, mut read: impl FnMut(u64) -> [u8; SECTOR]) -> Gpt {
    let h = read(1);
    if &h[..8] != b"EFI PART" {
        return Gpt::None;
    }
    let hsize = le32(&h, 12) as usize;
    if !(92..=SECTOR).contains(&hsize) {
        return Gpt::None;
    }
    let mut hc = h;
    hc[16..20].fill(0);
    if crc32_update(!0, &hc[..hsize]) ^ !0 != le32(&h, 16) {
        return Gpt::None;
    }
    let (first_usable, last_usable) = (le64(&h, 40), le64(&h, 48));
    let (entries_lba, count, esize) = (le64(&h, 72), le32(&h, 80) as usize, le32(&h, 84) as usize);
    if esize < 128 || esize % 8 != 0 || SECTOR % esize != 0 || count == 0 || count * esize > 64 * SECTOR {
        return Gpt::Foreign;
    }
    let per = SECTOR / esize;
    let mut crc = !0u32;
    let mut found = Gpt::Foreign;
    let nsec = (count * esize).div_ceil(SECTOR);
    for si in 0..nsec {
        let s = read(entries_lba + si as u64);
        let in_sec = per.min(count - si * per);
        crc = crc32_update(crc, &s[..in_sec * esize]);
        for k in 0..in_sec {
            let e = &s[k * esize..(k + 1) * esize];
            if e[..16] != ZAMIN_PART_GUID || found != Gpt::Foreign {
                continue;
            }
            let (first, last) = (le64(e, 32), le64(e, 40));
            if first >= first_usable && first <= last && last <= last_usable && last < capacity {
                found = Gpt::Zamin((si * per + k + 1) as u32, first, last - first + 1);
            }
        }
    }
    if crc ^ !0 != le32(&h, 88) {
        return Gpt::Foreign;
    }
    found
}

/// Tizim diskini tanlaydi. Tartibga tayanmaymiz (QEMU qurilmalarni teskari joylaştirişi
/// mumkin), mazmunga qaraymiz:
///   1) "ZAMINDSK" belgili butun disk (eski -kernel yöli);
///   2) GPT dagi Zamin maʼlumot bölimi (VPS obrazi: ESP + Zamin bölimi bitta diskda);
///   3) eng katta böş disk. Boşqa GPT disklarga (Zamin bölimisiz) heç qaçon yozilmaydi.
pub fn init() -> Option<u64> {
    let scratch = frame::alloc_zeroed()?;
    // (indeks, siğim, bölim boşi, ustuvorlik 0..2)
    let mut best: Option<(usize, u64, u64, u32, u8)> = None;
    let mut n = 0;
    while let Some(mut t) = Transport::probe_nth(DEVICE_BLOCK, n) {
        let cap = t.config_u32(0) as u64 | (t.config_u32(4) as u64) << 32;
        let s0 = peek_sector(&mut t, scratch, 0);
        let cand = if &s0[..8] == b"ZAMINDSK" {
            Some((n, cap, 0, 0, 2))
        } else {
            match parse_gpt(cap, |lba| peek_sector(&mut t, scratch, lba)) {
                Gpt::Zamin(num, first, count) => Some((n, count, first, num, 1)),
                Gpt::Foreign => None,
                Gpt::None => Some((n, cap, 0, 0, 0)),
            }
        };
        if let Some(c) = cand {
            let better = match best {
                None => true,
                Some(b) => c.4 > b.4 || (c.4 == b.4 && c.4 == 0 && c.1 > b.1),
            };
            if better {
                best = Some(c);
            }
            if c.4 == 2 {
                break; // aniq tizim diski
            }
        }
        n += 1;
        if n >= 8 {
            break;
        }
    }
    frame::free(scratch);
    let (idx, capacity, base, num, _) = best?;
    let t = Transport::probe_nth(DEVICE_BLOCK, idx)?;
    SYS_INDEX.store(idx as u32, Ordering::Release);
    PART_NUM.store(num, Ordering::Release);
    PART_BASE.store(base, Ordering::Release);
    let req = frame::alloc_zeroed()?;
    let data = frame::alloc_zeroed()?;
    DISK_BASE.store(t.ack_handle(), Ordering::Release);
    DISK_IRQ.store(t.irq(), Ordering::Release);
    t.enable_irq();
    crate::gic::enable(t.irq());
    *DISK.lock() = Some(VirtioBlk { t, capacity, req, data, base });
    CAPACITY.store(capacity, Ordering::Release);
    Some(capacity * SECTOR as u64)
}

/// Tizim GPT bölimida bölsa: (bölim raqami, boşlaniş LBA). Butun disk bölsa — None.
pub fn partition() -> Option<(u32, u64)> {
    match PART_NUM.load(Ordering::Acquire) {
        0 => None,
        n => Some((n, PART_BASE.load(Ordering::Acquire))),
    }
}

pub fn present() -> bool {
    CAPACITY.load(Ordering::Acquire) != 0
}

// --- Ikkinçi disk (masalan, FAT): faqat öqiş, sörov qilib (polled) ---
struct FatDisk {
    t: super::virtio::Transport,
    capacity: u64,
    req: usize,
    data: usize,
}
unsafe impl Send for FatDisk {}
static FAT_DISK: Mutex<Option<FatDisk>> = Mutex::new(None);

/// Ikkinçi blok qurilmasini işga tuşiradi. Natija: siğim (sektorlar).
pub fn init_fat() -> Option<u64> {
    let sys = SYS_INDEX.load(Ordering::Acquire) as usize;
    let fat_idx = if sys == 0 { 1 } else { 0 }; // tizim diskidan boşqasi
    let t = super::virtio::Transport::probe_nth(super::virtio::DEVICE_BLOCK, fat_idx)?;
    let capacity = t.config_u32(0) as u64 | (t.config_u32(4) as u64) << 32;
    let req = frame::alloc_zeroed()?;
    let data = frame::alloc_zeroed()?;
    *FAT_DISK.lock() = Some(FatDisk { t, capacity, req, data });
    Some(capacity)
}

#[allow(dead_code)]
pub fn fat_present() -> bool {
    FAT_DISK.lock().is_some()
}

/// Ikkinçi (FAT) disk siğimi — sektorlarda (disk bölmasa 0).
pub fn fat_capacity_sectors() -> u64 {
    FAT_DISK.lock().as_ref().map_or(0, |d| d.capacity)
}

/// Ikkinçi diskdan bitta sektor öqiydi (512 bayt).
pub fn fat_read(sector: u64, buf: &mut [u8; SECTOR]) -> Result<(), DiskError> {
    let mut g = FAT_DISK.lock();
    let d = g.as_mut().ok_or(DiskError::NoDisk)?;
    if sector >= d.capacity {
        return Err(DiskError::OutOfRange);
    }
    unsafe {
        write_volatile(d.req as *mut u32, 0); // REQ_IN
        write_volatile((d.req + 4) as *mut u32, 0);
        write_volatile((d.req + 8) as *mut u64, sector);
        write_volatile((d.req + 16) as *mut u8, 0xff);
    }
    d.t.submit(&[(d.req, 16, false), (d.data, SECTOR, true), (d.req + 16, 1, true)]);
    if unsafe { read_volatile((d.req + 16) as *const u8) } != 0 {
        return Err(DiskError::Io);
    }
    unsafe { core::ptr::copy_nonoverlapping(d.data as *const u8, buf.as_mut_ptr(), SECTOR) };
    Ok(())
}

/// Ikkinçi diskka bitta sektor yozadi (512 bayt).
pub fn fat_write(sector: u64, buf: &[u8; SECTOR]) -> Result<(), DiskError> {
    let mut g = FAT_DISK.lock();
    let d = g.as_mut().ok_or(DiskError::NoDisk)?;
    if sector >= d.capacity {
        return Err(DiskError::OutOfRange);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(buf.as_ptr(), d.data as *mut u8, SECTOR);
        write_volatile(d.req as *mut u32, 1); // REQ_OUT (yoziş)
        write_volatile((d.req + 4) as *mut u32, 0);
        write_volatile((d.req + 8) as *mut u64, sector);
        write_volatile((d.req + 16) as *mut u8, 0xff);
    }
    // maʼlumot bufer qurilma uçun öqiladigan (false), holat — yoziladigan (true)
    d.t.submit(&[(d.req, 16, false), (d.data, SECTOR, false), (d.req + 16, 1, true)]);
    if unsafe { read_volatile((d.req + 16) as *const u8) } != 0 {
        return Err(DiskError::Io);
    }
    Ok(())
}

/// Sektorlar soni.
pub fn capacity_sectors() -> u64 {
    CAPACITY.load(Ordering::Acquire)
}

/// Diskning GIC uzilişi (disk bölmasa — 0).
pub fn irq() -> u32 {
    DISK_IRQ.load(Ordering::Acquire)
}

pub fn irq_count() -> u64 {
    IRQ_COUNT.load(Ordering::Relaxed)
}

/// Uzilişlar işlovçisi: qurilmani tasdiqlab, kutayotgan vazifani uyğotadi.
pub fn handle_irq() {
    super::virtio::ack_interrupt(DISK_BASE.load(Ordering::Acquire));
    IRQ_COUNT.fetch_add(1, Ordering::Relaxed);
    crate::task::wake_event(DISK_EVENT);
}

impl VirtioBlk {
    fn request(&mut self, kind: u32, sector: u64, count: usize) -> Result<(), DiskError> {
        if sector + count as u64 > self.capacity {
            return Err(DiskError::OutOfRange);
        }
        unsafe {
            write_volatile(self.req as *mut u32, kind);
            write_volatile((self.req + 4) as *mut u32, 0);
            write_volatile((self.req + 8) as *mut u64, self.base + sector);
            write_volatile((self.req + 16) as *mut u8, 0xff);
        }
        self.t.submit_async(&[(self.req, 16, false), (self.data, count * SECTOR, kind == REQ_IN), (self.req + 16, 1, true)]);
        // Disk tugatgunça uxlaymiz (şart rejalaştiruvçi qulfi ostida tekşiriladi — uyğotiş yöqolmaydi)
        let used = self.t.used_idx_addr();
        let last = self.t.last_used();
        crate::task::wait_event(DISK_EVENT, || unsafe { read_volatile(used as *const u16) } == last);
        self.t.complete();
        if unsafe { read_volatile((self.req + 16) as *const u8) } != 0 {
            return Err(DiskError::Io);
        }
        Ok(())
    }
}

/// Panic paytida: qulfsiz va uzilişsiz (javobni aylanib kutib) bitta sektor yozadi.
pub fn write_polled(sector: u64, data: &[u8]) -> bool {
    let Some(d) = (unsafe { DISK.force_get() }).as_mut() else { return false };
    if sector >= d.capacity || data.len() > SECTOR {
        return false;
    }
    unsafe {
        core::ptr::write_bytes(d.data as *mut u8, 0, SECTOR);
        core::ptr::copy_nonoverlapping(data.as_ptr(), d.data as *mut u8, data.len());
        write_volatile(d.req as *mut u32, REQ_OUT);
        write_volatile((d.req + 4) as *mut u32, 0);
        write_volatile((d.req + 8) as *mut u64, d.base + sector);
        write_volatile((d.req + 16) as *mut u8, 0xff);
    }
    d.t.submit(&[(d.req, 16, false), (d.data, SECTOR, false), (d.req + 16, 1, true)]);
    unsafe { read_volatile((d.req + 16) as *const u8) == 0 }
}

/// `sector` dan boşlab öqiydi. `buf` uzunligi 512 ga karrali bölişi kerak.
pub fn read(sector: u64, buf: &mut [u8]) -> Result<(), DiskError> {
    let mut guard = DISK.lock();
    let d = guard.as_mut().ok_or(DiskError::NoDisk)?;
    for (i, chunk) in buf.chunks_mut(MAX_SECTORS * SECTOR).enumerate() {
        d.request(REQ_IN, sector + (i * MAX_SECTORS) as u64, chunk.len().div_ceil(SECTOR))?;
        unsafe { core::ptr::copy_nonoverlapping(d.data as *const u8, chunk.as_mut_ptr(), chunk.len()) };
    }
    Ok(())
}

/// `sector` dan boşlab yozadi. `buf` uzunligi 512 ga karrali bölişi kerak.
pub fn write(sector: u64, buf: &[u8]) -> Result<(), DiskError> {
    let mut guard = DISK.lock();
    let d = guard.as_mut().ok_or(DiskError::NoDisk)?;
    for (i, chunk) in buf.chunks(MAX_SECTORS * SECTOR).enumerate() {
        unsafe {
            core::ptr::write_bytes(d.data as *mut u8, 0, PAGE_SIZE);
            core::ptr::copy_nonoverlapping(chunk.as_ptr(), d.data as *mut u8, chunk.len());
        }
        d.request(REQ_OUT, sector + (i * MAX_SECTORS) as u64, chunk.len().div_ceil(SECTOR))?;
    }
    Ok(())
}

/// GPT tahlilçisi sinovi: xotirada yasalgan disk (ESP + Zamin bölimi), buzilgan sarlavha,
/// buzilgan jadval, Zamin bölimisiz GPT. Natija: ötgan sinovlar soni.
pub fn self_test_gpt() -> Result<usize, &'static str> {
    use alloc::vec;
    use alloc::vec::Vec;
    const CAP: u64 = 4096;
    fn build(with_zamin: bool) -> Vec<[u8; SECTOR]> {
        let mut d = vec![[0u8; SECTOR]; 34];
        let esp: [u8; 16] = [0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b];
        let mut put = |i: usize, ty: &[u8; 16], first: u64, last: u64| {
            let (s, o) = (2 + i * 128 / SECTOR, i * 128 % SECTOR);
            d[s][o..o + 16].copy_from_slice(ty);
            d[s][o + 16] = i as u8 + 1; // noyob GUID
            d[s][o + 32..o + 40].copy_from_slice(&first.to_le_bytes());
            d[s][o + 40..o + 48].copy_from_slice(&last.to_le_bytes());
        };
        put(0, &esp, 64, 1023);
        if with_zamin {
            put(1, &ZAMIN_PART_GUID, 1024, CAP - 34);
        }
        let mut crc = !0u32;
        for s in &d[2..34] {
            crc = crc32_update(crc, s);
        }
        let h = &mut d[1];
        h[..8].copy_from_slice(b"EFI PART");
        h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        h[12..16].copy_from_slice(&92u32.to_le_bytes());
        h[24..32].copy_from_slice(&1u64.to_le_bytes());
        h[32..40].copy_from_slice(&(CAP - 1).to_le_bytes());
        h[40..48].copy_from_slice(&34u64.to_le_bytes());
        h[48..56].copy_from_slice(&(CAP - 34).to_le_bytes());
        h[72..80].copy_from_slice(&2u64.to_le_bytes());
        h[80..84].copy_from_slice(&128u32.to_le_bytes());
        h[84..88].copy_from_slice(&128u32.to_le_bytes());
        h[88..92].copy_from_slice(&(crc ^ !0).to_le_bytes());
        let hc = crc32_update(!0, &h[..92]) ^ !0;
        h[16..20].copy_from_slice(&hc.to_le_bytes());
        d
    }
    let rd = |d: &Vec<[u8; SECTOR]>| {
        let d = d.clone();
        move |lba: u64| d.get(lba as usize).copied().unwrap_or([0; SECTOR])
    };
    let mut ok = 0;
    let good = build(true);
    if parse_gpt(CAP, rd(&good)) != Gpt::Zamin(2, 1024, CAP - 34 - 1024 + 1) {
        return Err("Zamin bölimi topilmadi");
    }
    ok += 1;
    if parse_gpt(CAP, rd(&build(false))) != Gpt::Foreign {
        return Err("Zamin bölimisiz GPT Foreign emas");
    }
    ok += 1;
    let mut bad_h = good.clone();
    bad_h[1][40] ^= 1;
    if parse_gpt(CAP, rd(&bad_h)) != Gpt::None {
        return Err("sarlavha CRC xatosi aniqlanmadi");
    }
    ok += 1;
    let mut bad_e = good.clone();
    bad_e[2][128 + 33] ^= 1; // Zamin bölimi boşini özgartiramiz
    if parse_gpt(CAP, rd(&bad_e)) != Gpt::Foreign {
        return Err("jadval CRC xatosi aniqlanmadi");
    }
    ok += 1;
    if parse_gpt(CAP, rd(&vec![[0u8; SECTOR]; 34])) != Gpt::None {
        return Err("böş disk GPT deb topildi");
    }
    ok += 1;
    // bölim disk siğimidan çiqsa — qabul qilinmaydi
    if parse_gpt(CAP - 100, rd(&good)) != Gpt::Foreign {
        return Err("siğimdan çiqqan bölim qabul qilindi");
    }
    Ok(ok + 1)
}
