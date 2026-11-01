//! GICv2 uzilişlar boşqaruvçisi drayveri.
//! QEMU virt: distributor 0x0800_0000, CPU interfeysi 0x0801_0000.

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

const GICD: usize = 0x0800_0000;
const GICC: usize = 0x0801_0000;

// Distributor registrlari
const GICD_CTLR: usize = 0x000;
const GICD_TYPER: usize = 0x004;
const GICD_ISENABLER: usize = 0x100;
const GICD_ICENABLER: usize = 0x180;
const GICD_ICPENDR: usize = 0x280;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;

// CPU interfeysi registrlari
const GICC_CTLR: usize = 0x00;
const GICC_PMR: usize = 0x04;
const GICC_BPR: usize = 0x08;
const GICC_IAR: usize = 0x0c;
const GICC_EOIR: usize = 0x10;

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

/// Uzilişlar soni (32 ga karrali).
pub fn line_count() -> u32 {
    ((rd(GICD + GICD_TYPER) & 0x1f) + 1) * 32
}

pub fn init() {
    wr(GICD + GICD_CTLR, 0);
    let lines = line_count();
    for i in 0..lines / 32 {
        wr(GICD + GICD_ICENABLER + 4 * i as usize, u32::MAX);
        wr(GICD + GICD_ICPENDR + 4 * i as usize, u32::MAX);
    }
    for id in 0..lines as usize {
        wr8(GICD + GICD_IPRIORITYR + id, 0x80);
        if id >= 32 {
            wr8(GICD + GICD_ITARGETSR + id, 1); // hammasi core 0 ga
        }
    }
    wr(GICD + GICD_CTLR, 1);

    wr(GICC + GICC_PMR, 0xff); // barça ustuvorliklarni qabul qilamiz
    wr(GICC + GICC_BPR, 0);
    wr(GICC + GICC_CTLR, 1);
}

pub fn enable(id: u32) {
    wr(GICD + GICD_ISENABLER + 4 * (id / 32) as usize, 1 << (id % 32));
}

/// Kelgan uzilişni tasdiqlaydi va uning raqamini qaytaradi.
pub fn acknowledge() -> u32 {
    rd(GICC + GICC_IAR) & 0x3ff
}

pub fn end_of_interrupt(id: u32) {
    wr(GICC + GICC_EOIR, id);
}

pub fn irqs_enable() {
    unsafe { asm!("msr daifclr, #2", options(nomem, nostack)) };
}

pub fn irqs_disable() {
    unsafe { asm!("msr daifset, #2", options(nomem, nostack)) };
}

/// Uzilişlar kelmagunça protsessorni uxlatadi (quvvatni tejaş).
/// Avval tekşiruv bajariladi — şunda "tekşirdik, keyin uziliş keldi, keyin uxladik"
/// poygasi bölmaydi: WFI maskalangan uzilişda ham uyğonadi.
pub fn wait_for_interrupt_unless(ready: impl Fn() -> bool) {
    irqs_disable();
    if !ready() {
        unsafe { asm!("wfi", options(nomem, nostack)) };
    }
    irqs_enable();
}
