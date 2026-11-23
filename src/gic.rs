//! GIC uzilişlar boşqaruvçisi drayveri: GICv2 (MMIO CPU interfeysi) va GICv3
//! (redistributor + ICC_* tizim registrlari). Versiya va manzillar DTB dan
//! (`arm,cortex-a15-gic` / `arm,gic-400` yoki `arm,gic-v3`) olinadi; DTB bölmasa —
//! QEMU virt standarti: GICv2, distributor 0x0800_0000, CPU interfeysi 0x0801_0000.

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

static GICD: AtomicUsize = AtomicUsize::new(0x0800_0000);
/// v2: CPU interfeysi; v3: redistributorlar boşi.
static GICC_OR_GICR: AtomicUsize = AtomicUsize::new(0x0801_0000);
static VERSION: AtomicU8 = AtomicU8::new(2);
/// v3: har bir yadroning redistributori (MPIDR.Aff0 böyiça).
const MAX_CPUS: usize = 8;
static RDIST: [AtomicUsize; MAX_CPUS] = [const { AtomicUsize::new(0) }; MAX_CPUS];

// Distributor registrlari
const GICD_CTLR: usize = 0x000;
const GICD_TYPER: usize = 0x004;
const GICD_IGROUPR: usize = 0x080;
const GICD_ISENABLER: usize = 0x100;
const GICD_ICENABLER: usize = 0x180;
const GICD_ICPENDR: usize = 0x280;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;
const GICD_SGIR: usize = 0xf00;
const GICD_IROUTER: usize = 0x6000;

// GICv2 CPU interfeysi registrlari
const GICC_CTLR: usize = 0x00;
const GICC_PMR: usize = 0x04;
const GICC_BPR: usize = 0x08;
const GICC_IAR: usize = 0x0c;
const GICC_EOIR: usize = 0x10;

// GICv3 redistributor
const GICR_TYPER: usize = 0x08;
const GICR_WAKER: usize = 0x14;
const GICR_SGI: usize = 0x1_0000; // SGI/PPI sahifasi

pub const SPURIOUS: u32 = 1023;

fn rd(addr: usize) -> u32 {
    unsafe { read_volatile(addr as *const u32) }
}

fn wr(addr: usize, v: u32) {
    unsafe { write_volatile(addr as *mut u32, v) }
}

fn wr8(addr: usize, v: u8) {
    unsafe { write_volatile(addr as *mut u8, v) }
}

fn gicd() -> usize {
    GICD.load(Ordering::Relaxed)
}

fn v3() -> bool {
    VERSION.load(Ordering::Relaxed) == 3
}

fn mpidr() -> u64 {
    let m: u64;
    unsafe { asm!("mrs {}, mpidr_el1", out(reg) m) };
    m
}

/// MPIDR dan affinitet (Aff3.Aff2.Aff1.Aff0) — GICR_TYPER[63:32] formatida.
fn affinity(m: u64) -> u64 {
    (m & 0xff_ffff) | ((m >> 32 & 0xff) << 24)
}

/// DTB dan GIC versiyasi va manzillari (birinçi `init` dan oldin).
pub fn configure(fdt: &crate::dtb::Fdt) {
    if let Some(n) = fdt.find_compatible("arm,gic-v3").filter(|n| n.nreg >= 2) {
        GICD.store(n.reg[0].0 as usize, Ordering::Relaxed);
        GICC_OR_GICR.store(n.reg[1].0 as usize, Ordering::Relaxed);
        VERSION.store(3, Ordering::Relaxed);
        return;
    }
    for c in ["arm,cortex-a15-gic", "arm,gic-400", "arm,cortex-a9-gic"] {
        if let Some(n) = fdt.find_compatible(c).filter(|n| n.nreg >= 2) {
            GICD.store(n.reg[0].0 as usize, Ordering::Relaxed);
            GICC_OR_GICR.store(n.reg[1].0 as usize, Ordering::Relaxed);
            return;
        }
    }
}

pub fn version() -> u8 {
    VERSION.load(Ordering::Relaxed)
}

/// Uzilişlar soni (32 ga karrali).
pub fn line_count() -> u32 {
    (((rd(gicd() + GICD_TYPER) & 0x1f) + 1) * 32).min(1020)
}

fn wait_rwp() {
    // GICD_CTLR.RWP — yoziş kuçga kirguniça
    let mut n = 0;
    while rd(gicd() + GICD_CTLR) & (1 << 31) != 0 && n < 1_000_000 {
        n += 1;
        core::hint::spin_loop();
    }
}

/// Distributor (umumiy qism) — faqat birinçi yadro sozlaydi.
pub fn init() {
    let d = gicd();
    wr(d + GICD_CTLR, 0);
    if v3() {
        wait_rwp();
    }
    let lines = line_count();
    for i in 1..lines / 32 {
        wr(d + GICD_ICENABLER + 4 * i as usize, u32::MAX);
        wr(d + GICD_ICPENDR + 4 * i as usize, u32::MAX);
        if v3() {
            wr(d + GICD_IGROUPR + 4 * i as usize, u32::MAX); // 1-guruh (IRQ)
        }
    }
    let boot_aff = mpidr() & 0xff_00ff_ffff; // IROUTER: Aff3 [39:32], Aff2.Aff1.Aff0 [23:0]
    for id in 32..lines as usize {
        wr8(d + GICD_IPRIORITYR + id, 0x80);
        if v3() {
            unsafe { write_volatile((d + GICD_IROUTER + 8 * id) as *mut u64, boot_aff) }; // hammasi core 0 ga
        } else {
            wr8(d + GICD_ITARGETSR + id, 1);
        }
    }
    if v3() {
        // ARE + 1-guruh (va DS=1 bölsa 0-guruh): xavfsizlik holatidan qatʼi nazar bir xil qiymat
        wr(d + GICD_CTLR, (1 << 4) | (1 << 1) | 1);
        wait_rwp();
    } else {
        // v2: 0..31 ham tozalanadi (v3 da ular redistributorda)
        wr(d + GICD_ICENABLER, u32::MAX);
        wr(d + GICD_ICPENDR, u32::MAX);
        for id in 0..32 {
            wr8(d + GICD_IPRIORITYR + id, 0x80);
        }
        wr(d + GICD_CTLR, 1);
    }

    init_cpu();
}

/// Joriy yadroning redistributorini topadi (GICR_TYPER affiniteti böyiça).
fn find_rdist() -> usize {
    let me = affinity(mpidr());
    let mut r = GICC_OR_GICR.load(Ordering::Relaxed);
    for _ in 0..256 {
        let typer = unsafe { read_volatile((r + GICR_TYPER) as *const u64) };
        if typer >> 32 == me {
            return r;
        }
        if typer & (1 << 4) != 0 {
            break; // oxirgisi
        }
        r += if typer & (1 << 1) != 0 { 0x4_0000 } else { 0x2_0000 }; // VLPIS: GICv4 — 4 sahifa
    }
    GICC_OR_GICR.load(Ordering::Relaxed)
}

fn this_rdist() -> usize {
    RDIST[(mpidr() & 0xff) as usize % MAX_CPUS].load(Ordering::Relaxed)
}

/// CPU interfeysi — har bir yadroda alohida (registrlar yadroga xos).
pub fn init_cpu() {
    if !v3() {
        let c = GICC_OR_GICR.load(Ordering::Relaxed);
        wr(c + GICC_PMR, 0xff); // barça ustuvorliklarni qabul qilamiz
        wr(c + GICC_BPR, 0);
        wr(c + GICC_CTLR, 1);
        return;
    }
    let r = find_rdist();
    RDIST[(mpidr() & 0xff) as usize % MAX_CPUS].store(r, Ordering::Relaxed);
    // Uyğotiş: ProcessorSleep = 0, ChildrenAsleep tuşişini kutamiz
    wr(r + GICR_WAKER, rd(r + GICR_WAKER) & !(1 << 1));
    let mut n = 0;
    while rd(r + GICR_WAKER) & (1 << 2) != 0 && n < 1_000_000 {
        n += 1;
        core::hint::spin_loop();
    }
    let s = r + GICR_SGI;
    wr(s + GICD_IGROUPR, u32::MAX);
    wr(s + GICD_ICENABLER, u32::MAX);
    wr(s + GICD_ICPENDR, u32::MAX);
    for id in 0..32 {
        wr8(s + GICD_IPRIORITYR + id, 0x80);
    }
    unsafe {
        asm!(
            "mrs {t}, S3_0_C12_C12_5", // ICC_SRE_EL1
            "orr {t}, {t}, #1",        // SRE: tizim registrlari
            "msr S3_0_C12_C12_5, {t}",
            "isb",
            "msr S3_0_C4_C6_0, {pmr}", // ICC_PMR_EL1
            "msr S3_0_C12_C12_3, xzr", // ICC_BPR1_EL1
            "msr S3_0_C12_C12_4, xzr", // ICC_CTLR_EL1: EOImode = 0
            "msr S3_0_C12_C12_7, {one}", // ICC_IGRPEN1_EL1
            "isb",
            t = out(reg) _,
            pmr = in(reg) 0xffu64,
            one = in(reg) 1u64,
        );
    }
}

/// Qolgan barça yadrolarga dasturiy uziliş (SGI) yuboradi.
pub fn send_sgi_others(id: u32) {
    unsafe { asm!("dsb ishst") };
    if v3() {
        let v: u64 = (1 << 40) | (((id & 0xf) as u64) << 24); // IRM = 1: özidan boşqa hammaga
        unsafe { asm!("msr S3_0_C12_C11_5, {}", "isb", in(reg) v) }; // ICC_SGI1R_EL1
    } else {
        wr(gicd() + GICD_SGIR, (1 << 24) | (id & 0xf));
    }
}

/// Uzilişni yoqadi. SGI/PPI (0..31) — joriy yadroda.
pub fn enable(id: u32) {
    if v3() && id < 32 {
        wr(this_rdist() + GICR_SGI + GICD_ISENABLER, 1 << id);
        return;
    }
    wr(gicd() + GICD_ISENABLER + 4 * (id / 32) as usize, 1 << (id % 32));
}

/// Kelgan uzilişni tasdiqlaydi. Natija: IAR ning töliq qiymati —
/// uni `end_of_interrupt` ga özgarişsiz beriş kerak (SGI da yuboruvçi yadro raqami bor).
pub fn acknowledge() -> u32 {
    if v3() {
        let v: u64;
        unsafe { asm!("mrs {}, S3_0_C12_C12_0", out(reg) v) }; // ICC_IAR1_EL1
        return v as u32;
    }
    rd(GICC_OR_GICR.load(Ordering::Relaxed) + GICC_IAR)
}

pub fn end_of_interrupt(iar: u32) {
    if v3() {
        unsafe { asm!("msr S3_0_C12_C12_1, {}", in(reg) iar as u64) }; // ICC_EOIR1_EL1
        return;
    }
    wr(GICC_OR_GICR.load(Ordering::Relaxed) + GICC_EOIR, iar);
}

pub fn irqs_enable() {
    unsafe { asm!("msr daifclr, #2", options(nomem, nostack)) };
}
