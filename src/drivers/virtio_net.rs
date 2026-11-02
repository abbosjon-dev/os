//! virtio-net tarmoq drayveri (virtio-mmio, v1 va v2).
//!
//! Ikki navbat: 0 — qabul (RX), 1 — jönatiş (TX). Qabul uzilişlar orqali: kelgan
//! kadrlar halqa buferga tüşadi, tarmoq oqimi ularni oladi. Jönatiş sinxron.
//! Barça DMA buferlari identity xaritalangan RAMʼda (PA == VA).

use crate::memory::{PAGE_SIZE, frame};
use crate::sync::SpinLock;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};

const MMIO_BASE: usize = 0x0a00_0000;
const MMIO_STRIDE: usize = 0x200;
const MMIO_SLOTS: usize = 32;
const DEVICE_NET: u32 = 1;

// Registrlar
const DEVICE_ID: usize = 0x008;
const DEVICE_FEATURES: usize = 0x010;
const DEVICE_FEATURES_SEL: usize = 0x014;
const DRIVER_FEATURES: usize = 0x020;
const DRIVER_FEATURES_SEL: usize = 0x024;
const GUEST_PAGE_SIZE: usize = 0x028;
const QUEUE_SEL: usize = 0x030;
#[allow(dead_code)]
const QUEUE_NUM_MAX: usize = 0x034;
const QUEUE_NUM: usize = 0x038;
const QUEUE_ALIGN: usize = 0x03c;
const QUEUE_PFN: usize = 0x040;
const QUEUE_READY: usize = 0x044;
const QUEUE_NOTIFY: usize = 0x050;
#[allow(dead_code)]
const INTERRUPT_STATUS: usize = 0x060;
#[allow(dead_code)]
const INTERRUPT_ACK: usize = 0x064;
const STATUS: usize = 0x070;
const QUEUE_DESC_LOW: usize = 0x080;
const QUEUE_DESC_HIGH: usize = 0x084;
const QUEUE_DRIVER_LOW: usize = 0x090;
const QUEUE_DRIVER_HIGH: usize = 0x094;
const QUEUE_DEVICE_LOW: usize = 0x0a0;
const QUEUE_DEVICE_HIGH: usize = 0x0a4;
const CONFIG: usize = 0x100;

const STATUS_ACK: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const VIRTIO_NET_F_MAC: u32 = 1 << 5;

const QSIZE: usize = 16;
#[allow(dead_code)]
const DESC_NEXT: u16 = 1;
const DESC_WRITE: u16 = 2;
/// virtio-net kadr sarlavhasi: eski (v1, MRG_RXBUF yöq) — 10 bayt, modern (v2) — 12 bayt
const FRAME: usize = 1600; // sarlavha + eng katta Ethernet kadri

#[repr(C)]
struct Desc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

struct Queue {
    mem: usize,  // deskriptorlar (0), avail (desc dan keyin), used (2-sahifa)
    bufs: usize, // QSIZE ta FRAME baytli bufer
    last_used: u16,
    avail_idx: u16,
}

impl Queue {
    fn new() -> Option<Queue> {
        let mem = frame::alloc_contiguous(2)?;
        let bufs = frame::alloc_contiguous((QSIZE * FRAME).div_ceil(PAGE_SIZE))?;
        unsafe { core::ptr::write_bytes(mem as *mut u8, 0, 2 * PAGE_SIZE) };
        Some(Queue { mem, bufs, last_used: 0, avail_idx: 0 })
    }
    fn desc(&self) -> *mut Desc {
        self.mem as *mut Desc
    }
    fn avail(&self) -> *mut u16 {
        (self.mem + QSIZE * 16) as *mut u16
    }
    fn used(&self) -> usize {
        self.mem + PAGE_SIZE
    }
    fn buf(&self, i: usize) -> usize {
        self.bufs + i * FRAME
    }
}

struct Net {
    base: usize,
    mac: [u8; 6],
    hdr: usize,
    rx: Queue,
    tx: Queue,
}

unsafe impl Send for Net {}

static NET: SpinLock<Option<Net>> = SpinLock::new(None);
/// Kelgan kadrlar (tarmoq oqimi oladi)
static INBOX: SpinLock<Vec<Vec<u8>>> = SpinLock::new(Vec::new());
static IRQ: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

fn barrier() {
    unsafe { core::arch::asm!("dsb sy", options(nostack)) };
}

impl Net {
    fn rd(&self, o: usize) -> u32 {
        unsafe { read_volatile((self.base + o) as *const u32) }
    }
    fn wr(&self, o: usize, v: u32) {
        unsafe { write_volatile((self.base + o) as *mut u32, v) }
    }

    fn setup_queue(&self, sel: u32, q: &Queue, version: u32) {
        self.wr(QUEUE_SEL, sel);
        self.wr(QUEUE_NUM, QSIZE as u32);
        let (d, a, u) = (q.mem as u64, q.avail() as u64, q.used() as u64);
        if version == 1 {
            self.wr(GUEST_PAGE_SIZE, PAGE_SIZE as u32);
            self.wr(QUEUE_ALIGN, PAGE_SIZE as u32);
            self.wr(QUEUE_PFN, (q.mem / PAGE_SIZE) as u32);
        } else {
            self.wr(QUEUE_DESC_LOW, d as u32);
            self.wr(QUEUE_DESC_HIGH, (d >> 32) as u32);
            self.wr(QUEUE_DRIVER_LOW, a as u32);
            self.wr(QUEUE_DRIVER_HIGH, (a >> 32) as u32);
            self.wr(QUEUE_DEVICE_LOW, u as u32);
            self.wr(QUEUE_DEVICE_HIGH, (u >> 32) as u32);
            self.wr(QUEUE_READY, 1);
        }
    }

    /// RX navbatini boş buferlar bilan töldiradi (qurilma ularga kadr yozadi).
    fn fill_rx(&mut self) {
        for i in 0..QSIZE {
            unsafe {
                self.rx.desc().add(i).write_volatile(Desc {
                    addr: self.rx.buf(i) as u64,
                    len: FRAME as u32,
                    flags: DESC_WRITE,
                    next: 0,
                });
                let a = self.rx.avail();
                write_volatile(a.add(2 + i), i as u16);
            }
        }
        barrier();
        unsafe { write_volatile(self.rx.avail().add(1), QSIZE as u16) };
        self.rx.avail_idx = QSIZE as u16;
        barrier();
        self.wr(QUEUE_NOTIFY, 0);
    }

    /// Kelgan kadrlarni INBOX ga köçiradi va buferlarni qayta navbatga qöyadi.
    fn drain_rx(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let used = self.rx.used();

        loop {
            let used_idx = unsafe { read_volatile((used + 2) as *const u16) };
            if used_idx == self.rx.last_used {
                break;
            }
            barrier();
            let slot = self.rx.last_used as usize % QSIZE;
            let id = unsafe { read_volatile((used + 4 + slot * 8) as *const u32) } as usize;
            let len = unsafe { read_volatile((used + 4 + slot * 8 + 4) as *const u32) } as usize;
            if len > self.hdr && id < QSIZE {
                let frame_ptr = (self.rx.buf(id) + self.hdr) as *const u8;
                let data = unsafe { core::slice::from_raw_parts(frame_ptr, len - self.hdr) };
                out.push(data.to_vec());
            }
            // Buferni qaytadan navbatga
            unsafe {
                let a = self.rx.avail();
                write_volatile(a.add(2 + (self.rx.avail_idx as usize % QSIZE)), id as u16);
            }
            self.rx.avail_idx = self.rx.avail_idx.wrapping_add(1);
            self.rx.last_used = self.rx.last_used.wrapping_add(1);
        }
        barrier();
        unsafe { write_volatile(self.rx.avail().add(1), self.rx.avail_idx) };
        barrier();
        self.wr(QUEUE_NOTIFY, 0);
        out
    }

    /// Bitta Ethernet kadrini jönatadi (sinxron).
    fn send(&mut self, frame_bytes: &[u8]) {
        let slot = self.tx.last_used as usize % QSIZE;
        let buf = self.tx.buf(slot);
        let hdr = self.hdr;
        unsafe {
            core::ptr::write_bytes(buf as *mut u8, 0, hdr);
            let n = frame_bytes.len().min(FRAME - hdr);
            core::ptr::copy_nonoverlapping(frame_bytes.as_ptr(), (buf + hdr) as *mut u8, n);
            self.tx.desc().add(slot).write_volatile(Desc {
                addr: buf as u64,
                len: (hdr + n) as u32,
                flags: 0,
                next: 0,
            });
            let a = self.tx.avail();
            let idx = read_volatile(a.add(1));
            write_volatile(a.add(2 + (idx as usize % QSIZE)), slot as u16);
            barrier();
            write_volatile(a.add(1), idx.wrapping_add(1));
        }
        barrier();
        self.wr(QUEUE_NOTIFY, 1);
        // Jönatilgunça kutamiz
        let used = self.tx.used();
        while unsafe { read_volatile((used + 2) as *const u16) } == self.tx.last_used {
            core::hint::spin_loop();
        }
        self.tx.last_used = self.tx.last_used.wrapping_add(1);
    }
}

/// Qurilmani topadi va işga tuşiradi. Natija: MAC manzil.
pub fn init() -> Option<[u8; 6]> {
    let base = (0..MMIO_SLOTS).map(|i| MMIO_BASE + i * MMIO_STRIDE).find(|&b| unsafe {
        read_volatile(b as *const u32) == 0x7472_6976 && read_volatile((b + DEVICE_ID) as *const u32) == DEVICE_NET
    })?;
    let mut net = Net { base, mac: [0; 6], hdr: 12, rx: Queue::new()?, tx: Queue::new()? };
    let version = net.rd(0x004);
    net.hdr = if version >= 2 { 12 } else { 10 };

    net.wr(STATUS, 0);
    net.wr(STATUS, STATUS_ACK);
    net.wr(STATUS, STATUS_ACK | STATUS_DRIVER);
    net.wr(DEVICE_FEATURES_SEL, 0);
    let feat = net.rd(DEVICE_FEATURES);
    net.wr(DRIVER_FEATURES_SEL, 0);
    net.wr(DRIVER_FEATURES, feat & VIRTIO_NET_F_MAC); // faqat MAC ni söraymiz
    if version >= 2 {
        net.wr(DRIVER_FEATURES_SEL, 1);
        net.wr(DRIVER_FEATURES, 1); // VERSION_1
        net.wr(STATUS, STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK);
        if net.rd(STATUS) & STATUS_FEATURES_OK == 0 {
            return None;
        }
    }
    for i in 0..6 {
        net.mac[i] = unsafe { read_volatile((net.base + CONFIG + i) as *const u8) };
    }
    net.setup_queue(0, &net.rx, version);
    net.setup_queue(1, &net.tx, version);
    let fok = if version >= 2 { STATUS_FEATURES_OK } else { 0 };
    net.wr(STATUS, STATUS_ACK | STATUS_DRIVER | fok | STATUS_DRIVER_OK);
    net.fill_rx();

    let mac = net.mac;
    let irq = 48 + ((base - MMIO_BASE) / MMIO_STRIDE) as u32;
    IRQ.store(irq, core::sync::atomic::Ordering::Release);
    crate::gic::enable(irq);
    *NET.lock() = Some(net);
    Some(mac)
}

pub fn present() -> bool {
    NET.lock().is_some()
}

pub fn mac() -> [u8; 6] {
    NET.lock().as_ref().map_or([0; 6], |n| n.mac)
}

pub fn irq() -> u32 {
    IRQ.load(core::sync::atomic::Ordering::Acquire)
}

/// Uzilişlar işlovçisi: kelgan kadrlarni INBOX ga qöyadi va tarmoq oqimini uyğotadi.
pub fn handle_irq() {
    let frames = {
        let mut g = NET.lock();
        let Some(n) = g.as_mut() else { return };
        super::virtio::ack_interrupt(n.base);
        n.drain_rx()
    };
    if !frames.is_empty() {
        INBOX.lock().extend(frames);
        crate::net::wake();
    }
}

/// Navbatdagi kelgan kadr (bölmasa — None).
pub fn recv() -> Option<Vec<u8>> {
    let mut inbox = INBOX.lock();
    if inbox.is_empty() {
        None
    } else {
        Some(inbox.remove(0))
    }
}

#[allow(dead_code)]
pub fn has_frames() -> bool {
    !INBOX.lock().is_empty()
}

/// Uziliş kutmasdan RX navbatini böşatadi (tarmoq oqimi muntazam çaqiradi).
pub fn poll() -> Vec<Vec<u8>> {
    let mut g = NET.lock();
    match g.as_mut() {
        Some(n) => n.drain_rx(),
        None => Vec::new(),
    }
}

/// Ethernet kadrini jönatadi.
pub fn send(frame_bytes: &[u8]) {
    if let Some(n) = NET.lock().as_mut() {
        n.send(frame_bytes);
    }
}
