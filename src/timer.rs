//! ARM generic timer (EL1 fizik taymer, PPI 14 -> INTID 30).
//! Har 10 ms da uziliş beradi (100 Hz) — kelajakda rejalaştiruvçi şu "tick"da işlaydi.

use core::arch::asm;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

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

/// Haqiqiy vaqt manbai. Tartib: PL031 (DTB dagi `arm,pl031`) → UEFI GetTime (yuklaniş
/// paytidagi qiymat + ARM taymer hisoblagiçi bilan ösiş) → 0 (soat yöq).
/// ACPI yölida (Netcup) DTB yaşirilgan va ACPI dan yasalgan DTB da RTC yöq — UEFI işlatiladi.
static RTC_BASE: AtomicUsize = AtomicUsize::new(0);
static EFI_UNIX: AtomicU64 = AtomicU64::new(0);
static EFI_CNT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimeSource {
    Pl031,
    Uefi,
    None,
}

impl TimeSource {
    pub fn name(self) -> &'static str {
        match self {
            TimeSource::Pl031 => "PL031",
            TimeSource::Uefi => "UEFI",
            TimeSource::None => "yöq",
        }
    }
}

/// Manbani tanlaydi (MMU yoqilgandan keyin: PL031 0..1 GiB qurilmalar hududida bölişi kerak).
pub fn configure_rtc(fdt: Option<&crate::dtb::Fdt>, efi_time: Option<(u64, u64)>) -> TimeSource {
    let pl031 = fdt
        .and_then(|f| f.find_compatible("arm,pl031"))
        .filter(|n| n.nreg > 0)
        .map(|n| n.reg[0].0 as usize)
        .filter(|&b| b != 0 && b < 0x4000_0000);
    if let Some(b) = pl031 {
        // 0 — soat işlamayapti (yoki sozlanmagan); UEFI vaqti bölsa — u işonçliroq
        let v = unsafe { core::ptr::read_volatile(b as *const u32) };
        if v != 0 || efi_time.is_none() {
            RTC_BASE.store(b, Ordering::Relaxed);
            return TimeSource::Pl031;
        }
    }
    if let Some((u, c)) = efi_time.filter(|&(u, _)| u != 0) {
        EFI_UNIX.store(u, Ordering::Relaxed);
        EFI_CNT.store(c, Ordering::Relaxed);
        return TimeSource::Uefi;
    }
    TimeSource::None
}

/// Hozirgi vaqt (Unix soniyalar, UTC). Soat bölmasa — 0.
pub fn unix_time() -> u64 {
    let b = RTC_BASE.load(Ordering::Relaxed);
    if b != 0 {
        return unsafe { core::ptr::read_volatile(b as *const u32) as u64 };
    }
    let u = EFI_UNIX.load(Ordering::Relaxed);
    if u == 0 {
        return 0;
    }
    let f = frequency().max(1);
    u + counter().wrapping_sub(EFI_CNT.load(Ordering::Relaxed)) / f
}
