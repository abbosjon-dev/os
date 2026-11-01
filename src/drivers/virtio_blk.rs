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
}

static DISK: Mutex<Option<VirtioBlk>> = Mutex::new(None);
/// Uzilişlar işlovçisi uçun (u qulfni ola olmaydi)
static DISK_BASE: AtomicUsize = AtomicUsize::new(0);
static DISK_IRQ: AtomicU32 = AtomicU32::new(0);
static CAPACITY: AtomicU64 = AtomicU64::new(0);
/// Disk uzilişlari soni (tekşiruv uçun)
static IRQ_COUNT: AtomicU64 = AtomicU64::new(0);
/// `task::wait_event` kaliti
const DISK_EVENT: usize = 0xd15c;

/// Diskni qidiradi va işga tuşiradi. Natija: siğim (baytlarda).
pub fn init() -> Option<u64> {
    let t = Transport::probe(DEVICE_BLOCK)?;
    let capacity = t.config_u32(0) as u64 | (t.config_u32(4) as u64) << 32;
    let req = frame::alloc_zeroed()?;
    let data = frame::alloc_zeroed()?;
    DISK_BASE.store(t.base(), Ordering::Release);
    DISK_IRQ.store(t.irq(), Ordering::Release);
    crate::gic::enable(t.irq());
    *DISK.lock() = Some(VirtioBlk { t, capacity, req, data });
    CAPACITY.store(capacity, Ordering::Release);
    Some(capacity * SECTOR as u64)
}

pub fn present() -> bool {
    CAPACITY.load(Ordering::Acquire) != 0
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
            write_volatile((self.req + 8) as *mut u64, sector);
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
        write_volatile((d.req + 8) as *mut u64, sector);
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
