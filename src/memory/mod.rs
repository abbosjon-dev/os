//! Xotira boşqaruvi: fizik sahifalar, MMU (virtual xotira) va kernel heap.

pub mod frame;
pub mod heap;
pub mod mmu;

pub const PAGE_SIZE: usize = 4096;

pub const fn align_up(x: usize, a: usize) -> usize {
    (x + a - 1) & !(a - 1)
}

#[allow(dead_code)]
pub const fn align_down(x: usize, a: usize) -> usize {
    x & !(a - 1)
}
