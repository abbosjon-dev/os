//! virtio-scsi disk kontrolleri (virtio 1.x zamonaviy PCI: 0x1af4:0x1048, eski 0x1004).
//!
//! Netcup SCP da disk kontrolleri "SCSI" qilib tanlansa, QEMU `virtio-scsi-pci` + `scsi-hd`
//! beradi. Navbatlar: controlq (0), eventq (1), requestq (2) — sörovlar faqat requestq orqali.
//! Nişonlar (target) 0..max_target, har birida LUN 0 dan boşlab skanerlanadi: INQUIRY
//! (to'g'ridan-to'g'ri murojaat qurilmasi, tur 0), TEST UNIT READY (UNIT ATTENTION ni tozalaydi),
//! READ CAPACITY(10) → kerak bölsa (16). Öqiş/yoziş: READ/WRITE(10), 2^32 sektordan katta diskda
//! (16); yozişdan keyin SYNCHRONIZE CACHE. Faqat 512 baytli sektorlar.
//!
//! Har bir LUN `virtio_blk` dagi umumiy blok qurilma interfeysi orqali GPT/ZaminFS va FAT ga
//! beriladi. Kontroller bitta, LUN lar umumiy qulf (`CTL`) bilan bölişiladi.

use super::virtio::{DEVICE_SCSI, Transport};
use crate::memory::frame;
use crate::sync::Mutex;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};

const SECTOR: usize = 512;
/// Söröv sarlavhasi (lun, id, task_attr, prio, crn, cdb) — DMA sahifasi boşida
const HDR: usize = 0;
/// Javob (sense_len, residual, status_qualifier, status, response, sense) — 256 dan
const RESP: usize = 256;
/// Kiçik ma'lumot (INQUIRY, READ CAPACITY) — 1024 dan
const SMALL: usize = 1024;

// SCSI buyruqlari
const OP_TEST_UNIT_READY: u8 = 0x00;
const OP_INQUIRY: u8 = 0x12;
const OP_READ_CAPACITY10: u8 = 0x25;
const OP_READ10: u8 = 0x28;
const OP_WRITE10: u8 = 0x2a;
const OP_SYNC_CACHE10: u8 = 0x35;
const OP_READ16: u8 = 0x88;
const OP_WRITE16: u8 = 0x8a;
const OP_SYNC_CACHE16: u8 = 0x91;
const OP_SERVICE_IN16: u8 = 0x9e;
const SA_READ_CAPACITY16: u8 = 0x10;

/// Söröv natijasini kutiş usuli.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// Aylanib kutiş (yuklaniş, FAT diski)
    Poll,
    /// Uziliş orqali uxlab kutiş (tizim diski)
    Irq,
    /// Panic: qulfsiz, aylanib
    Panic,
}

#[derive(Clone, Copy)]
pub struct Lun {
    pub target: u8,
    pub lun: u16,
    /// Sektorlar soni
    pub capacity: u64,
    /// READ/WRITE(16) kerak (LBA ≥ 2^32)
    pub use16: bool,
}

struct Ctl {
    t: Transport,
    /// DMA sahifasi: sarlavha, javob, kiçik ma'lumot
    req: usize,
    cdb_size: usize,
    sense_size: usize,
    luns: Vec<Lun>,
    tag: u64,
}
unsafe impl Send for Ctl {}

static CTL: Mutex<Option<Ctl>> = Mutex::new(None);

/// virtio-scsi LUN manzili (8 bayt): 1, nişon, 0x40 | LUN yuqori, LUN quyi (tekis format).
pub fn lun_addr(target: u8, lun: u16) -> [u8; 8] {
    [1, target, 0x40 | ((lun >> 8) as u8 & 0x3f), lun as u8, 0, 0, 0, 0]
}

/// READ/WRITE CDB: (10) — LBA 32 bit, uzunlik 16 bit; (16) — LBA 64 bit, uzunlik 32 bit.
/// Natija: (CDB, uzunligi).
pub fn cdb_rw(write: bool, lba: u64, count: u32, use16: bool) -> ([u8; 16], usize) {
    let mut c = [0u8; 16];
    if use16 {
        c[0] = if write { OP_WRITE16 } else { OP_READ16 };
        c[2..10].copy_from_slice(&lba.to_be_bytes());
        c[10..14].copy_from_slice(&count.to_be_bytes());
        (c, 16)
    } else {
        c[0] = if write { OP_WRITE10 } else { OP_READ10 };
        c[2..6].copy_from_slice(&(lba as u32).to_be_bytes());
        c[7..9].copy_from_slice(&(count as u16).to_be_bytes());
        (c, 10)
    }
}

/// SCSI sense dan kalit (sense key): qatʼiy (0x70/71) va deskriptor (0x72/73) formatlar.
fn sense_key(s: &[u8]) -> u8 {
    match s.first().map(|b| b & 0x7f) {
        Some(0x70 | 0x71) if s.len() > 2 => s[2] & 0xf,
        Some(0x72 | 0x73) if s.len() > 1 => s[1] & 0xf,
        _ => 0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Res {
    Ok,
    /// UNIT ATTENTION — qayta urinib köriş kerak
    Retry,
    /// Nişon/LUN yöq yoki xato
    Fail,
}

impl Ctl {
    /// Bitta buyruq: `cdb`, ma'lumot (manzil, uzunlik, qurilma yozadimi).
    fn command(&mut self, lun: [u8; 8], cdb: &[u8], data: Option<(usize, usize, bool)>, wait: Wait) -> Res {
        let (req, resp) = (self.req + HDR, self.req + RESP);
        self.tag = self.tag.wrapping_add(1);
        let hdr_len = 19 + self.cdb_size;
        let resp_len = 12 + self.sense_size;
        unsafe {
            core::ptr::write_bytes(req as *mut u8, 0, hdr_len);
            core::ptr::copy_nonoverlapping(lun.as_ptr(), req as *mut u8, 8);
            write_volatile((req + 8) as *mut u64, self.tag);
            core::ptr::copy_nonoverlapping(cdb.as_ptr(), (req + 19) as *mut u8, cdb.len().min(self.cdb_size));
            core::ptr::write_bytes(resp as *mut u8, 0, resp_len);
            write_volatile((resp + 11) as *mut u8, 0xff);
        }
        // Avval qurilma öqiydiganlar, keyin yozadiganlar
        let h = (req, hdr_len, false);
        let r = (resp, resp_len, true);
        let mut bufs = [h, r, r];
        let n = match data {
            None => 2,
            Some(d @ (_, _, false)) => {
                bufs[1] = d;
                3
            }
            Some(d) => {
                bufs[2] = d;
                3
            }
        };
        match wait {
            Wait::Irq => {
                self.t.submit_async(&bufs[..n]);
                super::virtio_blk::wait_irq(&mut self.t);
            }
            _ => {
                self.t.submit(&bufs[..n]);
            }
        }
        let response = unsafe { read_volatile((resp + 11) as *const u8) };
        let status = unsafe { read_volatile((resp + 10) as *const u8) };
        if response != 0 {
            return Res::Fail; // VIRTIO_SCSI_S_BAD_TARGET va h.k.
        }
        match status {
            0 => Res::Ok,
            2 => {
                // CHECK CONDITION
                let slen = (unsafe { read_volatile(resp as *const u32) } as usize).min(self.sense_size);
                let sense = unsafe { core::slice::from_raw_parts((resp + 12) as *const u8, slen) };
                if sense_key(sense) == 6 { Res::Retry } else { Res::Fail }
            }
            _ => Res::Retry, // BUSY / TASK SET FULL
        }
    }

    fn command_retry(&mut self, lun: [u8; 8], cdb: &[u8], data: Option<(usize, usize, bool)>, wait: Wait) -> bool {
        for _ in 0..4 {
            match self.command(lun, cdb, data, wait) {
                Res::Ok => return true,
                Res::Fail => return false,
                Res::Retry => {}
            }
        }
        false
    }

    fn rw(&mut self, i: usize, write: bool, lba: u64, count: usize, data: usize, wait: Wait) -> bool {
        let Some(l) = self.luns.get(i).copied() else { return false };
        if count == 0 || lba + count as u64 > l.capacity {
            return false;
        }
        let (cdb, n) = cdb_rw(write, lba, count as u32, l.use16);
        self.command_retry(lun_addr(l.target, l.lun), &cdb[..n], Some((data, count * SECTOR, !write)), wait)
    }

    /// Bitta LUN ni tekşiradi: disk bölsa — siğimi.
    fn probe_lun(&mut self, target: u8, lun: u16) -> Option<Lun> {
        let a = lun_addr(target, lun);
        let small = self.req + SMALL;
        unsafe { core::ptr::write_bytes(small as *mut u8, 0, 64) };
        let inq = [OP_INQUIRY, 0, 0, 0, 36, 0];
        if self.command(a, &inq, Some((small, 36, true)), Wait::Poll) != Res::Ok {
            return None;
        }
        let pdt = unsafe { read_volatile(small as *const u8) };
        if pdt != 0 {
            return None; // qualifier ≠ 0 (LUN yöq) yoki disk emas
        }
        // UNIT ATTENTION (reset dan keyin) ni tozalaymiz
        let mut ready = false;
        for _ in 0..6 {
            if self.command(a, &[OP_TEST_UNIT_READY, 0, 0, 0, 0, 0], None, Wait::Poll) == Res::Ok {
                ready = true;
                break;
            }
        }
        if !ready {
            return None;
        }
        unsafe { core::ptr::write_bytes(small as *mut u8, 0, 64) };
        let rc10 = [OP_READ_CAPACITY10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        if !self.command_retry(a, &rc10, Some((small, 8, true)), Wait::Poll) {
            return None;
        }
        let rd = |o: usize| unsafe { read_volatile((small + o) as *const [u8; 4]) };
        let mut last = u32::from_be_bytes(rd(0)) as u64;
        let mut blen = u32::from_be_bytes(rd(4));
        if last == 0xffff_ffff {
            let mut rc16 = [0u8; 16];
            rc16[0] = OP_SERVICE_IN16;
            rc16[1] = SA_READ_CAPACITY16;
            rc16[10..14].copy_from_slice(&32u32.to_be_bytes());
            unsafe { core::ptr::write_bytes(small as *mut u8, 0, 64) };
            if !self.command_retry(a, &rc16, Some((small, 32, true)), Wait::Poll) {
                return None;
            }
            last = u64::from_be_bytes(unsafe { read_volatile(small as *const [u8; 8]) });
            blen = u32::from_be_bytes(rd(8));
        }
        if blen as usize != SECTOR {
            crate::println!("  virtio-scsi: nişon {}, LUN {}: sektor {} bayt — qöllanmaydi", target, lun, blen);
            return None;
        }
        let capacity = last + 1;
        Some(Lun { target, lun, capacity, use16: last > 0xffff_ffff })
    }
}

/// Birinçi virtio-scsi kontrollerini işga tuşirib, disklarni (LUN) skanerlaydi.
/// Natija: topilgan disklar soni.
pub fn init() -> usize {
    if Transport::count(DEVICE_SCSI) == 0 {
        return 0;
    }
    let Some(t) = Transport::probe_pci_queues(DEVICE_SCSI, 0, 3, 2) else {
        crate::println!("  virtio-scsi: kontroller işga tuşmadi");
        return 0;
    };
    let Some(req) = frame::alloc_zeroed() else { return 0 };
    let cdb_size = t.config_u32(24) as usize;
    let sense_size = t.config_u32(20) as usize;
    let max_target = (t.config_u32(28) >> 16) as u16;
    let max_lun = t.config_u32(32);
    if !(16..=200).contains(&cdb_size) || sense_size > 600 {
        crate::println!("  virtio-scsi: cdb {} / sense {} — qöllanmaydi", cdb_size, sense_size);
        return 0;
    }
    let mut c = Ctl { t, req, cdb_size, sense_size, luns: Vec::new(), tag: 0 };
    for target in 0..=max_target.min(15) as u8 {
        for lun in 0..=max_lun.min(7) as u16 {
            match c.probe_lun(target, lun) {
                Some(l) => {
                    crate::println!("  virtio-scsi: LUN {}, {} sektor (nişon {}, {} MiB)", lun, l.capacity, target, l.capacity >> 11);
                    c.luns.push(l);
                }
                None if lun == 0 => break, // LUN 0 yöq — nişon böş
                None => {}
            }
        }
        if c.luns.len() >= 8 {
            break;
        }
    }
    let n = c.luns.len();
    *CTL.lock() = Some(c);
    n
}

/// Topilgan disklar soni.
pub fn count() -> usize {
    CTL.lock().as_ref().map_or(0, |c| c.luns.len())
}

/// `i`-çi disk siğimi (sektorlarda).
pub fn capacity(i: usize) -> u64 {
    CTL.lock().as_ref().and_then(|c| c.luns.get(i)).map_or(0, |l| l.capacity)
}

/// Kontroller uzilişi va `ack_interrupt` tutqiçi; uzilişlarni yoqadi.
pub fn irq_setup() -> Option<(u32, usize)> {
    let g = CTL.lock();
    let c = g.as_ref()?;
    c.t.enable_irq();
    Some((c.t.irq(), c.t.ack_handle()))
}

/// `i`-çi diskda `lba` dan `count` sektor öqiydi/yozadi (`data` — DMA manzili).
pub fn io(i: usize, write: bool, lba: u64, count: usize, data: usize, wait: Wait) -> bool {
    if wait == Wait::Panic {
        return unsafe { CTL.force_get() }.as_mut().is_some_and(|c| c.rw(i, write, lba, count, data, wait));
    }
    CTL.lock().as_mut().is_some_and(|c| c.rw(i, write, lba, count, data, wait))
}

/// Yozuv keşini diskka tuşiradi (SYNCHRONIZE CACHE).
pub fn sync(i: usize, wait: Wait) -> bool {
    let mut g = CTL.lock();
    let Some(c) = g.as_mut() else { return false };
    let Some(l) = c.luns.get(i).copied() else { return false };
    let a = lun_addr(l.target, l.lun);
    let mut cdb = [0u8; 16];
    let n = if l.use16 {
        cdb[0] = OP_SYNC_CACHE16;
        16
    } else {
        cdb[0] = OP_SYNC_CACHE10;
        10
    };
    c.command_retry(a, &cdb[..n], None, wait)
}

/// CDB yasaş sinovi: READ(10)/(16), WRITE(10)/(16) LBA va uzunlik maydonlari, LUN manzili,
/// sense kaliti. Natija: ötgan sinovlar soni.
pub fn self_test() -> Result<usize, &'static str> {
    let (c, n) = cdb_rw(false, 0x1234_5678, 8, false);
    if n != 10 || c[0] != 0x28 || c[2..6] != [0x12, 0x34, 0x56, 0x78] || c[7..9] != [0, 8] || c[1] | c[6] | c[9] != 0 {
        return Err("READ(10) CDB xato");
    }
    let (c, n) = cdb_rw(true, 0xffff_fffe, 0x1ff, false);
    if n != 10 || c[0] != 0x2a || c[2..6] != [0xff, 0xff, 0xff, 0xfe] || c[7..9] != [0x01, 0xff] {
        return Err("WRITE(10) CDB xato");
    }
    let (c, n) = cdb_rw(false, 0x0102_0304_0506_0708, 0x0a0b_0c0d, true);
    if n != 16 || c[0] != 0x88 || c[2..10] != [1, 2, 3, 4, 5, 6, 7, 8] || c[10..14] != [0x0a, 0x0b, 0x0c, 0x0d] || c[14] | c[15] != 0 {
        return Err("READ(16) CDB xato");
    }
    let (c, _) = cdb_rw(true, 1 << 33, 1, true);
    if c[0] != 0x8a || c[2..10] != [0, 0, 0, 2, 0, 0, 0, 0] || c[10..14] != [0, 0, 0, 1] {
        return Err("WRITE(16) CDB xato");
    }
    if lun_addr(3, 0) != [1, 3, 0x40, 0, 0, 0, 0, 0] || lun_addr(0, 0x105) != [1, 0, 0x41, 0x05, 0, 0, 0, 0] {
        return Err("LUN manzili xato");
    }
    if sense_key(&[0x70, 0, 0x06, 0, 0, 0, 0, 10]) != 6 || sense_key(&[0x72, 0x05]) != 5 || sense_key(&[]) != 0 {
        return Err("sense kaliti xato");
    }
    Ok(6)
}
