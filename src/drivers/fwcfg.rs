//! QEMU fw_cfg (MMIO) va `ramfb` — faqat QEMU sinovlari uçun.
//!
//! EDK2 ning ramfb drayveri faqat 640x480, 800x600, 1024x768 rejimlarini beradi. GOP zaxira
//! ekranini (src/drivers/fb.rs) boşqa ölçamlarda, qator qadami `stride != width` va RGBX bayt
//! tartibida sinaş uçun QEMU ga `-fw_cfg name=opt/zamin/ramfb,string=1280x800@1344:rgbx`
//! berilsa, yadro ramfb ni öziga ajratilgan yangi buferga qayta sozlaydi (fw_cfg DMA yozuvi,
//! QEMU `etc/ramfb` fayli) va GOP örniga şu framebufferni işlatadi. Fayl bölmasa (masalan,
//! haqiqiy VPS da) — hech narsa özgarmaydi.
//!
//! fw_cfg manzili: DTB dagi `qemu,fw-cfg-mmio` tuguni, yoki ACPI OEM ID "BOCHS " bölsa
//! QEMU `virt` standarti 0x0902_0000. Imzo ("QEMU") va DMA imkoniyati tekşiriladi.

use crate::efi::{GOP_BGRX, GOP_RGBX, GopInfo};
use crate::memory::{PAGE_SIZE, frame, mmu};
use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};

static BASE: AtomicUsize = AtomicUsize::new(0);

/// QEMU virt dagi fw_cfg MMIO manzili (DTB / ACPI yöq bölganda).
pub const VIRT_BASE: usize = 0x0902_0000;

const SEL: usize = 8;
const DMA: usize = 16;
const KEY_SIGNATURE: u16 = 0;
const KEY_ID: u16 = 1;
const KEY_FILE_DIR: u16 = 0x19;

/// fw_cfg manzilini belgilaydi (imzo tekşirilgandan keyin saqlanadi).
pub fn configure(pa: usize) {
    let Some(va) = mmu::ioremap(pa, 0x1000) else { return };
    BASE.store(va, Ordering::Relaxed);
    select(KEY_SIGNATURE);
    let sig = [read8(), read8(), read8(), read8()];
    select(KEY_ID);
    let id = read8();
    // DMA (bit1) şart — ramfb ga faqat DMA orqali yoziladi
    if &sig != b"QEMU" || id & 2 == 0 {
        BASE.store(0, Ordering::Relaxed);
    }
}

pub fn present() -> bool {
    BASE.load(Ordering::Relaxed) != 0
}

fn select(key: u16) {
    let b = BASE.load(Ordering::Relaxed);
    unsafe { ((b + SEL) as *mut u16).write_volatile(key.to_be()) };
}

fn read8() -> u8 {
    let b = BASE.load(Ordering::Relaxed);
    unsafe { (b as *const u8).read_volatile() }
}

fn read_be32() -> u32 {
    u32::from_be_bytes([read8(), read8(), read8(), read8()])
}

/// Fayl katalogidan nom böyiça: (kalit, ölçam).
fn find(name: &str) -> Option<(u16, u32)> {
    if !present() {
        return None;
    }
    select(KEY_FILE_DIR);
    let n = read_be32().min(512);
    for _ in 0..n {
        let size = read_be32();
        let key = u16::from_be_bytes([read8(), read8()]);
        let _ = (read8(), read8());
        let mut nm = [0u8; 56];
        for b in nm.iter_mut() {
            *b = read8();
        }
        let len = nm.iter().position(|&c| c == 0).unwrap_or(56);
        if &nm[..len] == name.as_bytes() {
            return Some((key, size));
        }
    }
    None
}

fn dcache_clean(mut a: usize, end: usize) {
    let ctr: u64;
    unsafe { asm!("mrs {}, ctr_el0", out(reg) ctr) };
    let line = 4usize << ((ctr >> 16) & 0xf);
    a &= !(line - 1);
    while a < end {
        unsafe { asm!("dc civac, {}", in(reg) a) };
        a += line;
    }
    unsafe { asm!("dsb sy") };
}

/// DMA bilan faylga yozadi (`page` — DMA tuzilmasi va maʼlumot uçun bir sahifa, identity xarita).
fn dma_write(key: u16, data: &[u8], page: usize) -> bool {
    if data.len() > PAGE_SIZE - 64 {
        return false;
    }
    let b = BASE.load(Ordering::Relaxed);
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), (page + 64) as *mut u8, data.len());
        let d = page as *mut u32;
        // control: kalit << 16 | SELECT (0x08) | WRITE (0x10)
        d.write_volatile(((key as u32) << 16 | 0x18).to_be());
        d.add(1).write_volatile((data.len() as u32).to_be());
        (page as *mut u64).add(1).write_volatile(((page + 64) as u64).to_be());
    }
    dcache_clean(page, page + 64 + data.len());
    let pa = page as u64;
    unsafe {
        ((b + DMA) as *mut u32).write_volatile(((pa >> 32) as u32).to_be());
        ((b + DMA + 4) as *mut u32).write_volatile((pa as u32).to_be());
    }
    for _ in 0..1_000_000 {
        dcache_clean(page, page + 8);
        let c = u32::from_be(unsafe { (page as *const u32).read_volatile() });
        if c & !1 == 0 {
            return c & 1 == 0; // bit0 — xato
        }
        core::hint::spin_loop();
    }
    false
}

/// "1280x800", "1280x800@1344", "1024x768:rgbx" → (en, böy, qator qadami (piksel), rgbx).
pub fn parse_spec(s: &str) -> Option<(u32, u32, u32, bool)> {
    let s = s.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    let (s, rgbx) = match s.split_once(':') {
        Some((a, f)) if f.eq_ignore_ascii_case("rgbx") => (a, true),
        Some((a, f)) if f.eq_ignore_ascii_case("bgrx") => (a, false),
        Some(_) => return None,
        None => (s, false),
    };
    let (wh, st) = match s.split_once('@') {
        Some((a, b)) => (a, Some(b.parse::<u32>().ok()?)),
        None => (s, None),
    };
    let (w, h) = wh.split_once('x')?;
    let (w, h): (u32, u32) = (w.parse().ok()?, h.parse().ok()?);
    if !(320..=4096).contains(&w) || !(200..=4096).contains(&h) {
        return None;
    }
    let stride = st.unwrap_or(w);
    if stride < w || stride > w + 1024 {
        return None;
    }
    Some((w, h, stride, rgbx))
}

/// `opt/zamin/ramfb` bölsa — ramfb ni yangi ölçam / qadam / formatga sozlab, uning GopInfo sini qaytaradi.
pub fn ramfb_override() -> Option<GopInfo> {
    let (key, size) = find("opt/zamin/ramfb")?;
    let (rkey, _) = find("etc/ramfb")?;
    select(key);
    let mut spec = [0u8; 48];
    let n = (size as usize).min(spec.len());
    for b in spec[..n].iter_mut() {
        *b = read8();
    }
    let (w, h, stride, rgbx) = parse_spec(core::str::from_utf8(&spec[..n]).ok()?)?;
    let bytes = stride as usize * h as usize * 4;
    let fb = frame::alloc_contiguous(bytes.div_ceil(PAGE_SIZE))?;
    unsafe { core::ptr::write_bytes(fb as *mut u8, 0, bytes) };
    dcache_clean(fb, fb + bytes);
    let page = frame::alloc_contiguous(1)?;
    // RAMFBCfg (big-endian, 28 bayt): addr, fourcc, flags, width, height, stride (bayt)
    // DRM fourcc: XR24 — xotirada B,G,R,X; XB24 — R,G,B,X
    let fourcc: u32 = if rgbx { 0x3432_4258 } else { 0x3432_5258 };
    let mut cfg = [0u8; 28];
    cfg[0..8].copy_from_slice(&(fb as u64).to_be_bytes());
    cfg[8..12].copy_from_slice(&fourcc.to_be_bytes());
    cfg[16..20].copy_from_slice(&w.to_be_bytes());
    cfg[20..24].copy_from_slice(&h.to_be_bytes());
    cfg[24..28].copy_from_slice(&(stride * 4).to_be_bytes());
    if !dma_write(rkey, &cfg, page) {
        return None;
    }
    Some(GopInfo { base: fb as u64, size: bytes as u64, width: w, height: h, stride, format: if rgbx { GOP_RGBX } else { GOP_BGRX }, mode: 0, max_mode: 0 })
}

/// O'z-özini tekşiruv: spetsifikatsiya tahlili.
pub fn self_test() -> Result<usize, &'static str> {
    let ok = [
        ("1280x800", (1280, 800, 1280, false)),
        ("1024x768@1088:rgbx", (1024, 768, 1088, true)),
        ("1280x800@1344:BGRX\0", (1280, 800, 1344, false)),
    ];
    for (s, want) in ok {
        if parse_spec(s) != Some(want) {
            return Err("ramfb: töğri spetsifikatsiya rad etildi");
        }
    }
    for s in ["", "1280", "1280x", "x800", "10x10", "1280x800@1000", "1280x800:yuv", "1280x800@99999", "1280x800@"] {
        if parse_spec(s).is_some() {
            return Err("ramfb: notöğri spetsifikatsiya qabul qilindi");
        }
    }
    Ok(ok.len() + 9)
}
