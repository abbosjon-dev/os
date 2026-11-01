//! PSCI orqali quvvat boshqaruvi (QEMU virt'da HVC kanali).

use core::arch::asm;

const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;

fn psci_call(func: u64) {
    unsafe { asm!("hvc #0", inout("x0") func => _, options(nomem, nostack)) };
}

pub fn shutdown() -> ! {
    psci_call(PSCI_SYSTEM_OFF);
    halt()
}

#[allow(dead_code)]
pub fn reboot() -> ! {
    psci_call(PSCI_SYSTEM_RESET);
    halt()
}

pub fn halt() -> ! {
    loop {
        unsafe { asm!("wfe") };
    }
}
