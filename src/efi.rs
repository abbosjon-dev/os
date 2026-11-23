//! UEFI kiriş nuqtasi (EFI stub): yadro tasvirining özi EFI ilovasi (BOOTAA64.EFI).
//!
//! `scripts/mkefi.py` ELF yadrodan PE/COFF fayl yasaydi: tasvir bazasi — yadro bağlangan
//! manzil (0x4020_0000) dan 64 KiB oldin, relokatsiyalar yöq, şuning uçun firmware uni
//! aynan bağlangan manzilga yuklaydi va kod özgarişsiz işlaydi (UEFI identity xarita).
//!
//! `efi_main`:
//!   1) EFI konfiguratsiya jadvalidan DTB ni (GUID b1b621d5-…) topadi va uni yadro
//!      içidagi `__boot_dtb` sohasiga nusxalaydi,
//!   2) EFI xotira xaritasidan RAM da bölmagan / band (runtime, ACPI NVS, MMIO…) hududlarni
//!      `__bootinfo` ga yozadi — frame allokatori ularga tegmaydi,
//!   3) ExitBootServices, keyin MMU/keşni öçirib (`efi_handoff`) odatiy `_start` ga ötadi
//!      (x0 = DTB manzili) — xuddi `-kernel` bilan yuklangandek.

use crate::acpi::{self, AcpiInfo};
use core::ptr::{copy, read_unaligned};

unsafe extern "C" {
    static mut __bootinfo: BootInfo;
    static mut __boot_dtb: u8;
    static __boot_dtb_end: u8;
    static __kernel_start: u8;
    static __kernel_end: u8;
    fn efi_handoff(dtb: usize, clean_start: usize, clean_end: usize) -> !;
}

pub const BOOTINFO_MAGIC: u64 = 0x5a41_4d49_4e45_4649; // "ZAMINEFI"
pub const MAX_RESERVED: usize = 64;

/// EFI yuklovçidan yadroga uzatiladigan maʼlumot (bss dan taşqarida — nollanmaydi).
#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    /// EFI xaritasidagi oddiy (işlatiş mumkin bölgan) RAM: eng past va eng yuqori manzil.
    pub ram_lo: u64,
    pub ram_hi: u64,
    pub efi_regions: u64,
    pub reserved_count: u64,
    /// (boşlaniş, oxir) — işlatib bölmaydigan hududlar.
    pub reserved: [[u64; 2]; MAX_RESERVED],
    /// 1 — DTB yöq edi, qurilma tavsifi ACPI dan yasaldi (`acpi` toʻldirilgan).
    pub acpi_used: u64,
    pub acpi: AcpiInfo,
}

impl BootInfo {
    /// ACPI zaxira yöli işlatilgan bölsa — öqilgan jadvallar.
    pub fn acpi(&self) -> Option<&AcpiInfo> {
        (self.acpi_used == 1).then_some(&self.acpi)
    }
}

#[repr(C)]
struct TableHeader {
    signature: u64,
    revision: u32,
    header_size: u32,
    crc32: u32,
    reserved: u32,
}

#[repr(C)]
struct SystemTable {
    hdr: TableHeader,
    firmware_vendor: *const u16,
    firmware_revision: u32,
    console_in_handle: usize,
    con_in: usize,
    console_out_handle: usize,
    con_out: *const TextOutput,
    stderr_handle: usize,
    stderr: usize,
    runtime_services: usize,
    boot_services: *const BootServices,
    table_entries: usize,
    config_table: *const ConfigTable,
}

#[repr(C)]
struct TextOutput {
    reset: usize,
    output_string: extern "efiapi" fn(*const TextOutput, *const u16) -> usize,
}

type GetMemoryMap = extern "efiapi" fn(*mut usize, *mut u8, *mut usize, *mut usize, *mut u32) -> usize;
type ExitBootServices = extern "efiapi" fn(usize, usize) -> usize;

#[repr(C)]
struct BootServices {
    hdr: TableHeader,
    /// RaiseTPL … ExitBootServices (26 ta) — bizga faqat 4 (GetMemoryMap) va 26 kerak.
    fns: [usize; 27],
}

#[repr(C)]
struct ConfigTable {
    guid: [u8; 16],
    table: usize,
}

/// EFI_DTB_TABLE_GUID b1b621d5-f19c-41a5-830b-d9152c69aae0 (xotiradagi tartibda).
const DTB_GUID: [u8; 16] = [
    0xd5, 0x21, 0xb6, 0xb1, 0x9c, 0xf1, 0xa5, 0x41, 0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0,
];

/// EFI_ACPI_20_TABLE_GUID 8868e871-e4f1-11d3-bc22-0080c73c8881.
const ACPI20_GUID: [u8; 16] = [
    0x71, 0xe8, 0x68, 0x88, 0xf1, 0xe4, 0xd3, 0x11, 0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81,
];

const EFI_SUCCESS: usize = 0;
const EFI_LOAD_ERROR: usize = (1 << 63) | 1;

/// Xotira xaritasi uçun bufer (.bss — PE yuklovçi uni nollaydi; `_start` keyin yana nollaydi).
static mut MAP: [u8; 32 * 1024] = [0; 32 * 1024];

fn say(st: &SystemTable, s: &str) {
    if st.con_out.is_null() {
        return;
    }
    let mut buf = [0u16; 160];
    let mut n = 0;
    for c in s.chars() {
        if n + 3 >= buf.len() {
            break;
        }
        if c == '\n' {
            buf[n] = b'\r' as u16;
            n += 1;
        }
        buf[n] = if (c as u32) < 0x10000 { c as u16 } else { b'?' as u16 };
        n += 1;
    }
    buf[n] = 0;
    unsafe { ((*st.con_out).output_string)(st.con_out, buf.as_ptr()) };
}

fn be32(p: *const u8) -> u32 {
    u32::from_be_bytes(unsafe { read_unaligned(p.cast::<[u8; 4]>()) })
}

/// Xarita turi işlatiş mumkin bölgan oddiy RAM mi (ExitBootServices dan keyin).
fn usable(ty: u32) -> bool {
    // 1/2 — yuklovçi kodi/maʼlumoti, 3/4 — boot services, 7 — böş
    matches!(ty, 1 | 2 | 3 | 4 | 7)
}

/// RAM hisoblanadigan, lekin işlatib bölmaydigan turlar (runtime, ACPI, band, PAL…).
fn reserved_ram(ty: u32) -> bool {
    matches!(ty, 0 | 5 | 6 | 8 | 9 | 10 | 13 | 14)
}

#[unsafe(no_mangle)]
extern "efiapi" fn efi_main(image: usize, st: *const SystemTable) -> usize {
    let st = unsafe { &*st };
    say(st, "Zamin OS: UEFI yuklovchi (BOOTAA64.EFI)\n");

    // 1) DTB
    let (mut dtb, mut rsdp) = (0usize, 0usize);
    for i in 0..st.table_entries {
        let e = unsafe { &*st.config_table.add(i) };
        if e.guid == DTB_GUID {
            dtb = e.table;
        } else if e.guid == ACPI20_GUID {
            rsdp = e.table;
        }
    }
    let dst = &raw mut __boot_dtb;
    let room = &raw const __boot_dtb_end as usize - dst as usize;
    let mut acpi = None;
    if dtb == 0 || be32(dtb as *const u8) != 0xd00d_feed {
        dtb = 0;
        // ACPI rejimi (EDK2 ACPI bölganda DTB bermaydi): kerakli qismlarni ACPI dan olamiz
        match acpi::scan(rsdp) {
            Some(a) => {
                say(st, "Zamin OS: DTB yo'q - ACPI jadvallaridan (MADT, GTDT, SPCR, MCFG, FADT) qurilma tavsifi yasaladi\n");
                acpi = Some(a);
            }
            None => {
                say(st, "Zamin OS: na DTB, na ACPI topildi (EFI configuration table) - yuklanish to'xtatildi\n");
                return EFI_LOAD_ERROR;
            }
        }
    }
    let dtb_size = if dtb != 0 { be32((dtb + 4) as *const u8) as usize } else { 0 };
    if dtb_size > room {
        say(st, "Zamin OS: DTB juda katta\n");
        return EFI_LOAD_ERROR;
    }

    // 2) Xotira xaritasi + ExitBootServices (xarita özgarsa — qayta urinamiz)
    let bs = unsafe { &*st.boot_services };
    let get_map: GetMemoryMap = unsafe { core::mem::transmute(bs.fns[4]) };
    let exit_bs: ExitBootServices = unsafe { core::mem::transmute(bs.fns[26]) };
    say(st, "Zamin OS: ExitBootServices...\n");
    let map = &raw mut MAP as *mut u8;
    let (mut size, mut key, mut dsize, mut dver) = (0usize, 0usize, 0usize, 0u32);
    let mut ok = false;
    for _ in 0..8 {
        size = core::mem::size_of::<[u8; 32 * 1024]>();
        if get_map(&mut size, map, &mut key, &mut dsize, &mut dver) != EFI_SUCCESS {
            continue;
        }
        if exit_bs(image, key) == EFI_SUCCESS {
            ok = true;
            break;
        }
    }
    if !ok {
        say(st, "Zamin OS: ExitBootServices muvaffaqiyatsiz\n");
        return EFI_LOAD_ERROR;
    }
    // Bundan keyin firmware xizmatlari yöq: faqat xotiraga yozamiz.

    let info = unsafe { &mut *(&raw mut __bootinfo) };
    info.magic = 0;
    info.ram_lo = u64::MAX;
    info.ram_hi = 0;
    info.reserved_count = 0;
    info.acpi_used = 0;
    let n = if dsize > 0 { size / dsize } else { 0 };
    info.efi_regions = n as u64;
    for i in 0..n {
        let d = unsafe { map.add(i * dsize) };
        let ty = unsafe { read_unaligned(d as *const u32) };
        let start = unsafe { read_unaligned(d.add(8) as *const u64) };
        let pages = unsafe { read_unaligned(d.add(24) as *const u64) };
        let end = start + pages * 4096;
        if usable(ty) {
            info.ram_lo = info.ram_lo.min(start);
            info.ram_hi = info.ram_hi.max(end);
        } else if reserved_ram(ty) {
            let c = info.reserved_count as usize;
            // Qöşni hududlarni birlaştiramiz; joy tugasa — oxirgisini kengaytiramiz
            if c > 0 && info.reserved[c - 1][1] == start {
                info.reserved[c - 1][1] = end;
            } else if c < MAX_RESERVED {
                info.reserved[c] = [start, end];
                info.reserved_count += 1;
            } else {
                info.reserved[c - 1][1] = info.reserved[c - 1][1].max(end);
            }
        }
    }
    info.magic = BOOTINFO_MAGIC;

    // DTB ni yadro içiga (manba firmware xotirasida edi — u endi böş hisoblanadi)
    if let Some(a) = acpi {
        info.acpi = a;
        info.acpi_used = 1;
        let buf = unsafe { core::slice::from_raw_parts_mut(dst, room) };
        build_dtb(buf, &a, info.ram_lo, info.ram_hi);
    } else {
        unsafe { copy(dtb as *const u8, dst, dtb_size) };
    }

    // 3) MMU/keşni öçirib `_start` ga
    let ks = &raw const __kernel_start as usize;
    let ke = &raw const __kernel_end as usize;
    unsafe { efi_handoff(dst as usize, ks, ke) }
}

/// EFI orqali yuklanganmi — bootinfo ni tekşiradi.
pub fn boot_info() -> Option<&'static BootInfo> {
    let info = unsafe { &*(&raw const __bootinfo) };
    (info.magic == BOOTINFO_MAGIC).then_some(info)
}

/// Kiçik FDT yozuvçisi (heap yöq, berilgan buferga).
struct FdtWriter<'a> {
    buf: &'a mut [u8],
    /// struktura bloki (bufer boşidan 0x40 dan)
    sp: usize,
    strings: [u8; 512],
    slen: usize,
}

impl FdtWriter<'_> {
    const STRUCT: usize = 0x40;

    fn u32(&mut self, v: u32) {
        self.buf[self.sp..self.sp + 4].copy_from_slice(&v.to_be_bytes());
        self.sp += 4;
    }
    fn bytes(&mut self, b: &[u8]) {
        self.buf[self.sp..self.sp + b.len()].copy_from_slice(b);
        self.sp += b.len();
        while self.sp % 4 != 0 {
            self.buf[self.sp] = 0;
            self.sp += 1;
        }
    }
    fn begin(&mut self, name: &[u8]) {
        self.u32(1);
        let mut tmp = [0u8; 48];
        tmp[..name.len()].copy_from_slice(name);
        self.bytes(&tmp[..name.len() + 1]);
    }
    fn begin_at(&mut self, name: &[u8], addr: u64) {
        let mut tmp = [0u8; 40];
        tmp[..name.len()].copy_from_slice(name);
        let mut n = name.len();
        tmp[n] = b'@';
        n += 1;
        let mut started = false;
        for sh in (0..16).rev() {
            let d = ((addr >> (sh * 4)) & 0xf) as usize;
            if d != 0 || started || sh == 0 {
                tmp[n] = b"0123456789abcdef"[d];
                n += 1;
                started = true;
            }
        }
        self.begin(&tmp[..n]);
    }
    fn end(&mut self) {
        self.u32(2);
    }
    fn name_off(&mut self, name: &str) -> u32 {
        let nb = name.as_bytes();
        // mavjud bölsa — qayta işlatamiz
        let mut i = 0;
        while i < self.slen {
            let e = self.strings[i..self.slen].iter().position(|&b| b == 0).unwrap_or(0) + i;
            if &self.strings[i..e] == nb {
                return i as u32;
            }
            i = e + 1;
        }
        let off = self.slen;
        self.strings[off..off + nb.len()].copy_from_slice(nb);
        self.strings[off + nb.len()] = 0;
        self.slen += nb.len() + 1;
        off as u32
    }
    fn prop(&mut self, name: &str, val: &[u8]) {
        let off = self.name_off(name);
        self.u32(3);
        self.u32(val.len() as u32);
        self.u32(off);
        self.bytes(val);
    }
    fn prop_u32s(&mut self, name: &str, vals: &[u32]) {
        let mut tmp = [0u8; 64];
        for (i, v) in vals.iter().enumerate() {
            tmp[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
        }
        self.prop(name, &tmp[..vals.len() * 4]);
    }
    fn prop_regs(&mut self, name: &str, regs: &[(u64, u64)]) {
        let mut v = [0u32; 16];
        for (i, &(a, s)) in regs.iter().enumerate() {
            v[i * 4..i * 4 + 4].copy_from_slice(&[(a >> 32) as u32, a as u32, (s >> 32) as u32, s as u32]);
        }
        self.prop_u32s(name, &v[..regs.len() * 4]);
    }
    fn finish(mut self) {
        self.u32(9); // FDT_END
        let struct_size = self.sp - Self::STRUCT;
        let str_off = self.sp;
        let total = str_off + self.slen;
        self.buf[str_off..total].copy_from_slice(&self.strings[..self.slen]);
        let hdr: [u32; 10] = [
            0xd00d_feed,
            total as u32,
            Self::STRUCT as u32,
            str_off as u32,
            0x28,  // mem_rsvmap (böş: 16 nol bayt 0x28..0x38)
            17,
            16,
            0,
            self.slen as u32,
            struct_size as u32,
        ];
        for (i, h) in hdr.iter().enumerate() {
            self.buf[i * 4..i * 4 + 4].copy_from_slice(&h.to_be_bytes());
        }
        self.buf[0x28..Self::STRUCT].fill(0);
    }
}

/// ACPI maʼlumotlaridan minimal DTB: memory, chosen, cpus, timer, psci, pl011, GIC, PCIe ECAM.
fn build_dtb(buf: &mut [u8], a: &AcpiInfo, ram_lo: u64, ram_hi: u64) {
    let mut w = FdtWriter { buf, sp: FdtWriter::STRUCT, strings: [0; 512], slen: 0 };
    w.begin(b"");
    w.prop_u32s("#address-cells", &[2]);
    w.prop_u32s("#size-cells", &[2]);
    w.prop("compatible", b"linux,dummy-virt\0");
    w.prop("model", b"Zamin OS: ACPI dan yasalgan\0");

    w.begin(b"chosen");
    w.prop("bootargs", b"\0");
    w.end();

    w.begin_at(b"memory", ram_lo);
    w.prop("device_type", b"memory\0");
    w.prop_regs("reg", &[(ram_lo, ram_hi - ram_lo)]);
    w.end();

    // /cpus — MADT GICC yozuvlaridagi MPIDR lar (PSCI CPU_ON şu manzillar bilan)
    w.begin(b"cpus");
    w.prop_u32s("#address-cells", &[2]);
    w.prop_u32s("#size-cells", &[0]);
    for &m in &a.mpidr[..(a.ncpu as usize).min(acpi::MAX_CPUS)] {
        w.begin_at(b"cpu", m);
        w.prop("device_type", b"cpu\0");
        w.prop("compatible", b"arm,armv8\0");
        w.prop("enable-method", b"psci\0");
        w.prop_u32s("reg", &[(m >> 32) as u32, m as u32]);
        w.end();
    }
    w.end();

    // /timer — GTDT dagi PPI lar (GSIV 16..31 → PPI raqami GSIV-16); GTDT yöq bölsa — standart
    let mut t = [0u32; 12];
    for k in 0..4 {
        let (g, f) = (a.timer_gsiv[k], a.timer_flags[k]);
        let ppi = if (16..32).contains(&g) { g - 16 } else { [13, 14, 11, 10][k] };
        // bayroqlar: bit0 — front (1) / daraja (0), bit1 — past faol → DT IRQ_TYPE
        let ty = match (f & 1, f & 2) {
            (1, 0) => 1,
            (1, _) => 2,
            (_, 0) => 4,
            _ => 8,
        };
        t[k * 3..k * 3 + 3].copy_from_slice(&[1, ppi, ty]);
    }
    w.begin(b"timer");
    w.prop("compatible", b"arm,armv8-timer\0");
    w.prop_u32s("interrupts", &t);
    w.end();

    w.begin(b"psci");
    w.prop("compatible", b"arm,psci-1.0\0arm,psci-0.2\0");
    w.prop("method", if a.psci_hvc != 0 { b"hvc\0" } else { b"smc\0" });
    w.end();

    if a.uart != 0 && matches!(a.uart_type, 3 | 0x0d | 0x0e) {
        w.begin_at(b"pl011", a.uart);
        w.prop("compatible", b"arm,pl011\0arm,primecell\0");
        w.prop_regs("reg", &[(a.uart, 0x1000)]);
        if a.uart_gsiv >= 32 {
            w.prop_u32s("interrupts", &[0, a.uart_gsiv - 32, 4]);
        }
        w.end();
    }

    w.begin_at(b"intc", a.gicd);
    if a.gic_version >= 3 {
        w.prop("compatible", b"arm,gic-v3\0");
        w.prop_regs("reg", &[(a.gicd, 0x10000), (a.gicr_or_gicc, a.gicr_len)]);
    } else {
        w.prop("compatible", b"arm,cortex-a15-gic\0");
        w.prop_regs("reg", &[(a.gicd, 0x10000), (a.gicr_or_gicc, 0x2000)]);
    }
    w.end();

    if a.ecam != 0 {
        w.begin_at(b"pcie", a.ecam);
        w.prop("compatible", b"pci-host-ecam-generic\0");
        w.prop("device_type", b"pci\0");
        // MCFG manzili 0-şinaga mos keladi; reg — boşlanğiç şinadan
        let (b0, b1) = (a.ecam_start_bus as u64, (a.ecam_end_bus as u64).max(a.ecam_start_bus as u64));
        w.prop_regs("reg", &[(a.ecam + (b0 << 20), (b1 - b0 + 1) << 20)]);
        w.prop_u32s("bus-range", &[b0 as u32, b1 as u32]);
        w.end();
    }

    w.end();
    w.finish();
}
