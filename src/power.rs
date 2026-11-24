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
const PSCI_CPU_ON: u64 = 0xc400_0003; // 64-bit

/// PSCI çaqiruvi (3 argument bilan). Natija: 0 — muvaffaqiyat.
fn psci_call3(func: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: i64;
    unsafe {
        if USE_SMC.load(Ordering::Relaxed) {
            asm!("smc #0", inout("x0") func as i64 => ret, in("x1") a1, in("x2") a2, in("x3") a3, options(nomem, nostack));
        } else {
            asm!("hvc #0", inout("x0") func as i64 => ret, in("x1") a1, in("x2") a2, in("x3") a3, options(nomem, nostack));
        }
    }
    ret
}

/// Yadroni (MPIDR = `cpu`) uyğotadi: u `entry` manzilidan, x0 = `arg` bilan boşlaydi.
pub fn cpu_on(cpu: u64, entry: usize, arg: u64) -> bool {
    psci_call3(PSCI_CPU_ON, cpu, entry as u64, arg) == 0
}

fn psci_call(func: u64) {
    unsafe {
        if USE_SMC.load(Ordering::Relaxed) {
            asm!("smc #0", inout("x0") func => _, options(nomem, nostack));
        } else {
            asm!("hvc #0", inout("x0") func => _, options(nomem, nostack));
        }
    }
}

/// Quvvat tugmasi kuzatuvçisi (yadro oqimi): tugma bosilsa — muloyim öçiriş.
pub extern "C" fn button_worker(_: usize) {
    loop {
        crate::task::sleep_ms(50);
        if crate::drivers::pwrbtn::take_pressed() {
            graceful_shutdown();
        }
    }
}

/// Muloyim öçiriş: GUI bildirişnomasi → ZaminFS va sozlamalarni diskka yoziş → PSCI SYSTEM_OFF.
pub fn graceful_shutdown() -> ! {
    use crate::i18n::pick;
    crate::println!(
        "  Quvvat tugmasi bosildi ({}, {}-marta) — muloyim öçiriş boşlandi",
        crate::drivers::pwrbtn::source(),
        crate::drivers::pwrbtn::presses()
    );
    crate::gui::notify(
        pick(&["Öçirilmoqda…", "Выключение…", "Shutting down…"]),
        pick(&[
            "Quvvat tugmasi: maʼlumotlar diskka yozilmoqda",
            "Кнопка питания: данные записываются на диск",
            "Power button: saving data to disk",
        ]),
        0xff9f0a,
    );
    crate::task::sleep_ms(1200); // bildirişnoma ekranda körinsin
    let t0 = crate::timer::uptime_ms();
    let fs = crate::fs::commit();
    // Sozlamalar faqat sozlaş ustasi tugagan bölsa (hisob bor) — aks holda böş hisob yozilmasin
    let conf = if crate::drivers::virtio_blk::present() && !crate::accounts::all().is_empty() {
        if crate::config::save_current() { "saqlandi" } else { "xato" }
    } else {
        "ötkazildi"
    };
    crate::audio::sound::stop();
    match fs {
        Ok(n) => crate::println!(
            "  Quvvat: disk sinxronlandi (ZaminFS: {} blok, sozlamalar: {}, {} ms) — PSCI SYSTEM_OFF",
            n, conf, crate::timer::uptime_ms() - t0
        ),
        Err(e) => crate::println!(
            "  Quvvat: disk sinxronlandi (ZaminFS: {:?}, sozlamalar: {}, {} ms) — PSCI SYSTEM_OFF",
            e, conf, crate::timer::uptime_ms() - t0
        ),
    }
    shutdown()
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
