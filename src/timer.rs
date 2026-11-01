//! ARM generic timer (EL1 fizik taymer, PPI 14 -> INTID 30).
//! Har 10 ms da uziliş beradi (100 Hz) — kelajakda rejalaştiruvçi şu "tick"da işlaydi.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

pub const IRQ: u32 = 30;
pub const HZ: u64 = 100;

static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn frequency() -> u64 {
    let f: u64;
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) f) };
    f
}

pub fn counter() -> u64 {
    let c: u64;
    unsafe { asm!("isb", "mrs {}, cntpct_el0", out(reg) c) };
    c
}

fn arm() {
    let interval = frequency() / HZ;
    unsafe {
        asm!("msr cntp_tval_el0, {}", in(reg) interval);
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64); // yoqilgan, maskalanmagan
    }
}

pub fn init() {
    arm();
    crate::gic::enable(IRQ);
}

pub fn handle_irq() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    arm();
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Tizim işga tuşganidan beri ötgan vaqt (millisekund).
pub fn uptime_ms() -> u64 {
    counter() / (frequency() / 1000)
}
