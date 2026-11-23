//! Chiziqli framebuffer (UEFI GOP) drayveri — virtio-gpu bölmagan VPS lar uçun zaxira ekran.
//!
//! Firmware (masalan EDK2 + QEMU `ramfb`, yoki boşqa GOP displey) ExitBootServices dan oldin
//! bergan chiziqli framebuffer ([`crate::efi::GopInfo`]) işlatiladi. Interfeys virtio-gpu
//! bilan bir xil: çiziş ichki buferga (RAM, u32 = 0x00RRGGBB) yoziladi, `flush` uni
//! framebufferga (qator qadami — `stride`, kanal tartibi — RGBX/BGRX böyiça) köçiradi.
//!
//! Xaritalaş: framebuffer RAM içida bölsa (ramfb) — identity xarita (Normal), köçirişdan keyin
//! keş qatorlari tozalanadi (`dc cvac`) — displey uni xotiradan öqiydi. RAM dan taşqarida
//! bölsa (PCI BAR va h.k.) — `mmu::ioremap` orqali Device sifatida.

use crate::efi::{GOP_RGBX, GopInfo};
use crate::memory::{PAGE_SIZE, frame, mmu};
use crate::sync::Mutex;
use core::arch::asm;

struct Fb {
    /// ichki bufer (çiziş şu yerga)
    back: usize,
    /// framebuffer VA si
    vram: usize,
    width: u32,
    height: u32,
    stride: u32,
    rgbx: bool,
    /// framebuffer keşlanadigan RAM da (köçirişdan keyin keşni tozalaş kerak)
    cached: bool,
}

static FB: Mutex<Option<Fb>> = Mutex::new(None);

/// GOP framebufferini tayyorlaydi. Natija: (width, height, ichki bufer manzili).
/// `ram` — yadro xaritalagan oddiy RAM hududi [lo, hi).
pub fn init(g: &GopInfo, ram: (usize, usize)) -> Option<(u32, u32, usize)> {
    let (base, len) = (g.base as usize, g.stride as usize * g.height as usize * 4);
    let cached = base >= ram.0 && base + len <= ram.1;
    let vram = if cached { base } else { mmu::ioremap(base, len)? };
    let pages = (g.width as usize * g.height as usize * 4).div_ceil(PAGE_SIZE);
    let back = frame::alloc_contiguous(pages)?;
    unsafe { core::ptr::write_bytes(back as *mut u8, 0, pages * PAGE_SIZE) };
    *FB.lock() = Some(Fb {
        back,
        vram,
        width: g.width,
        height: g.height,
        stride: g.stride,
        rgbx: g.format == GOP_RGBX,
        cached,
    });
    flush();
    Some((g.width, g.height, back))
}

pub fn present() -> bool {
    FB.lock().is_some()
}

pub fn resolution() -> (u32, u32) {
    FB.lock().as_ref().map_or((0, 0), |f| (f.width, f.height))
}

fn dcache_clean(mut a: usize, end: usize) {
    let ctr: u64;
    unsafe { asm!("mrs {}, ctr_el0", out(reg) ctr) };
    let line = 4usize << ((ctr >> 16) & 0xf);
    a &= !(line - 1);
    while a < end {
        unsafe { asm!("dc cvac, {}", in(reg) a) };
        a += line;
    }
    unsafe { asm!("dsb sy") };
}

/// Töğriburçakni ichki buferdan framebufferga köçiradi.
pub fn flush_rect(x: u32, y: u32, w: u32, h: u32) {
    let guard = FB.lock();
    let Some(f) = guard.as_ref() else { return };
    let (x, y) = (x.min(f.width) as usize, y.min(f.height) as usize);
    let x1 = (x + w as usize).min(f.width as usize);
    let y1 = (y + h as usize).min(f.height as usize);
    if x >= x1 {
        return;
    }
    for row in y..y1 {
        let src = (f.back + (row * f.width as usize + x) * 4) as *const u32;
        let dst = (f.vram + (row * f.stride as usize + x) * 4) as *mut u32;
        for i in 0..x1 - x {
            unsafe {
                let c = src.add(i).read();
                // BGRX: u32 (LE) = 0x00RRGGBB — özgarişsiz; RGBX: R va B almaşadi
                let c = if f.rgbx { (c & 0xff00) | ((c >> 16) & 0xff) | ((c & 0xff) << 16) } else { c };
                dst.add(i).write_volatile(c);
            }
        }
        if f.cached {
            dcache_clean(dst_addr(f, row, x), dst_addr(f, row, x1));
        }
    }
}

fn dst_addr(f: &Fb, row: usize, col: usize) -> usize {
    f.vram + (row * f.stride as usize + col) * 4
}

/// Butun ekranni yangilaydi.
pub fn flush() {
    let (w, h) = resolution();
    flush_rect(0, 0, w, h);
}
