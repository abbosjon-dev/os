//! ARM generic timer (EL1 fizik taymer, PPI 14 -> INTID 30).
//! Har 10 ms da uziliş beradi (100 Hz) — kelajakda rejalaştiruvçi şu "tick"da işlaydi.

use core::arch::asm;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Standart: NS EL1 fizik taymer, PPI 14 → INTID 30. DTB `/timer` (yoki ACPI GTDT dan
/// yasalgan) boşqaça desa — `configure` özgartiradi.
pub const DEFAULT_IRQ: u32 = 30;
static IRQ: AtomicU32 = AtomicU32::new(DEFAULT_IRQ);

/// Taymer uzilişi (INTID).
pub fn irq() -> u32 {
    IRQ.load(Ordering::Relaxed)
}

/// DTB dagi `arm,armv8-timer` tugunining 2-uçligi (NS EL1 fizik taymer PPI si).
pub fn configure(fdt: &crate::dtb::Fdt) {
    let n = fdt.find_compatible("arm,armv8-timer").or_else(|| fdt.find_compatible("arm,armv7-timer"));
    if let Some((1, ppi, _)) = n.and_then(|n| n.irq_at(1)) {
        if ppi < 16 {
            IRQ.store(ppi + 16, Ordering::Relaxed);
        }
    }
}
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
    crate::gic::enable(irq());
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

/// PL031 haqiqiy vaqt soati (QEMU virt: 0x0901_0000). RTCDR — 1970-yildan beri soniyalar.
const RTC_BASE: usize = 0x0901_0000;

/// Hozirgi vaqt (Unix soniyalar, UTC). Soat bölmasa — 0.
pub fn unix_time() -> u64 {
    unsafe { core::ptr::read_volatile(RTC_BASE as *const u32) as u64 }
}
