//! MMU va sahifa jadvallari (4 KiB granula, 39-bit virtual manzil, 3 daraja: L1→L2→L3).
//!
//! Virtual manzil maydoni ikki yarimga bölingan:
//!   0x00_0000_0000..0x40_0000_0000  — yadro yarmi (L1 indeks 0..256), hamma jarayonda bir xil
//!       0x0000_0000..0x4000_0000    qurilmalar (UART, GIC...) — Device-nGnRnE
//!       RAM (1 GiB bloklar)         oddiy, keşlanadigan xotira (identity: VA == PA)
//!   0x40_0000_0000..0x80_0000_0000  — foydalanuvçi yarmi (L1 indeks 256..512), har jarayonda öziniki
//!
//! Har bir jarayonning öz L1 jadvali bor; uning yadro yarmi yadro jadvalidan nusxa olinadi,
//! foydalanuvçi sahifalarida esa EL0 ruxsat biti yoqiq. Yadro sahifalariga EL0 dan kiriş — taqiqlangan.

use super::{PAGE_SIZE, frame};
use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

const ENTRIES: usize = 512;
const GIB: usize = 1 << 30;
pub const VA_BITS: usize = 39;

pub const USER_BASE: usize = 0x40_0000_0000;
pub const USER_END: usize = 1 << VA_BITS;
const USER_L1_FIRST: usize = USER_BASE / GIB; // 256

#[repr(C, align(4096))]
struct Table([u64; ENTRIES]);

static mut KERNEL_L1: Table = Table([0; ENTRIES]);

// Deskriptor bitlari
const VALID: u64 = 1 << 0;
const TABLE_OR_PAGE: u64 = 1 << 1; // L1/L2 da: jadval; L3 da: sahifa
const ATTR_DEVICE: u64 = 0 << 2; // MAIR indeks 0
const ATTR_NORMAL: u64 = 1 << 2; // MAIR indeks 1
const AP_EL0: u64 = 1 << 6; // EL0 (foydalanuvçi) ham kira oladi
const AP_RO: u64 = 1 << 7; // faqat öqiş
const SH_INNER: u64 = 3 << 8;
const AF: u64 = 1 << 10;
const NG: u64 = 1 << 11; // global emas (jarayonga xos)
const PXN: u64 = 1 << 53; // EL1 bajara olmaydi
const UXN: u64 = 1 << 54; // EL0 bajara olmaydi
const ADDR_MASK: u64 = 0x0000_ffff_ffff_f000;

/// Sahifa turi.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Kind {
    /// Yadro maʼlumotlari: öqiş/yoziş, bajarib bölmaydi.
    KernelData,
    /// Yadro kodi: faqat öqiş, bajariladi.
    KernelCode,
    /// Faqat öqiş.
    KernelRo,
    /// Qurilma registrlari (MMIO).
    Device,
    /// Foydalanuvçi kodi: EL0 öqiydi va bajaradi, yoza olmaydi.
    UserCode,
    /// Foydalanuvçi maʼlumotlari / steki: EL0 öqiydi va yozadi, bajara olmaydi.
    UserData,
}

impl Kind {
    fn bits(self) -> u64 {
        match self {
            Kind::KernelData => ATTR_NORMAL | SH_INNER | PXN | UXN,
            Kind::KernelCode => ATTR_NORMAL | SH_INNER | AP_RO | UXN,
            Kind::KernelRo => ATTR_NORMAL | SH_INNER | AP_RO | PXN | UXN,
            Kind::Device => ATTR_DEVICE | PXN | UXN,
            Kind::UserCode => ATTR_NORMAL | SH_INNER | AP_EL0 | AP_RO | PXN | NG,
            Kind::UserData => ATTR_NORMAL | SH_INNER | AP_EL0 | PXN | UXN | NG,
        }
    }
}

#[derive(Debug)]
pub enum MapError {
    OutOfMemory,
    AlreadyMapped,
    BlockInTheWay,
    BadAddress,
}

fn kernel_root() -> *mut u64 {
    (&raw mut KERNEL_L1).cast()
}

pub fn kernel_root_pa() -> usize {
    kernel_root() as usize
}

const fn index(va: usize, level: usize) -> usize {
    (va >> (12 + 9 * (3 - level))) & (ENTRIES - 1)
}

/// Identity xaritani quradi va MMU + keşlarni yoqadi.
pub fn init(ram_start: usize, ram_size: usize) {
    unsafe {
        let l1 = kernel_root();
        // Qurilmalar oynasi: birinçi 1 GiB.
        write_volatile(l1, VALID | AF | Kind::Device.bits());
        // RAM: uni qoplaydigan 1 GiB bloklar (kod ham şu yerda — hozirça RWX).
        let first = ram_start / GIB;
        let last = ((ram_start + ram_size - 1) / GIB).min(USER_L1_FIRST - 1);
        for i in first..=last {
            write_volatile(l1.add(i), (i * GIB) as u64 | VALID | AF | ATTR_NORMAL | SH_INNER | UXN);
        }

        let mair: u64 = 0x00 | (0xff << 8); // 0: Device-nGnRnE, 1: Normal WB/WA
        let mmfr0: u64;
        asm!("mrs {}, id_aa64mmfr0_el1", out(reg) mmfr0);
        let ips = (mmfr0 & 0xf).min(5);
        let tcr: u64 = (64 - VA_BITS as u64) // T0SZ
            | (1 << 8)   // IRGN0: WB/WA
            | (1 << 10)  // ORGN0: WB/WA
            | (3 << 12)  // SH0: inner shareable
            | (0 << 14)  // TG0: 4 KiB
            | (1 << 23)  // EPD1: TTBR1 hozirça işlatilmaydi
            | (ips << 32);

        asm!(
            "msr mair_el1, {mair}",
            "msr tcr_el1, {tcr}",
            "msr ttbr0_el1, {ttbr}",
            "dsb ish",
            "isb",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            "mrs {tmp}, sctlr_el1",
            "orr {tmp}, {tmp}, #(1 << 0)",  // M: MMU
            "orr {tmp}, {tmp}, #(1 << 2)",  // C: maʼlumot keşi
            "orr {tmp}, {tmp}, #(1 << 12)", // I: instruksiya keşi
            "msr sctlr_el1, {tmp}",
            "isb",
            mair = in(reg) mair,
            tcr = in(reg) tcr,
            ttbr = in(reg) l1 as u64,
            tmp = out(reg) _,
        );
    }
}

/// Berilgan L1 jadvalni faol qiladi (jarayon almaştirilganda).
pub fn activate(root_pa: usize) {
    unsafe {
        asm!(
            "dsb ish",
            "msr ttbr0_el1, {}",
            "isb",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            in(reg) root_pa as u64,
        );
    }
}

unsafe fn map_in(root: *mut u64, va: usize, pa: usize, kind: Kind) -> Result<(), MapError> {
    if va % PAGE_SIZE != 0 || pa % PAGE_SIZE != 0 || va >= USER_END {
        return Err(MapError::BadAddress);
    }
    unsafe {
        let mut table = root;
        for level in 1..3 {
            let e = table.add(index(va, level));
            let v = read_volatile(e);
            table = if v & VALID == 0 {
                // Jadvallar identity xaritalangan RAMʼda, şuning uçun PA == VA.
                let next = frame::alloc_zeroed().ok_or(MapError::OutOfMemory)?;
                asm!("dsb ishst");
                write_volatile(e, next as u64 | VALID | TABLE_OR_PAGE);
                next as *mut u64
            } else if v & TABLE_OR_PAGE == 0 {
                return Err(MapError::BlockInTheWay);
            } else {
                (v & ADDR_MASK) as *mut u64
            };
        }
        let e = table.add(index(va, 3));
        if read_volatile(e) & VALID != 0 {
            return Err(MapError::AlreadyMapped);
        }
        write_volatile(e, pa as u64 | VALID | TABLE_OR_PAGE | AF | kind.bits());
        flush(va);
    }
    Ok(())
}

unsafe fn translate_in(root: *mut u64, va: usize) -> Option<usize> {
    if va >= USER_END {
        return None;
    }
    unsafe {
        let mut table = root;
        for level in 1..=3 {
            let v = read_volatile(table.add(index(va, level)));
            if v & VALID == 0 {
                return None;
            }
            let is_block = level < 3 && v & TABLE_OR_PAGE == 0;
            if is_block || level == 3 {
                let span = 1usize << (12 + 9 * (3 - level));
                return Some((v & ADDR_MASK) as usize & !(span - 1) | (va & (span - 1)));
            }
            table = (v & ADDR_MASK) as *mut u64;
        }
    }
    None
}

unsafe fn leaf_entry(root: *mut u64, va: usize) -> Option<*mut u64> {
    if va >= USER_END {
        return None;
    }
    unsafe {
        let mut table = root;
        for level in 1..3 {
            let v = read_volatile(table.add(index(va, level)));
            if v & VALID == 0 || v & TABLE_OR_PAGE == 0 {
                return None;
            }
            table = (v & ADDR_MASK) as *mut u64;
        }
        let e = table.add(index(va, 3));
        (read_volatile(e) & VALID != 0).then_some(e)
    }
}

/// Yadro jadvalida bitta 4 KiB sahifani xaritalaydi: `va` -> `pa`.
pub fn map_page(va: usize, pa: usize, kind: Kind) -> Result<(), MapError> {
    if va >= USER_BASE {
        return Err(MapError::BadAddress);
    }
    unsafe { map_in(kernel_root(), va, pa, kind) }
}

/// Yadro jadvalidan sahifani olib taşlaydi va uning fizik manzilini qaytaradi.
pub fn unmap_page(va: usize) -> Option<usize> {
    unsafe {
        let e = leaf_entry(kernel_root(), va)?;
        let v = read_volatile(e);
        write_volatile(e, 0);
        flush(va);
        Some((v & ADDR_MASK) as usize)
    }
}

/// Virtual manzilni fizikka tarjima qiladi (yadro jadvali böyiça).
pub fn translate(va: usize) -> Option<usize> {
    unsafe { translate_in(kernel_root(), va) }
}

fn flush(va: usize) {
    unsafe {
        asm!(
            "dsb ishst",
            "tlbi vaae1is, {}",
            "dsb ish",
            "isb",
            in(reg) va >> 12,
        );
    }
}

/// Jarayonning manzil maydoni. Yöq qilinganda (drop) barça foydalanuvçi
/// sahifalari va jadvallari frame allokatoriga qaytariladi.
pub struct AddressSpace {
    root: usize,
}

impl AddressSpace {
    pub fn new_user() -> Option<Self> {
        let root = frame::alloc_zeroed()?;
        unsafe {
            let k = kernel_root();
            let r = root as *mut u64;
            for i in 0..USER_L1_FIRST {
                write_volatile(r.add(i), read_volatile(k.add(i)));
            }
        }
        Some(Self { root })
    }

    pub fn root_pa(&self) -> usize {
        self.root
    }

    pub fn map(&mut self, va: usize, pa: usize, kind: Kind) -> Result<(), MapError> {
        if va < USER_BASE {
            return Err(MapError::BadAddress);
        }
        unsafe { map_in(self.root as *mut u64, va, pa, kind) }
    }

    pub fn translate(&self, va: usize) -> Option<usize> {
        if va < USER_BASE {
            return None;
        }
        unsafe { translate_in(self.root as *mut u64, va) }
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        unsafe {
            let l1 = self.root as *mut u64;
            for i in USER_L1_FIRST..ENTRIES {
                let v1 = read_volatile(l1.add(i));
                if v1 & VALID == 0 || v1 & TABLE_OR_PAGE == 0 {
                    continue;
                }
                let l2 = (v1 & ADDR_MASK) as *mut u64;
                for j in 0..ENTRIES {
                    let v2 = read_volatile(l2.add(j));
                    if v2 & VALID == 0 || v2 & TABLE_OR_PAGE == 0 {
                        continue;
                    }
                    let l3 = (v2 & ADDR_MASK) as *mut u64;
                    for k in 0..ENTRIES {
                        let v3 = read_volatile(l3.add(k));
                        if v3 & VALID != 0 {
                            frame::free((v3 & ADDR_MASK) as usize);
                        }
                    }
                    frame::free(l3 as usize);
                }
                frame::free(l2 as usize);
            }
        }
        frame::free(self.root);
    }
}
