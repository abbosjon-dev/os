//! PCIe (ECAM) şinasi: qurilmalarni sanab çiqiş va virtio-pci qurilmalarini topiş.
//!
//! ECAM manzili DTB dagi `pci-host-ecam-generic` tugunidan (ACPI rejimida — MCFG dan
//! yasalgan tugundan) olinadi. UEFI BAR larni allaqaçon tayinlagan böladi; `-kernel`
//! bilan yuklanganda (firmware yöq) böş BAR larga DTB `ranges` dagi 32-bit MMIO oynadan
//! manzil beriladi. INTx uzilişi: QEMU virt (va unga asoslangan KVM VPS) sxemasi —
//! INTA..INTD → SPI 3..6, qurilma raqami böyiça aylantiriladi (swizzle).

use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};

use crate::sync::SpinLock;

const VIRTIO_VENDOR: u16 = 0x1af4;

/// Topilgan virtio-pci qurilmasi.
#[derive(Clone, Copy)]
pub struct VirtioPci {
    /// virtio qurilma turi (2 — blk, 4 — rng, 16 — gpu…)
    pub device_type: u32,
    /// Konfiguratsiya maydoni (ECAM) VA si.
    pub cfg: usize,
    /// BAR lar VA si (0 — yöq).
    pub bars: [usize; 6],
    /// GIC INTID (INTx).
    pub irq: u32,
}

static DEVICES: SpinLock<Vec<VirtioPci>> = SpinLock::new(Vec::new());

/// PCI sinf kodi (sinf, kiçik sinf, interfeys): xHCI USB kontrolleri.
pub const CLASS_XHCI: u32 = 0x0c_03_30;

/// Topilgan xHCI (USB 3) kontrolleri: konfiguratsiya maydoni va BAR0 (MMIO) VA si.
#[derive(Clone, Copy)]
pub struct XhciPci {
    pub bar0: usize,
    pub vendor: u16,
    pub device: u16,
}

static XHCI: SpinLock<Vec<XhciPci>> = SpinLock::new(Vec::new());

pub fn cfg_rd32(cfg: usize, off: usize) -> u32 {
    unsafe { read_volatile((cfg + off) as *const u32) }
}
pub fn cfg_wr32(cfg: usize, off: usize, v: u32) {
    unsafe { write_volatile((cfg + off) as *mut u32, v) }
}
pub fn cfg_rd16(cfg: usize, off: usize) -> u16 {
    unsafe { read_volatile((cfg + off) as *const u16) }
}
pub fn cfg_wr16(cfg: usize, off: usize, v: u16) {
    unsafe { write_volatile((cfg + off) as *mut u16, v) }
}
pub fn cfg_rd8(cfg: usize, off: usize) -> u8 {
    unsafe { read_volatile((cfg + off) as *const u8) }
}

fn be(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]])
}

/// DTB dagi 32-bit MMIO oynasi (pci manzil, cpu manzil, hajm) — BAR tayinlaş uçun.
fn mem32_window(ranges: &[u8]) -> Option<(u64, u64, u64)> {
    // (phys.hi, phys.mid, phys.lo) (cpu: 2 hujayra) (hajm: 2 hujayra) = 7 hujayra
    let n = ranges.len() / 28;
    (0..n).find_map(|k| {
        let c = &ranges[k * 28..];
        let space = (be(c, 0) >> 24) & 3;
        let pci = (be(c, 1) as u64) << 32 | be(c, 2) as u64;
        let cpu = (be(c, 3) as u64) << 32 | be(c, 4) as u64;
        let size = (be(c, 5) as u64) << 32 | be(c, 6) as u64;
        (space == 2).then_some((pci, cpu, size))
    })
}

/// PCIe ni sanab çiqadi. MMU va heap tayyor bölgandan keyin, jarayonlardan oldin.
/// Natija: topilgan virtio qurilmalar soni.
pub fn init(fdt: Option<&crate::dtb::Fdt>) -> usize {
    let Some(node) = fdt.and_then(|f| f.find_compatible("pci-host-ecam-generic")) else { return 0 };
    if node.nreg == 0 {
        return 0;
    }
    let (ecam_pa, ecam_size) = (node.reg[0].0 as usize, node.reg[0].1 as usize);
    let Some(ecam) = crate::memory::mmu::ioremap(ecam_pa, ecam_size) else { return 0 };
    let buses = (ecam_size >> 20).clamp(1, 256);
    // BAR tayinlaş oynasi (faqat DTB da `ranges` bölsa); firmware tayinlaganlarini tegmaymiz
    let mut window = mem32_window(node.ranges);
    let mut found = Vec::new();
    let mut usb = Vec::new();
    for bus in 0..buses {
        for dev in 0..32 {
            for func in 0..8 {
                let cfg = ecam + (bus << 20 | dev << 15 | func << 12);
                let id = cfg_rd32(cfg, 0);
                if id & 0xffff == 0xffff {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let vendor = id as u16;
                let device = (id >> 16) as u16;
                let header = cfg_rd8(cfg, 0x0e);
                if vendor == VIRTIO_VENDOR && (0x1000..=0x107f).contains(&device) && header & 0x7f == 0 {
                    let device_type = if device >= 0x1040 {
                        (device - 0x1040) as u32
                    } else {
                        cfg_rd16(cfg, 0x2e) as u32 // eski (transitional): subsystem ID = tur
                    };
                    let bars = setup_bars(cfg, &mut window);
                    let pin = cfg_rd8(cfg, 0x3d) as usize; // 1 = INTA
                    let irq = if pin == 0 { 0 } else { 32 + 3 + ((dev + pin - 1) % 4) as u32 };
                    // Xotira + bus master yoqiq, INTx hozirça öçiq (drayver kerak bölsa yoqadi)
                    let cmd = cfg_rd16(cfg, 0x04);
                    cfg_wr16(cfg, 0x04, (cmd | 0x2 | 0x4 | (1 << 10)) & !0x1);
                    found.push(VirtioPci { device_type, cfg, bars, irq });
                } else if cfg_rd32(cfg, 0x08) >> 8 == CLASS_XHCI && header & 0x7f == 0 {
                    // xHCI: BAR0 (64-bit MMIO); uzilişlar işlatilmaydi — hodisa halqasi sörov bilan
                    let bars = setup_bars(cfg, &mut window);
                    let cmd = cfg_rd16(cfg, 0x04);
                    cfg_wr16(cfg, 0x04, cmd | 0x2 | 0x4 | (1 << 10));
                    if bars[0] != 0 {
                        usb.push(XhciPci { bar0: bars[0], vendor, device });
                    }
                }
                if func == 0 && header & 0x80 == 0 {
                    break; // bir funksiyali qurilma
                }
            }
        }
    }
    let n = found.len();
    *DEVICES.lock() = found;
    *XHCI.lock() = usb;
    n
}

/// `n`-çi xHCI kontrolleri (PCI sinfi 0x0C0330).
pub fn xhci_nth(n: usize) -> Option<XhciPci> {
    XHCI.lock().get(n).copied()
}

/// BAR larni öqiydi (kerak bölsa tayinlaydi) va ularni VA ga xaritalaydi.
fn setup_bars(cfg: usize, window: &mut Option<(u64, u64, u64)>) -> [usize; 6] {
    let mut out = [0usize; 6];
    // tayinlaş paytida dekodlaşni öçiramiz
    let cmd = cfg_rd16(cfg, 0x04);
    cfg_wr16(cfg, 0x04, cmd & !0x3);
    let mut i = 0;
    while i < 6 {
        let off = 0x10 + i * 4;
        let lo = cfg_rd32(cfg, off);
        if lo & 1 != 0 {
            i += 1; // I/O BAR — işlatmaymiz
            continue;
        }
        let is64 = (lo >> 1) & 3 == 2;
        let hi = if is64 { cfg_rd32(cfg, off + 4) } else { 0 };
        // hajmni aniqlaymiz
        cfg_wr32(cfg, off, u32::MAX);
        let mut mask = (cfg_rd32(cfg, off) & !0xf) as u64;
        if is64 {
            cfg_wr32(cfg, off + 4, u32::MAX);
            mask |= (cfg_rd32(cfg, off + 4) as u64) << 32;
        } else {
            mask |= 0xffff_ffff_0000_0000;
        }
        cfg_wr32(cfg, off, lo);
        if is64 {
            cfg_wr32(cfg, off + 4, hi);
        }
        let size = (!mask).wrapping_add(1);
        let mut addr = ((hi as u64) << 32) | (lo & !0xf) as u64;
        if mask != 0 && size != 0 {
            if addr == 0 {
                // firmware tayinlamagan: oynadan (hajmga tekislangan) joy beramiz
                if let Some((pci, cpu, wsize)) = window.as_mut() {
                    let a = (*pci).div_ceil(size) * size;
                    if a + size <= *pci + *wsize {
                        let used = a + size - *pci;
                        let cpu_a = *cpu + (a - *pci);
                        *pci += used;
                        *cpu += used;
                        *wsize -= used;
                        cfg_wr32(cfg, off, a as u32 | (lo & 0xf));
                        if is64 {
                            cfg_wr32(cfg, off + 4, (a >> 32) as u32);
                        }
                        addr = cpu_a;
                    }
                }
            }
            if addr != 0 {
                out[i] = crate::memory::mmu::ioremap(addr as usize, size as usize).unwrap_or(0);
            }
        }
        i += if is64 { 2 } else { 1 };
    }
    cfg_wr16(cfg, 0x04, cmd | 0x2);
    out
}

/// Berilgan turdagi `n`-çi virtio-pci qurilmasi.
pub fn virtio_nth(device_type: u32, n: usize) -> Option<VirtioPci> {
    DEVICES.lock().iter().filter(|d| d.device_type == device_type).nth(n).copied()
}

/// INTx ni yoqadi / öçiradi (Command registri, bit 10).
pub fn set_intx(cfg: usize, on: bool) {
    let cmd = cfg_rd16(cfg, 0x04);
    cfg_wr16(cfg, 0x04, if on { cmd & !(1 << 10) } else { cmd | (1 << 10) });
}

/// Topilgan virtio-pci qurilmalar (tur) — yuklaniş xabari uçun.
pub fn virtio_types() -> Vec<u32> {
    DEVICES.lock().iter().map(|d| d.device_type).collect()
}
