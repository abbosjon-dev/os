//! virtio-blk disk drayveri (virtio-mmio transporti, eski v1 va zamonaviy v2 versiyalari).
//!
//! QEMU virt maşinasida 32 ta virtio-mmio uyaçasi bor (0x0a00_0000 dan, har biri 0x200).
//! Drayver ularni aylanib çiqib, birinçi blok qurilmasini topadi.
//! Hozirça sinxron rejim: söröv yuboriladi va tugaşi kutiladi (uziliş işlatilmaydi).
//! Barça DMA buferlari identity xaritalangan RAMʼda (PA == VA).

use crate::memory::{PAGE_SIZE, frame};
use crate::sync::SpinLock;
use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

pub const SECTOR: usize = 512;

const MMIO_BASE: usize = 0x0a00_0000;
const MMIO_STRIDE: usize = 0x200;
const MMIO_SLOTS: usize = 32;

// virtio-mmio registrlari
const MAGIC: usize = 0x000; // "virt"
const VERSION: usize = 0x004;
const DEVICE_ID: usize = 0x008;
const DEVICE_FEATURES: usize = 0x010;
const DEVICE_FEATURES_SEL: usize = 0x014;
const DRIVER_FEATURES: usize = 0x020;
const DRIVER_FEATURES_SEL: usize = 0x024;
const GUEST_PAGE_SIZE: usize = 0x028; // faqat v1
const QUEUE_SEL: usize = 0x030;
const QUEUE_NUM_MAX: usize = 0x034;
const QUEUE_NUM: usize = 0x038;
const QUEUE_ALIGN: usize = 0x03c; // faqat v1
const QUEUE_PFN: usize = 0x040; // faqat v1
const QUEUE_READY: usize = 0x044; // faqat v2
const QUEUE_NOTIFY: usize = 0x050;
const INTERRUPT_STATUS: usize = 0x060;
const INTERRUPT_ACK: usize = 0x064;
const STATUS: usize = 0x070;
const QUEUE_DESC_LOW: usize = 0x080;
const QUEUE_DESC_HIGH: usize = 0x084;
const QUEUE_DRIVER_LOW: usize = 0x090;
const QUEUE_DRIVER_HIGH: usize = 0x094;
const QUEUE_DEVICE_LOW: usize = 0x0a0;
const QUEUE_DEVICE_HIGH: usize = 0x0a4;
const CONFIG: usize = 0x100; // blok qurilmasi: siğim (sektorlarda, u64)

const DEVICE_BLOCK: u32 = 2;
const STATUS_ACK: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;

const QSIZE: usize = 8;
const DESC_NEXT: u16 = 1;
const DESC_WRITE: u16 = 2; // qurilma yozadi

const REQ_IN: u32 = 0; // diskdan öqiş
const REQ_OUT: u32 = 1; // diskka yoziş

/// Bitta sörövda eng köp sektor (bitta DMA sahifasi).
const MAX_SECTORS: usize = PAGE_SIZE / SECTOR;

#[repr(C)]
struct Desc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[derive(Debug)]
pub enum DiskError {
    NoDisk,
    OutOfRange,
    Io,
}

struct VirtioBlk {
    base: usize,
    capacity: u64,
    /// Navbat: deskriptorlar (0), "avail" halqa (128), "used" halqa (4096)
    queue: usize,
    /// DMA sahifasi: söröv sarlavhasi (0) va holat bayti (16)
    req: usize,
    /// Maʼlumot uçun DMA sahifasi
    data: usize,
    last_used: u16,
}

unsafe impl Send for VirtioBlk {}

static DISK: SpinLock<Option<VirtioBlk>> = SpinLock::new(None);

fn rd(base: usize, off: usize) -> u32 {
    unsafe { read_volatile((base + off) as *const u32) }
}

fn wr(base: usize, off: usize, v: u32) {
    unsafe { write_volatile((base + off) as *mut u32, v) }
}

fn barrier() {
    unsafe { asm!("dsb sy", options(nostack)) };
}

/// Diskni qidiradi va işga tuşiradi. Natija: siğim (baytlarda).
pub fn init() -> Option<u64> {
    let base = (0..MMIO_SLOTS)
        .map(|i| MMIO_BASE + i * MMIO_STRIDE)
        .find(|&b| rd(b, MAGIC) == 0x7472_6976 && rd(b, DEVICE_ID) == DEVICE_BLOCK)?;
    let version = rd(base, VERSION);

    // 1) Qayta tiklaş va "taniymiz" bosqiçlari
    wr(base, STATUS, 0);
    wr(base, STATUS, STATUS_ACK);
    wr(base, STATUS, STATUS_ACK | STATUS_DRIVER);

    // 2) Imkoniyatlar: hozirça heç narsa sörmaymiz; v2 da VERSION_1 (bit 32) majburiy
    wr(base, DEVICE_FEATURES_SEL, 0);
    let _ = rd(base, DEVICE_FEATURES);
    wr(base, DRIVER_FEATURES_SEL, 0);
    wr(base, DRIVER_FEATURES, 0);
    if version >= 2 {
        wr(base, DRIVER_FEATURES_SEL, 1);
        wr(base, DRIVER_FEATURES, 1);
        wr(base, STATUS, STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK);
        if rd(base, STATUS) & STATUS_FEATURES_OK == 0 {
            return None;
        }
    }

    // 3) Navbat (virtqueue)
    let queue = frame::alloc_contiguous(2)?;
    unsafe { core::ptr::write_bytes(queue as *mut u8, 0, 2 * PAGE_SIZE) };
    let req = frame::alloc_zeroed()?;
    let data = frame::alloc_zeroed()?;

    wr(base, QUEUE_SEL, 0);
    if (rd(base, QUEUE_NUM_MAX) as usize) < QSIZE {
        return None;
    }
    wr(base, QUEUE_NUM, QSIZE as u32);
    let (desc, avail, used) = (queue as u64, (queue + 128) as u64, (queue + PAGE_SIZE) as u64);
    if version == 1 {
        wr(base, GUEST_PAGE_SIZE, PAGE_SIZE as u32);
        wr(base, QUEUE_ALIGN, PAGE_SIZE as u32);
        wr(base, QUEUE_PFN, (queue / PAGE_SIZE) as u32);
    } else {
        wr(base, QUEUE_DESC_LOW, desc as u32);
        wr(base, QUEUE_DESC_HIGH, (desc >> 32) as u32);
        wr(base, QUEUE_DRIVER_LOW, avail as u32);
        wr(base, QUEUE_DRIVER_HIGH, (avail >> 32) as u32);
        wr(base, QUEUE_DEVICE_LOW, used as u32);
        wr(base, QUEUE_DEVICE_HIGH, (used >> 32) as u32);
        wr(base, QUEUE_READY, 1);
    }

    // 4) Tayyor
    let st = if version >= 2 { STATUS_FEATURES_OK } else { 0 };
    wr(base, STATUS, STATUS_ACK | STATUS_DRIVER | st | STATUS_DRIVER_OK);

    let capacity = rd(base, CONFIG) as u64 | (rd(base, CONFIG + 4) as u64) << 32;
    *DISK.lock() = Some(VirtioBlk { base, capacity, queue, req, data, last_used: 0 });
    Some(capacity * SECTOR as u64)
}

pub fn present() -> bool {
    DISK.lock().is_some()
}

impl VirtioBlk {
    /// Bitta söröv: `count` sektor, maʼlumot `self.data` sahifasida.
    fn request(&mut self, kind: u32, sector: u64, count: usize) -> Result<(), DiskError> {
        if sector + count as u64 > self.capacity {
            return Err(DiskError::OutOfRange);
        }
        unsafe {
            // Sarlavha: turi, zaxira, sektor
            let hdr = self.req as *mut u32;
            write_volatile(hdr, kind);
            write_volatile(hdr.add(1), 0);
            write_volatile((self.req + 8) as *mut u64, sector);
            write_volatile((self.req + 16) as *mut u8, 0xff);

            let desc = self.queue as *mut Desc;
            let data_flags = if kind == REQ_IN { DESC_NEXT | DESC_WRITE } else { DESC_NEXT };
            desc.add(0).write_volatile(Desc { addr: self.req as u64, len: 16, flags: DESC_NEXT, next: 1 });
            desc.add(1).write_volatile(Desc { addr: self.data as u64, len: (count * SECTOR) as u32, flags: data_flags, next: 2 });
            desc.add(2).write_volatile(Desc { addr: (self.req + 16) as u64, len: 1, flags: DESC_WRITE, next: 0 });

            // "avail" halqa: flags (0), idx (2), ring[] (4..)
            let avail = (self.queue + 128) as *mut u16;
            let idx = read_volatile(avail.add(1));
            write_volatile(avail.add(2 + (idx as usize % QSIZE)), 0);
            barrier();
            write_volatile(avail.add(1), idx.wrapping_add(1));
            barrier();
            wr(self.base, QUEUE_NOTIFY, 0);

            // "used" halqa: flags (0), idx (2) — qurilma tugatgunça kutamiz
            let used_idx = (self.queue + PAGE_SIZE + 2) as *const u16;
            while read_volatile(used_idx) == self.last_used {
                core::hint::spin_loop();
            }
            barrier();
            self.last_used = self.last_used.wrapping_add(1);
            wr(self.base, INTERRUPT_ACK, rd(self.base, INTERRUPT_STATUS));

            if read_volatile((self.req + 16) as *const u8) != 0 {
                return Err(DiskError::Io);
            }
        }
        Ok(())
    }
}

/// `sector` dan boşlab öqiydi. `buf` uzunligi 512 ga karrali bölişi kerak.
pub fn read(sector: u64, buf: &mut [u8]) -> Result<(), DiskError> {
    let mut guard = DISK.lock();
    let d = guard.as_mut().ok_or(DiskError::NoDisk)?;
    for (i, chunk) in buf.chunks_mut(MAX_SECTORS * SECTOR).enumerate() {
        let count = chunk.len().div_ceil(SECTOR);
        d.request(REQ_IN, sector + (i * MAX_SECTORS) as u64, count)?;
        unsafe { core::ptr::copy_nonoverlapping(d.data as *const u8, chunk.as_mut_ptr(), chunk.len()) };
    }
    Ok(())
}

/// `sector` dan boşlab yozadi. `buf` uzunligi 512 ga karrali bölişi kerak.
pub fn write(sector: u64, buf: &[u8]) -> Result<(), DiskError> {
    let mut guard = DISK.lock();
    let d = guard.as_mut().ok_or(DiskError::NoDisk)?;
    for (i, chunk) in buf.chunks(MAX_SECTORS * SECTOR).enumerate() {
        let count = chunk.len().div_ceil(SECTOR);
        unsafe {
            core::ptr::write_bytes(d.data as *mut u8, 0, PAGE_SIZE);
            core::ptr::copy_nonoverlapping(chunk.as_ptr(), d.data as *mut u8, chunk.len());
        }
        d.request(REQ_OUT, sector + (i * MAX_SECTORS) as u64, count)?;
    }
    Ok(())
}
