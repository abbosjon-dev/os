//! Protsessor imkoniyatlari (ID_AA64* registrlari) — yuklanişda jurnalga yoziladi.
//!
//! Yadro faqat ARMv8.0 asosiga tayanadi: PAN (v8.1), RNDR (v8.5) va GICv3 tizim registrlari
//! işlatilişidan oldin ID registrlari böyiça tekşiriladi (qarang: `mmu::enable_pan`,
//! `random::rndr_supported`, `boot.s`). Atomik amallar LDXR/STXR (LSE talab qilinmaydi —
//! target `aarch64-unknown-none-softfloat` standarti). Netcup ARM (Ampere Altra, Neoverse-N1:
//! v8.2, RNDR/PAuth/BTI/MTE yöq) va QEMU `-cpu max` ikkalasida bir xil kod işlaydi.

use core::arch::asm;

macro_rules! sysreg {
    ($name:literal) => {{
        let v: u64;
        unsafe { asm!(concat!("mrs {}, ", $name), out(reg) v) };
        v
    }};
}

fn f(v: u64, sh: u32) -> u64 {
    (v >> sh) & 0xf
}

fn yn(b: bool) -> &'static str {
    if b { "ha" } else { "yöq" }
}

/// Bir qatorli hisobot (heap kerak emas — yuklanişning boşida): MIDR, ID_AA64* dan PAN, LSE, RNDR, PAuth, BTI, MTE, PA kengligi.
pub fn report() {
    let midr = sysreg!("midr_el1");
    let isar0 = sysreg!("id_aa64isar0_el1");
    let isar1 = sysreg!("id_aa64isar1_el1");
    let pfr0 = sysreg!("id_aa64pfr0_el1");
    let pfr1 = sysreg!("id_aa64pfr1_el1");
    let mmfr0 = sysreg!("id_aa64mmfr0_el1");
    let mmfr1 = sysreg!("id_aa64mmfr1_el1");
    let (imp, part) = ((midr >> 24) & 0xff, (midr >> 4) & 0xfff);
    let name = match (imp, part) {
        (0x41, 0xd0c) => "Neoverse-N1",
        (0x41, 0xd49) => "Neoverse-N2",
        (0x41, 0xd40) => "Neoverse-V1",
        (0x41, 0xd08) => "Cortex-A72",
        (0x41, 0xd07) => "Cortex-A57",
        (0x41, 0xd03) => "Cortex-A53",
        (0xc0, _) => "Ampere",
        (0, 0) => "QEMU max",
        _ => "boşqa",
    };
    let pa = match mmfr0 & 0xf {
        0 => 32,
        1 => 36,
        2 => 40,
        3 => 42,
        4 => 44,
        5 => 48,
        _ => 52,
    };
    let pauth = f(isar1, 4) | f(isar1, 8) | f(isar1, 24) | f(isar1, 28) != 0;
    crate::println!(
        "  CPU: MIDR {:#010x} ({}), ID_AA64: PAN {}, LSE {}, RNDR {}, PAuth {}, BTI {}, MTE {}, GICv3 sysreg {}, PA {} bit — yadro faqat ARMv8.0 asosiga tayanadi",
        midr,
        name,
        yn(f(mmfr1, 20) != 0),
        yn(f(isar0, 20) >= 2),
        yn(f(isar0, 60) != 0),
        yn(pauth),
        yn(f(pfr1, 0) != 0),
        yn(f(pfr1, 8) != 0),
        yn(f(pfr0, 24) != 0),
        pa
    );
}
