//! virtio transporti: qurilmani topiş, işga tuşiriş va navbat (virtqueue).
//! virtio-mmio: eski (v1) va zamonaviy (v2) versiyalar; virtio-pci: zamonaviy (1.0+,
//! PCI capability lar orqali: common/notify/isr/device). Avval MMIO, keyin PCI qurilmalar sanaladi.
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
pub const DEVICE_GPU: u32 = 16;
pub const DEVICE_SCSI: u32 = 8;

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
    pci: Option<PciRegs>,
    /// Bildiriş (notify) uçun navbat raqami (virtio-scsi: requestq = 2)
    qsel: u16,
}

/// virtio-pci zamonaviy registrlari (VA).
#[derive(Clone, Copy)]
struct PciRegs {
    cfg: usize,
    notify: usize,
    isr: usize,
    device: usize,
    irq: u32,
}

// virtio_pci_common_cfg maydonlari
const PC_DFSELECT: usize = 0;
const PC_DF: usize = 4;
const PC_GFSELECT: usize = 8;
const PC_GF: usize = 12;
const PC_STATUS: usize = 20;
const PC_QSELECT: usize = 22;
const PC_QSIZE: usize = 24;
const PC_QENABLE: usize = 28;
const PC_QNOTIFY_OFF: usize = 30;
const PC_QDESC: usize = 32;
const PC_QDRIVER: usize = 40;
const PC_QDEVICE: usize = 48;

fn mrd32(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}
fn mwr32(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}
fn mwr16(a: usize, v: u16) {
    unsafe { write_volatile(a as *mut u16, v) }
}
fn mrd16(a: usize) -> u16 {
    unsafe { read_volatile(a as *const u16) }
}
fn mwr8(a: usize, v: u8) {
    unsafe { write_volatile(a as *mut u8, v) }
}
fn mrd8(a: usize) -> u8 {
    unsafe { read_volatile(a as *const u8) }
}
fn mwr64(a: usize, v: u64) {
    mwr32(a, v as u32);
    mwr32(a + 4, (v >> 32) as u32);
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
        match self.pci {
            Some(p) => mrd32(p.device + off),
            None => self.rd(CONFIG + off),
        }
    }

    /// Berilgan turdagi birinçi qurilmani topib, işga tuşiradi.
    pub fn probe(device_id: u32) -> Option<Transport> {
        Self::probe_nth(device_id, 0)
    }

    /// Berilgan turdagi `n`-çi (0 dan) qurilmani topib işga tuşiradi.
    pub fn probe_nth(device_id: u32, n: usize) -> Option<Transport> {
        let mut mmio = (0..MMIO_SLOTS).map(|i| MMIO_BASE + i * MMIO_STRIDE).filter(|&b| unsafe {
            read_volatile(b as *const u32) == 0x7472_6976 && read_volatile((b + DEVICE_ID) as *const u32) == device_id
        });
        let mut skipped = 0;
        let base = loop {
            match mmio.next() {
                Some(b) if skipped == n => break b,
                Some(_) => skipped += 1,
                None => return Self::probe_pci(device_id, n - skipped),
            }
        };
        let mut t = Transport { base, queue: 0, last_used: 0, pci: None, qsel: 0 };
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

    /// virtio-pci qurilmasini (zamonaviy interfeys) işga tuşiradi.
    fn probe_pci(device_id: u32, n: usize) -> Option<Transport> {
        let c = PciCaps::find(device_id, n)?;
        c.begin(0)?;
        let queue = frame::alloc_contiguous(2)?;
        unsafe { core::ptr::write_bytes(queue as *mut u8, 0, 2 * PAGE_SIZE) };
        let notify = c.setup_queue(0, QSIZE, queue, queue + 128, queue + PAGE_SIZE)?;
        c.driver_ok();
        let regs = PciRegs { cfg: c.cfg, notify, isr: c.isr, device: c.device, irq: c.irq };
        Some(Transport { base: 0, queue, last_used: 0, pci: Some(regs), qsel: 0 })
    }

    /// Berilgan turdagi qurilmalar soni (MMIO + PCI) — qurilmani reset qilmasdan.
    pub fn count(device_id: u32) -> usize {
        let mmio = (0..MMIO_SLOTS)
            .map(|i| MMIO_BASE + i * MMIO_STRIDE)
            .filter(|&b| unsafe {
                read_volatile(b as *const u32) == 0x7472_6976 && read_volatile((b + DEVICE_ID) as *const u32) == device_id
            })
            .count();
        mmio + super::pci::virtio_types().iter().filter(|&&t| t == device_id).count()
    }

    /// Bir neça navbatli virtio-pci qurilmasi (masalan, virtio-scsi: controlq, eventq,
    /// requestq): `nq` ta navbat sozlanadi, Transport `main` navbat bilan işlaydi.
    pub fn probe_pci_queues(device_id: u32, n: usize, nq: u16, main: u16) -> Option<Transport> {
        let c = PciCaps::find(device_id, n)?;
        c.begin(0)?;
        let mut main_q = None;
        for sel in 0..nq {
            let queue = frame::alloc_contiguous(2)?;
            unsafe { core::ptr::write_bytes(queue as *mut u8, 0, 2 * PAGE_SIZE) };
            let notify = c.setup_queue(sel, QSIZE, queue, queue + 128, queue + PAGE_SIZE)?;
            if sel == main {
                main_q = Some((queue, notify));
            }
        }
        let (queue, notify) = main_q?;
        c.driver_ok();
        let regs = PciRegs { cfg: c.cfg, notify, isr: c.isr, device: c.device, irq: c.irq };
        Some(Transport { base: 0, queue, last_used: 0, pci: Some(regs), qsel: main })
    }

    /// Qurilmaning uzilişi (GIC INTID): virtio-mmio uyaçasi n → SPI 16+n → 48+n;
    /// virtio-pci — INTx (qarang: pci.rs).
    pub fn irq(&self) -> u32 {
        match self.pci {
            Some(p) => p.irq,
            None => 48 + ((self.base - MMIO_BASE) / MMIO_STRIDE) as u32,
        }
    }

    /// Uzilişlarni yoqadi (PCI: INTx). MMIO da uziliş GIC da yoqilganda keladi.
    pub fn enable_irq(&self) {
        if let Some(p) = self.pci {
            super::pci::set_intx(p.cfg, true);
        }
    }

    /// `ack_interrupt` uçun tutqiç: MMIO — registrlar boşi; PCI — ISR manzili | 1.
    pub fn ack_handle(&self) -> usize {
        match self.pci {
            Some(p) => p.isr | 1,
            None => self.base,
        }
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
        match self.pci {
            Some(p) => mwr16(p.notify, self.qsel),
            None => self.wr(QUEUE_NOTIFY, self.qsel as u32),
        }
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
        ack_interrupt(self.ack_handle());
        n
    }
}

/// virtio-pci qurilmasining zamonaviy registrlari (capability lardan topilgan VA lar).
#[derive(Clone, Copy)]
pub struct PciCaps {
    pub cfg: usize,
    pub common: usize,
    pub notify: usize,
    pub notify_mult: u32,
    pub isr: usize,
    pub device: usize,
    pub irq: u32,
}

impl PciCaps {
    /// `device_type` turidagi `n`-çi virtio-pci qurilmasi va uning capability lari.
    pub fn find(device_type: u32, n: usize) -> Option<PciCaps> {
        use super::pci::{cfg_rd8, cfg_rd16, cfg_rd32};
        let d = super::pci::virtio_nth(device_type, n)?;
        let mut c = PciCaps { cfg: d.cfg, common: 0, notify: 0, notify_mult: 0, isr: 0, device: 0, irq: d.irq };
        // virtio capability (0x09): cfg_type (+3), bar (+4), offset (+8); notify: multiplier (+16)
        let mut ptr = if cfg_rd16(d.cfg, 0x06) & (1 << 4) != 0 { cfg_rd8(d.cfg, 0x34) as usize } else { 0 };
        let mut guard = 0;
        while ptr >= 0x40 && guard < 48 {
            guard += 1;
            if cfg_rd8(d.cfg, ptr) == 0x09 {
                let bar = cfg_rd8(d.cfg, ptr + 4) as usize;
                let at = if bar < 6 && d.bars[bar] != 0 { d.bars[bar] + cfg_rd32(d.cfg, ptr + 8) as usize } else { 0 };
                match cfg_rd8(d.cfg, ptr + 3) {
                    1 if c.common == 0 => c.common = at,
                    2 if c.notify == 0 => {
                        c.notify = at;
                        c.notify_mult = cfg_rd32(d.cfg, ptr + 16);
                    }
                    3 if c.isr == 0 => c.isr = at,
                    4 if c.device == 0 => c.device = at,
                    _ => {}
                }
            }
            ptr = cfg_rd8(d.cfg, ptr + 1) as usize;
        }
        (c.common != 0 && c.notify != 0 && c.isr != 0).then_some(c)
    }

    /// Reset, ACK/DRIVER, imkoniyatlar: `features` (0..31 bitlar) + VERSION_1, FEATURES_OK.
    /// Natija: qurilma taklif qilgan 0..31 bitlar. INTx öçiriladi.
    pub fn begin(&self, features: u32) -> Option<u32> {
        let cm = self.common;
        super::pci::set_intx(self.cfg, false);
        mwr8(cm + PC_STATUS, 0);
        let mut n = 0;
        while mrd8(cm + PC_STATUS) != 0 && n < 1_000_000 {
            n += 1;
            core::hint::spin_loop();
        }
        mwr8(cm + PC_STATUS, STATUS_ACK as u8);
        mwr8(cm + PC_STATUS, (STATUS_ACK | STATUS_DRIVER) as u8);
        mwr32(cm + PC_DFSELECT, 0);
        let offered = mrd32(cm + PC_DF);
        mwr32(cm + PC_GFSELECT, 0);
        mwr32(cm + PC_GF, features & offered);
        mwr32(cm + PC_GFSELECT, 1);
        mwr32(cm + PC_GF, 1); // VIRTIO_F_VERSION_1
        mwr8(cm + PC_STATUS, (STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK) as u8);
        (mrd8(cm + PC_STATUS) as u32 & STATUS_FEATURES_OK != 0).then_some(offered)
    }

    /// `sel` navbatini sozlaydi va yoqadi. Natija: shu navbatning notify manzili.
    pub fn setup_queue(&self, sel: u16, size: usize, desc: usize, avail: usize, used: usize) -> Option<usize> {
        let cm = self.common;
        mwr16(cm + PC_QSELECT, sel);
        if (mrd16(cm + PC_QSIZE) as usize) < size {
            return None;
        }
        mwr16(cm + PC_QSIZE, size as u16);
        mwr64(cm + PC_QDESC, desc as u64);
        mwr64(cm + PC_QDRIVER, avail as u64);
        mwr64(cm + PC_QDEVICE, used as u64);
        let qoff = mrd16(cm + PC_QNOTIFY_OFF) as usize;
        mwr16(cm + PC_QENABLE, 1);
        Some(self.notify + qoff * self.notify_mult as usize)
    }

    pub fn driver_ok(&self) {
        mwr8(self.common + PC_STATUS, (STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK) as u8);
    }
}

/// Navbat bildiriş (virtio-pci notify registri).
pub fn pci_notify(addr: usize, queue: u16) {
    mwr16(addr, queue);
}

/// Qurilma uzilişini tasdiqlaydi (uzilişlar işlovçisidan ham çaqiriladi).
pub fn ack_interrupt(base: usize) {
    if base & 1 != 0 {
        let _ = mrd8(base & !1); // virtio-pci ISR: öqiş tozalaydi (INTx tuşadi)
        return;
    }
    unsafe {
        let st = read_volatile((base + INTERRUPT_STATUS) as *const u32);
        write_volatile((base + INTERRUPT_ACK) as *mut u32, st);
    }
}
