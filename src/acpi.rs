//! ACPI jadvallari öquvçisi (zaxira yöl: firmware DTB bermasa).
//!
//! Ba'zi ARM VPS UEFI lari (masalan EDK2 ArmVirtQemu ACPI bilan) faqat ACPI beradi.
//! EFI konfiguratsiya jadvalidagi RSDP dan boşlab:
//!   RSDP → XSDT (yoki eski RSDT) → MADT (GIC manzillari, CPU MPIDR lari),
//!   GTDT (generic timer uzilişlari), SPCR (seriya port), MCFG (PCIe ECAM),
//!   FADT (PSCI çaqiruv usuli — HVC/SMC).
//!
//! `scan` EFI stub içida (ExitBootServices dan oldin, firmware identity xaritasida) işlaydi:
//! heap, println, panic yöq — faqat xotiradan öqiş. Natija (`AcpiInfo`) `BootInfo` ga
//! yoziladi: yadro undan qisqa hisobot çiqaradi, `efi::build_dtb` esa minimal DTB yasaydi.

use core::ptr::read_unaligned;

pub const MAX_CPUS: usize = 8;
pub const MAX_TABLES: usize = 24;

/// ACPI dan olingan, yadroga kerakli qurilma maʼlumotlari (`BootInfo` içida — repr(C)).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AcpiInfo {
    /// RSDP revision (0 — ACPI 1.0/RSDT, 2+ — XSDT).
    pub rsdp_rev: u8,
    /// 1 — XSDT, 0 — RSDT orqali.
    pub xsdt: u8,
    pub gic_version: u8,
    /// SPCR interfeys turi (3 — PL011, 0x0e — SBSA generic UART, 0x0d — SBSA 32-bit).
    pub uart_type: u8,
    pub psci_hvc: u8,
    pub ecam_end_bus: u8,
    pub ecam_start_bus: u8,
    pub _pad: u8,
    pub gicd: u64,
    /// v3: redistributorlar hududi; v2: CPU interfeysi.
    pub gicr_or_gicc: u64,
    pub gicr_len: u64,
    pub uart: u64,
    pub uart_gsiv: u32,
    /// GTDT: xavfsiz EL1, NS EL1 (fizik), virtual, NS EL2 taymer GSIV lari.
    pub timer_gsiv: [u32; 4],
    /// GTDT: shu taymerlarning bayroqlari (bit0 — fronti böyiça, bit1 — past faol).
    pub timer_flags: [u32; 4],
    pub ecam: u64,
    /// MADT dagi yoqilgan CPU lar (GICC) soni va MPIDR lari.
    pub ncpu: u32,
    pub mpidr: [u64; MAX_CPUS],
    /// Topilgan jadvallar imzolari (tartibda) va checksum xato bölganlar soni.
    pub ntables: u32,
    pub bad_checksum: u32,
    pub sigs: [[u8; 4]; MAX_TABLES],
    /// Ildiz jadval (XSDT/RSDT) OEM ID si ("BOCHS " — QEMU).
    pub oem: [u8; 6],
    pub _pad2: [u8; 2],
}

impl AcpiInfo {
    pub const EMPTY: Self = Self {
        rsdp_rev: 0,
        xsdt: 0,
        gic_version: 0,
        uart_type: 0,
        psci_hvc: 0,
        ecam_end_bus: 0,
        ecam_start_bus: 0,
        _pad: 0,
        gicd: 0,
        gicr_or_gicc: 0,
        gicr_len: 0,
        uart: 0,
        uart_gsiv: 0,
        timer_gsiv: [0; 4],
        timer_flags: [0; 4],
        ecam: 0,
        ncpu: 0,
        mpidr: [0; MAX_CPUS],
        ntables: 0,
        bad_checksum: 0,
        sigs: [[0; 4]; MAX_TABLES],
        oem: [0; 6],
        _pad2: [0; 2],
    };

    pub fn has(&self, sig: &[u8; 4]) -> bool {
        self.sigs[..(self.ntables as usize).min(MAX_TABLES)].iter().any(|s| s == sig)
    }
}

fn rd_u8(a: usize) -> u8 {
    unsafe { (a as *const u8).read() }
}
fn rd_u16(a: usize) -> u16 {
    unsafe { read_unaligned(a as *const u16) }
}
fn rd_u32(a: usize) -> u32 {
    unsafe { read_unaligned(a as *const u32) }
}
fn rd_u64(a: usize) -> u64 {
    unsafe { read_unaligned(a as *const u64) }
}

/// Baytlar yiğindisi (mod 256) nol bölişi şart.
fn checksum_ok(a: usize, len: usize) -> bool {
    let mut s = 0u8;
    for i in 0..len {
        s = s.wrapping_add(rd_u8(a + i));
    }
    s == 0
}

/// Jadval sarlavhasi to'ğri va checksum ötadimi. Natija: uzunlik.
fn table_len(t: usize) -> Option<usize> {
    if t == 0 || t % 4 != 0 {
        return None;
    }
    let len = rd_u32(t + 4) as usize;
    (36..(1 << 20)).contains(&len).then_some(len)
}

/// RSDP → XSDT/RSDT → MADT, GTDT, SPCR, MCFG, FADT. GIC distributor topilmasa — `None`.
pub fn scan(rsdp: usize) -> Option<AcpiInfo> {
    if rsdp == 0 || unsafe { core::slice::from_raw_parts(rsdp as *const u8, 8) } != b"RSD PTR " {
        return None;
    }
    // ACPI 1.0 qismi (20 bayt) checksum
    if !checksum_ok(rsdp, 20) {
        return None;
    }
    let mut a = AcpiInfo::EMPTY;
    a.rsdp_rev = rd_u8(rsdp + 15);
    let xsdt = if a.rsdp_rev >= 2 && checksum_ok(rsdp, (rd_u32(rsdp + 20) as usize).clamp(36, 64)) {
        rd_u64(rsdp + 24) as usize
    } else {
        0
    };
    let (root, entry) = if xsdt != 0 { (xsdt, 8) } else { (rd_u32(rsdp + 16) as usize, 4) };
    a.xsdt = (entry == 8) as u8;
    let root_len = table_len(root)?;
    for (k, b) in a.oem.iter_mut().enumerate() {
        *b = rd_u8(root + 10 + k);
    }
    if !checksum_ok(root, root_len) {
        a.bad_checksum += 1; // davom etamiz: ba'zi firmware lar xato checksum bilan keladi
    }
    let n = (root_len - 36) / entry;
    for i in 0..n {
        let p = root + 36 + i * entry;
        let t = if entry == 8 { rd_u64(p) as usize } else { rd_u32(p) as usize };
        let Some(len) = table_len(t) else { continue };
        let mut sig = [0u8; 4];
        for (k, b) in sig.iter_mut().enumerate() {
            *b = rd_u8(t + k);
        }
        if (a.ntables as usize) < MAX_TABLES {
            a.sigs[a.ntables as usize] = sig;
            a.ntables += 1;
        }
        if !checksum_ok(t, len) {
            a.bad_checksum += 1;
            continue;
        }
        match &sig {
            b"APIC" => madt(&mut a, t, len),
            b"GTDT" if len >= 80 => {
                for k in 0..4 {
                    a.timer_gsiv[k] = rd_u32(t + 48 + k * 8);
                    a.timer_flags[k] = rd_u32(t + 52 + k * 8);
                }
            }
            b"SPCR" if len >= 58 => {
                a.uart_type = rd_u8(t + 36);
                // Generic Address Structure: 40 — manzil maydoni turi (0 — xotira), 44 — manzil
                if rd_u8(t + 40) == 0 {
                    a.uart = rd_u64(t + 44);
                }
                // 52: uziliş turi, bit 3 — GIC GSIV
                if rd_u8(t + 52) & 8 != 0 {
                    a.uart_gsiv = rd_u32(t + 54);
                }
            }
            b"MCFG" if len >= 60 => {
                // Birinçi segment (0) yozuvi: manzil, segment, boşlanğiç va oxirgi şina
                let mut e = t + 44;
                while e + 16 <= t + len {
                    if rd_u16(e + 8) == 0 {
                        a.ecam = rd_u64(e);
                        a.ecam_start_bus = rd_u8(e + 10);
                        a.ecam_end_bus = rd_u8(e + 11);
                        break;
                    }
                    e += 16;
                }
            }
            b"FACP" if len >= 131 => {
                // ARM_BOOT_ARCH (129): bit0 — PSCI bor, bit1 — HVC orqali
                a.psci_hvc = (rd_u8(t + 129) & 2 != 0) as u8;
            }
            _ => {}
        }
    }
    // MADT dagi GICD da versiya 0 bölişi mumkin ("aniqlanmagan"): GICR bölsa — v3
    if a.gic_version == 0 {
        a.gic_version = if a.gicr_len != 0 { 3 } else { 2 };
    }
    if a.gic_version >= 3 && a.gicr_len == 0 {
        a.gicr_len = 0x20000 * a.ncpu.max(1) as u64;
    }
    (a.gicd != 0 && a.gicr_or_gicc != 0).then_some(a)
}

/// MADT (imzo "APIC"): GICC (0x0b) — har bir CPU, GICD (0x0c), GICR (0x0e).
fn madt(a: &mut AcpiInfo, t: usize, len: usize) {
    let mut p = t + 44;
    let (mut gicc, mut rd_lo, mut rd_hi, mut have_gicr) = (0u64, u64::MAX, 0u64, false);
    while p + 2 <= t + len {
        let (ty, l) = (rd_u8(p), rd_u8(p + 1) as usize);
        if l < 2 || p + l > t + len {
            break;
        }
        match ty {
            0x0b if l >= 76 => {
                let flags = rd_u32(p + 12);
                // bit0 — yoqilgan; bit3 — keyinroq yoqiş mumkin (online capable)
                if flags & 1 != 0 {
                    if gicc == 0 {
                        gicc = rd_u64(p + 32);
                    }
                    let r = rd_u64(p + 60);
                    if r != 0 {
                        rd_lo = rd_lo.min(r);
                        rd_hi = rd_hi.max(r + 0x20000);
                    }
                    let m = rd_u64(p + 68) & 0xff_00ff_ffff;
                    if (a.ncpu as usize) < MAX_CPUS {
                        a.mpidr[a.ncpu as usize] = m;
                    }
                    a.ncpu += 1;
                }
            }
            0x0c if l >= 24 => {
                a.gicd = rd_u64(p + 8);
                a.gic_version = rd_u8(p + 20);
            }
            0x0e if l >= 16 && !have_gicr => {
                a.gicr_or_gicc = rd_u64(p + 4);
                a.gicr_len = rd_u32(p + 12) as u64;
                have_gicr = true;
            }
            _ => {}
        }
        p += l;
    }
    if have_gicr {
        return;
    }
    if a.gic_version == 3 || (a.gic_version == 0 && rd_hi > 0) {
        // GICR strukturasi yöq — har bir GICC dagi redistributor manzillaridan
        if rd_hi > 0 {
            a.gicr_or_gicc = rd_lo;
            a.gicr_len = rd_hi - rd_lo;
        }
    } else if gicc != 0 {
        a.gicr_or_gicc = gicc;
    }
}

/// Qisqa hisobot (yadro jurnaliga). `out` — 4 qator matn yasaş uçun.
pub fn report(a: &AcpiInfo, mut line: impl FnMut(core::fmt::Arguments)) {
    struct Sigs<'a>(&'a AcpiInfo);
    impl core::fmt::Display for Sigs<'_> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            for (i, s) in self.0.sigs[..(self.0.ntables as usize).min(MAX_TABLES)].iter().enumerate() {
                let s = core::str::from_utf8(s).unwrap_or("????");
                write!(f, "{}{}", if i > 0 { " " } else { "" }, s)?;
            }
            Ok(())
        }
    }
    struct Cpus<'a>(&'a AcpiInfo);
    impl core::fmt::Display for Cpus<'_> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            for (i, m) in self.0.mpidr[..(self.0.ncpu as usize).min(MAX_CPUS)].iter().enumerate() {
                write!(f, "{}{:#x}", if i > 0 { " " } else { "" }, m)?;
            }
            Ok(())
        }
    }
    line(format_args!(
        "  ACPI: RSDP v{} → {}: {} jadval [{}], checksum xatosi: {}",
        a.rsdp_rev,
        if a.xsdt != 0 { "XSDT" } else { "RSDT" },
        a.ntables,
        Sigs(a),
        a.bad_checksum
    ));
    line(format_args!(
        "  ACPI MADT: GICv{}, GICD {:#x}, {} {:#x} ({} KiB), {} ta CPU (MPIDR: {})",
        a.gic_version,
        a.gicd,
        if a.gic_version >= 3 { "GICR" } else { "GICC" },
        a.gicr_or_gicc,
        if a.gic_version >= 3 { a.gicr_len >> 10 } else { 8 },
        a.ncpu,
        Cpus(a)
    ));
    if a.has(b"GTDT") {
        line(format_args!(
            "  ACPI GTDT: taymer GSIV — xavfsiz EL1 {}, NS EL1 {}, virtual {}, EL2 {}",
            a.timer_gsiv[0], a.timer_gsiv[1], a.timer_gsiv[2], a.timer_gsiv[3]
        ));
    } else {
        line(format_args!("  ACPI GTDT: yöq — taymer uzilişi standart (PPI 14 → 30)"));
    }
    line(format_args!(
        "  ACPI SPCR: {} {:#x}, GSIV {}; MCFG: ECAM {:#x} şina {}..{}; PSCI: {}",
        match a.uart_type {
            3 => "PL011",
            0x0d | 0x0e => "SBSA UART",
            0 if a.uart == 0 => "yöq",
            _ => "nomaʼlum UART",
        },
        a.uart,
        a.uart_gsiv,
        a.ecam,
        a.ecam_start_bus,
        a.ecam_end_bus,
        if a.psci_hvc != 0 { "HVC" } else { "SMC" }
    ));
}
