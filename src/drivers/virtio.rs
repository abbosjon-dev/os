//! virtio-mmio transporti: qurilmani topiş, işga tuşiriş va navbat (virtqueue).
//! Eski (v1) va zamonaviy (v2) versiyalar qöllab-quvvatlanadi.
//! Hozirça sinxron: söröv yuboriladi va tugaşi kutiladi.

use crate::memory::{PAGE_SIZE, frame};
use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

const MMIO_BASE: usize = 0x0a00_0000;
const MMIO_STRIDE: usize = 0x200;
const MMIO_SLOTS: usize = 32;

const MAGIC: usize = 0x000;
const VERSION: usize = 0x004;
const DEVICE_ID: usize = 0x008;
const DEVICE_FEATURES: usize = 0x010;
const DEVICE_FEATURES_SEL: usize = 0x014;
const DRIVER_FEATURES: usize = 0x020;
const DRIVER_FEATURES_SEL: usize = 0x024;
const GUEST_PAGE_SIZE: usize = 0x028;
const QUEUE_SEL: usize = 0x030;
const QUEUE_NUM_MAX: usize = 0x034;
const QUEUE_NUM: usize = 0x038;
const QUEUE_ALIGN: usize = 0x03c;
const QUEUE_PFN: usize = 0x040;
const QUEUE_READY: usize = 0x044;
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
pub const CONFIG: usize = 0x100;

const STATUS_ACK: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;

const QSIZE: usize = 8;
const DESC_NEXT: u16 = 1;
const DESC_WRITE: u16 = 2;

pub const DEVICE_BLOCK: u32 = 2;
pub const DEVICE_RNG: u32 = 4;

#[repr(C)]
struct Desc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

/// Bufer: (fizik manzil, uzunlik, qurilma yozadimi)
pub type Buf = (usize, usize, bool);

pub struct Transport {
    base: usize,
    queue: usize,
    last_used: u16,
}

unsafe impl Send for Transport {}

fn barrier() {
    unsafe { asm!("dsb sy", options(nostack)) };
}

impl Transport {
    fn rd(&self, off: usize) -> u32 {
        unsafe { read_volatile((self.base + off) as *const u32) }
    }

    fn wr(&self, off: usize, v: u32) {
        unsafe { write_volatile((self.base + off) as *mut u32, v) }
    }

    pub fn config_u32(&self, off: usize) -> u32 {
        self.rd(CONFIG + off)
    }

    /// Berilgan turdagi birinçi qurilmani topib, işga tuşiradi.
    pub fn probe(device_id: u32) -> Option<Transport> {
        let base = (0..MMIO_SLOTS).map(|i| MMIO_BASE + i * MMIO_STRIDE).find(|&b| unsafe {
            read_volatile(b as *const u32) == 0x7472_6976 && read_volatile((b + DEVICE_ID) as *const u32) == device_id
        })?;
        let mut t = Transport { base, queue: 0, last_used: 0 };
        let version = t.rd(VERSION);
        let _ = t.rd(MAGIC);

        t.wr(STATUS, 0);
        t.wr(STATUS, STATUS_ACK);
        t.wr(STATUS, STATUS_ACK | STATUS_DRIVER);

        // Imkoniyatlar: heç narsa sörmaymiz; v2 da VERSION_1 (bit 32) majburiy
        t.wr(DEVICE_FEATURES_SEL, 0);
        let _ = t.rd(DEVICE_FEATURES);
        t.wr(DRIVER_FEATURES_SEL, 0);
        t.wr(DRIVER_FEATURES, 0);
        let features_ok = if version >= 2 {
            t.wr(DRIVER_FEATURES_SEL, 1);
            t.wr(DRIVER_FEATURES, 1);
            t.wr(STATUS, STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK);
            if t.rd(STATUS) & STATUS_FEATURES_OK == 0 {
                return None;
            }
            STATUS_FEATURES_OK
        } else {
            0
        };

        // Navbat: deskriptorlar (0), "avail" (128), "used" (4096)
        let queue = frame::alloc_contiguous(2)?;
        unsafe { core::ptr::write_bytes(queue as *mut u8, 0, 2 * PAGE_SIZE) };
        t.queue = queue;
        t.wr(QUEUE_SEL, 0);
        if (t.rd(QUEUE_NUM_MAX) as usize) < QSIZE {
            return None;
        }
        t.wr(QUEUE_NUM, QSIZE as u32);
        let (desc, avail, used) = (queue as u64, (queue + 128) as u64, (queue + PAGE_SIZE) as u64);
        if version == 1 {
            t.wr(GUEST_PAGE_SIZE, PAGE_SIZE as u32);
            t.wr(QUEUE_ALIGN, PAGE_SIZE as u32);
            t.wr(QUEUE_PFN, (queue / PAGE_SIZE) as u32);
        } else {
            t.wr(QUEUE_DESC_LOW, desc as u32);
            t.wr(QUEUE_DESC_HIGH, (desc >> 32) as u32);
            t.wr(QUEUE_DRIVER_LOW, avail as u32);
            t.wr(QUEUE_DRIVER_HIGH, (avail >> 32) as u32);
            t.wr(QUEUE_DEVICE_LOW, used as u32);
            t.wr(QUEUE_DEVICE_HIGH, (used >> 32) as u32);
            t.wr(QUEUE_READY, 1);
        }
        t.wr(STATUS, STATUS_ACK | STATUS_DRIVER | features_ok | STATUS_DRIVER_OK);
        Some(t)
    }

    /// Qurilmaning uzilişi (GIC INTID): virtio-mmio uyaçasi n → SPI 16+n → 48+n.
    pub fn irq(&self) -> u32 {
        48 + ((self.base - MMIO_BASE) / MMIO_STRIDE) as u32
    }

    pub fn base(&self) -> usize {
        self.base
    }

    /// "used" halqa indeksining manzili — kutiş şarti uçun (qulfsiz öqiladi).
    pub fn used_idx_addr(&self) -> usize {
        self.queue + PAGE_SIZE + 2
    }

    pub fn last_used(&self) -> u16 {
        self.last_used
    }

    /// Buferlar zanjirini yuboradi (kutmaydi).
    pub fn submit_async(&mut self, bufs: &[Buf]) {
        assert!(!bufs.is_empty() && bufs.len() <= QSIZE);
        unsafe {
            let desc = self.queue as *mut Desc;
            for (i, &(addr, len, write)) in bufs.iter().enumerate() {
                let mut flags = if write { DESC_WRITE } else { 0 };
                if i + 1 < bufs.len() {
                    flags |= DESC_NEXT;
                }
                desc.add(i).write_volatile(Desc { addr: addr as u64, len: len as u32, flags, next: (i + 1) as u16 });
            }
            let avail = (self.queue + 128) as *mut u16;
            let idx = read_volatile(avail.add(1));
            write_volatile(avail.add(2 + (idx as usize % QSIZE)), 0);
            barrier();
            write_volatile(avail.add(1), idx.wrapping_add(1));
            barrier();
        }
        self.wr(QUEUE_NOTIFY, 0);
    }

    pub fn is_done(&self) -> bool {
        unsafe { read_volatile(self.used_idx_addr() as *const u16) != self.last_used }
    }

    /// Tugagan sörövni qabul qiladi. Natija: qurilma yozgan baytlar soni.
    pub fn complete(&mut self) -> usize {
        barrier();
        let used = self.queue + PAGE_SIZE;
        let slot = self.last_used as usize % QSIZE;
        let written = unsafe { read_volatile((used + 4 + slot * 8 + 4) as *const u32) } as usize;
        self.last_used = self.last_used.wrapping_add(1);
        written
    }

    /// Yuboradi va tugaşini aylanib kutadi (uzilişsiz; yuklaniş paytida va kiçik qurilmalar uçun).
    pub fn submit(&mut self, bufs: &[Buf]) -> usize {
        self.submit_async(bufs);
        while !self.is_done() {
            core::hint::spin_loop();
        }
        let n = self.complete();
        ack_interrupt(self.base);
        n
    }
}

/// Qurilma uzilişini tasdiqlaydi (uzilişlar işlovçisidan ham çaqiriladi).
pub fn ack_interrupt(base: usize) {
    unsafe {
        let st = read_volatile((base + INTERRUPT_STATUS) as *const u32);
        write_volatile((base + INTERRUPT_ACK) as *mut u32, st);
    }
}
