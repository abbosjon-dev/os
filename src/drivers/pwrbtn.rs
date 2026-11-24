//! Quvvat tugmasi (muloyim öçiriş hodisasi) — Netcup SCP "Shutdown (ACPI)", `virsh shutdown`,
//! QMP `system_powerdown`.
//!
//! QEMU virt da ikki xil yöl bor:
//!   * ACPI (UEFI + ACPI jadvallari): Generic Event Device — DSDT dagi `ACPI0013`. Hodisa
//!     tanlaş registri (32-bit, MMIO) öqilganda tozalanadi; bit 1 — "quvvat tugmasi" (PWR_DOWN).
//!     Uziliş qisqa impuls (edge) — GIC da fronti böyiça sozlanadi.
//!   * DTB (`-kernel` yoki UEFI + acpi=off): `/gpio-keys` → PL061 GPIO ning 3-oyoği (KEY_POWER).
//!     PL061 da oyoq kiriş, kötariluvçi front böyiça uziliş; GIC da daraja (level) uzilişi.
//!
//! Uziliş işlovçisi faqat belgi qöyadi — diskka yoziş va öçiriş `power::button_worker` oqimida.

use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicUsize, Ordering};

const MODE_NONE: u8 = 0;
const MODE_GED: u8 = 1;
const MODE_PL061: u8 = 2;

/// GED hodisa tanlaş registri: bit 1 — quvvat tugmasi (QEMU ACPI_GED_PWR_DOWN_EVT).
const GED_EVT_SEL: usize = 0x0;
const GED_PWR_DOWN: u32 = 1 << 1;

// PL061 registrlari
const GPIODIR: usize = 0x400;
const GPIOIS: usize = 0x404;
const GPIOIBE: usize = 0x408;
const GPIOIEV: usize = 0x40c;
const GPIOIE: usize = 0x410;
const GPIOMIS: usize = 0x418;
const GPIOIC: usize = 0x41c;

static MODE: AtomicU8 = AtomicU8::new(MODE_NONE);
static BASE: AtomicUsize = AtomicUsize::new(0);
static IRQ: AtomicU32 = AtomicU32::new(0);
static PIN: AtomicU32 = AtomicU32::new(0);
static PRESSED: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicU32 = AtomicU32::new(0);

fn rd(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}
fn wr(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}

/// Quvvat tugmasini topadi va uzilişini yoqadi (`gic::init` dan keyin). Jurnalga bir qator.
pub fn init(fdt: Option<&crate::dtb::Fdt>, acpi: Option<&crate::acpi::AcpiInfo>) -> bool {
    if let Some(a) = acpi.filter(|a| a.ged_base != 0 && a.ged_gsiv >= 32) {
        let base = a.ged_base as usize;
        let _ = rd(base + GED_EVT_SEL); // eski hodisalarni tozalaymiz
        BASE.store(base, Ordering::Relaxed);
        IRQ.store(a.ged_gsiv, Ordering::Relaxed);
        MODE.store(MODE_GED, Ordering::Release);
        crate::gic::set_edge(a.ged_gsiv);
        crate::gic::enable(a.ged_gsiv);
        crate::println!(
            "  Quvvat tugmasi: GED @{:#x}, GSIV {} ({})",
            base,
            a.ged_gsiv,
            if a.ged_src == 1 { "DSDT ACPI0013" } else { "QEMU virt zaxira manzili" }
        );
        return true;
    }
    let Some(f) = fdt else {
        crate::println!("  Quvvat tugmasi: yöq (ACPI GED ham, DTB gpio-keys ham topilmadi)");
        return false;
    };
    let gpio = f.find_compatible("arm,pl061").filter(|n| n.nreg > 0);
    let pin = f.gpio_key_power();
    let (Some(g), Some(pin)) = (gpio, pin.filter(|&p| p < 8)) else {
        crate::println!("  Quvvat tugmasi: yöq (ACPI GED ham, DTB gpio-keys ham topilmadi)");
        return false;
    };
    let Some((0, spi)) = g.irq else {
        crate::println!("  Quvvat tugmasi: PL061 uzilişi DTB da yöq");
        return false;
    };
    let (base, gsiv, bit) = (g.reg[0].0 as usize, spi + 32, 1u32 << pin);
    wr(base + GPIOIE, rd(base + GPIOIE) & !bit);
    wr(base + GPIODIR, rd(base + GPIODIR) & !bit); // kiriş
    wr(base + GPIOIS, rd(base + GPIOIS) & !bit); // front böyiça
    wr(base + GPIOIBE, rd(base + GPIOIBE) & !bit); // faqat bitta front
    wr(base + GPIOIEV, rd(base + GPIOIEV) | bit); // kötariluvçi
    wr(base + GPIOIC, bit);
    BASE.store(base, Ordering::Relaxed);
    IRQ.store(gsiv, Ordering::Relaxed);
    PIN.store(pin, Ordering::Relaxed);
    MODE.store(MODE_PL061, Ordering::Release);
    wr(base + GPIOIE, rd(base + GPIOIE) | bit);
    crate::gic::enable(gsiv);
    crate::println!("  Quvvat tugmasi: PL061 GPIO @{:#x}, oyoq {} (gpio-keys KEY_POWER), GSIV {}", base, pin, gsiv);
    true
}

/// Uziliş raqami (0 — tugma yöq).
pub fn irq() -> u32 {
    if MODE.load(Ordering::Acquire) == MODE_NONE {
        0
    } else {
        IRQ.load(Ordering::Relaxed)
    }
}

/// Manba nomi (jurnal uçun).
pub fn source() -> &'static str {
    match MODE.load(Ordering::Acquire) {
        MODE_GED => "ACPI GED",
        MODE_PL061 => "PL061 gpio-keys",
        _ => "yöq",
    }
}

pub fn handle_irq() {
    let base = BASE.load(Ordering::Relaxed);
    let hit = match MODE.load(Ordering::Acquire) {
        MODE_GED => rd(base + GED_EVT_SEL) & GED_PWR_DOWN != 0,
        MODE_PL061 => {
            let bit = 1u32 << PIN.load(Ordering::Relaxed);
            let mis = rd(base + GPIOMIS);
            wr(base + GPIOIC, mis); // barça kelgan hodisalarni tozalaymiz
            mis & bit != 0
        }
        _ => false,
    };
    if hit {
        COUNT.fetch_add(1, Ordering::Relaxed);
        PRESSED.store(true, Ordering::Release);
    }
}

/// Tugma bosilganmi (va belgini tozalaydi).
pub fn take_pressed() -> bool {
    PRESSED.swap(false, Ordering::AcqRel)
}

/// Hozirgaça bosilişlar soni.
pub fn presses() -> u32 {
    COUNT.load(Ordering::Relaxed)
}
