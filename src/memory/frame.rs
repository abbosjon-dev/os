//! Fizik sahifa (frame) allokatori — bitmap asosida.
//! Har bir bit bitta 4 KiB sahifani bildiradi: 0 = bo'sh, 1 = band.
//! Bitmapning o'zi yadrodan keyingi birinchi bo'sh joyga joylashtiriladi.

use super::{PAGE_SIZE, align_up};
use crate::sync::SpinLock;

struct FrameAllocator {
    base: usize,
    pages: usize,
    bitmap: *mut u64,
    words: usize,
    hint: usize,
    free: usize,
}

unsafe impl Send for FrameAllocator {}

static FRAMES: SpinLock<Option<FrameAllocator>> = SpinLock::new(None);

impl FrameAllocator {
    fn is_used(&self, i: usize) -> bool {
        unsafe { *self.bitmap.add(i / 64) & (1 << (i % 64)) != 0 }
    }

    fn set(&mut self, i: usize, used: bool) {
        let w = unsafe { &mut *self.bitmap.add(i / 64) };
        if used {
            *w |= 1 << (i % 64);
        } else {
            *w &= !(1 << (i % 64));
        }
    }

    fn alloc(&mut self) -> Option<usize> {
        for n in 0..self.words {
            let wi = (self.hint + n) % self.words;
            let w = unsafe { *self.bitmap.add(wi) };
            if w != u64::MAX {
                let i = wi * 64 + w.trailing_ones() as usize;
                if i >= self.pages {
                    continue;
                }
                self.set(i, true);
                self.free -= 1;
                self.hint = wi;
                return Some(self.base + i * PAGE_SIZE);
            }
        }
        None
    }

    fn alloc_contiguous(&mut self, count: usize) -> Option<usize> {
        let mut run = 0;
        for i in 0..self.pages {
            if self.is_used(i) {
                run = 0;
                continue;
            }
            run += 1;
            if run == count {
                let first = i + 1 - count;
                for j in first..=i {
                    self.set(j, true);
                }
                self.free -= count;
                return Some(self.base + first * PAGE_SIZE);
            }
        }
        None
    }

    fn free(&mut self, pa: usize) {
        assert!(pa >= self.base && pa % PAGE_SIZE == 0, "noto'g'ri frame: {:#x}", pa);
        let i = (pa - self.base) / PAGE_SIZE;
        assert!(i < self.pages && self.is_used(i), "ikki marta bo'shatish: {:#x}", pa);
        self.set(i, false);
        self.free += 1;
    }
}

/// `reserved_end` gacha bo'lgan hamma narsa (DTB, yadro) band deb belgilanadi.
pub fn init(ram_start: usize, ram_size: usize, reserved_end: usize) {
    let pages = ram_size / PAGE_SIZE;
    let words = pages.div_ceil(64);
    let bitmap = align_up(reserved_end, PAGE_SIZE) as *mut u64;
    unsafe { core::ptr::write_bytes(bitmap, 0, words) };

    let mut fa = FrameAllocator { base: ram_start, pages, bitmap, words, hint: 0, free: pages };
    let used_end = align_up(bitmap as usize + words * 8, PAGE_SIZE);
    for i in 0..(used_end - ram_start) / PAGE_SIZE {
        fa.set(i, true);
        fa.free -= 1;
    }
    // Oxirgi so'zdagi mavjud bo'lmagan sahifalar ham "band".
    for i in pages..words * 64 {
        fa.set(i, true);
    }
    *FRAMES.lock() = Some(fa);
}

fn with<R>(f: impl FnOnce(&mut FrameAllocator) -> R) -> R {
    f(FRAMES.lock().as_mut().expect("frame allokatori ishga tushirilmagan"))
}

/// Bitta nollangan 4 KiB sahifa.
pub fn alloc_zeroed() -> Option<usize> {
    let pa = with(|fa| fa.alloc())?;
    unsafe { core::ptr::write_bytes(pa as *mut u8, 0, PAGE_SIZE) };
    Some(pa)
}

pub fn alloc_contiguous(count: usize) -> Option<usize> {
    with(|fa| fa.alloc_contiguous(count))
}

pub fn free(pa: usize) {
    with(|fa| fa.free(pa))
}

/// (bo'sh sahifalar, jami sahifalar)
pub fn stats() -> (usize, usize) {
    with(|fa| (fa.free, fa.pages))
}
