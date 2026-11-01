//! virtio-blk disk drayveri. DMA buferlari identity xaritalangan RAMʼda (PA == VA).

use super::virtio::{DEVICE_BLOCK, Transport};
use crate::memory::{PAGE_SIZE, frame};
use crate::sync::SpinLock;
use core::ptr::{read_volatile, write_volatile};

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

static DISK: SpinLock<Option<VirtioBlk>> = SpinLock::new(None);

/// Diskni qidiradi va işga tuşiradi. Natija: siğim (baytlarda).
pub fn init() -> Option<u64> {
    let t = Transport::probe(DEVICE_BLOCK)?;
    let capacity = t.config_u32(0) as u64 | (t.config_u32(4) as u64) << 32;
    let req = frame::alloc_zeroed()?;
    let data = frame::alloc_zeroed()?;
    *DISK.lock() = Some(VirtioBlk { t, capacity, req, data });
    Some(capacity * SECTOR as u64)
}

pub fn present() -> bool {
    DISK.lock().is_some()
}

/// Sektorlar soni.
pub fn capacity_sectors() -> u64 {
    DISK.lock().as_ref().map_or(0, |d| d.capacity)
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
        self.t.submit(&[(self.req, 16, false), (self.data, count * SECTOR, kind == REQ_IN), (self.req + 16, 1, true)]);
        if unsafe { read_volatile((self.req + 16) as *const u8) } != 0 {
            return Err(DiskError::Io);
        }
        Ok(())
    }
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
