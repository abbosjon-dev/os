//! MMU va sahifa jadvallari (4 KiB granula, 39-bit virtual manzil, 3 daraja: L1→L2→L3).
//!
//! Boşlangʼiç xarita (identity: VA == PA):
//!   0x0000_0000..0x4000_0000  — qurilmalar (UART, GIC...) — Device-nGnRnE, bajarib bölmaydi
//!   RAM (1 GiB bloklar)       — oddiy, keşlanadigan xotira

use super::frame;
use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

const ENTRIES: usize = 512;
const GIB: usize = 1 << 30;
pub const VA_BITS: usize = 39;

#[repr(C, align(4096))]
struct Table([u64; ENTRIES]);

static mut L1: Table = Table([0; ENTRIES]);

// Deskriptor bitlari
const VALID: u64 = 1 << 0;
const TABLE_OR_PAGE: u64 = 1 << 1; // L1/L2 da: jadval; L3 da: sahifa
const ATTR_DEVICE: u64 = 0 << 2; // MAIR indeks 0
const ATTR_NORMAL: u64 = 1 << 2; // MAIR indeks 1
const AP_RO: u64 = 1 << 7;
const SH_INNER: u64 = 3 << 8;
const AF: u64 = 1 << 10;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;
const ADDR_MASK: u64 = 0x0000_ffff_ffff_f000;

/// Sahifa turi.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Kind {
    /// Kernel maʼlumotlari: öqiş/yoziş, bajarib bölmaydi.
    KernelData,
    /// Kernel kodi: faqat öqiş, bajariladi.
    KernelCode,
    /// Faqat öqiş.
    KernelRo,
    /// Qurilma registrlari (MMIO).
    Device,
}

impl Kind {
    fn bits(self) -> u64 {
        match self {
            Kind::KernelData => ATTR_NORMAL | SH_INNER | PXN | UXN,
            Kind::KernelCode => ATTR_NORMAL | SH_INNER | AP_RO | UXN,
            Kind::KernelRo => ATTR_NORMAL | SH_INNER | AP_RO | PXN | UXN,
            Kind::Device => ATTR_DEVICE | PXN | UXN,
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

fn l1() -> *mut u64 {
    (&raw mut L1).cast()
}

const fn index(va: usize, level: usize) -> usize {
    (va >> (12 + 9 * (3 - level))) & (ENTRIES - 1)
}

/// Identity xaritani quradi va MMU + keşlarni yoqadi.
pub fn init(ram_start: usize, ram_size: usize) {
    unsafe {
        // Qurilmalar oynasi: birinçi 1 GiB.
        write_volatile(l1(), VALID | AF | Kind::Device.bits());
        // RAM: uni qoplaydigan 1 GiB bloklar (kod ham şu yerda — hozirça RWX).
        let first = ram_start / GIB;
        let last = (ram_start + ram_size - 1) / GIB;
        for i in first..=last {
            write_volatile(l1().add(i), (i * GIB) as u64 | VALID | AF | ATTR_NORMAL | SH_INNER | UXN);
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
            "orr {tmp}, {tmp}, #(1 << 2)",  // C: data keş
            "orr {tmp}, {tmp}, #(1 << 12)", // I: instruksiya keş
            "msr sctlr_el1, {tmp}",
            "isb",
            mair = in(reg) mair,
            tcr = in(reg) tcr,
            ttbr = in(reg) l1() as u64,
            tmp = out(reg) _,
        );
    }
}

/// Bitta 4 KiB sahifani xaritalaydi: `va` -> `pa`.
/// Kerakli L2/L3 jadvallar frame allokatoridan olinadi.
pub fn map_page(va: usize, pa: usize, kind: Kind) -> Result<(), MapError> {
    if va % 4096 != 0 || pa % 4096 != 0 || va >= 1 << VA_BITS {
        return Err(MapError::BadAddress);
    }
    unsafe {
        let mut table = l1();
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

/// Sahifani xaritadan olib taşlaydi va uning fizik manzilini qaytaradi.
pub fn unmap_page(va: usize) -> Option<usize> {
    let e = leaf_entry(va)?;
    unsafe {
        let v = read_volatile(e);
        write_volatile(e, 0);
        flush(va);
        Some((v & ADDR_MASK) as usize)
    }
}

/// Virtual manzilni fizikka tarjima qiladi (dasturiy jadval yuriş).
pub fn translate(va: usize) -> Option<usize> {
    if va >= 1 << VA_BITS {
        return None;
    }
    unsafe {
        let mut table = l1();
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

fn leaf_entry(va: usize) -> Option<*mut u64> {
    if va >= 1 << VA_BITS {
        return None;
    }
    unsafe {
        let mut table = l1();
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
