//! PSCI orqali quvvat boşqaruvi.
//! Çaqiruv usuli (HVC yoki SMC) DTB dagi `/psci` tugunidan aniqlanadi.

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

static USE_SMC: AtomicBool = AtomicBool::new(false);

pub fn init(fdt: Option<&crate::dtb::Fdt>) {
    let smc = fdt
        .and_then(|f| f.top_level_prop("psci", "method"))
        .is_some_and(|m| m.starts_with(b"smc"));
    USE_SMC.store(smc, Ordering::Relaxed);
}

const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;

fn psci_call(func: u64) {
    unsafe {
        if USE_SMC.load(Ordering::Relaxed) {
            asm!("smc #0", inout("x0") func => _, options(nomem, nostack));
        } else {
            asm!("hvc #0", inout("x0") func => _, options(nomem, nostack));
        }
    }
}

pub fn shutdown() -> ! {
    psci_call(PSCI_SYSTEM_OFF);
    halt()
}

pub fn reboot() -> ! {
    psci_call(PSCI_SYSTEM_RESET);
    halt()
}

pub fn halt() -> ! {
    loop {
        unsafe { asm!("wfe") };
    }
}
