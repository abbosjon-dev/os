//! Oqimlar uçun qulf (futex asosida): band bölsa, oqim uxlaydi (protsessorni band qilmaydi).

use crate::abi::{SYS_FUTEX_WAIT, SYS_FUTEX_WAKE};
use crate::syscall;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

pub struct Mutex<T> {
    /// 0 — böş, 1 — band, 2 — band va kutayotganlar bor
    state: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(v: T) -> Self {
        Self { state: AtomicU32::new(0), data: UnsafeCell::new(v) }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        if self.state.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
            // Band: "kutayotganlar bor" deb belgilab, uxlaymiz
            while self.state.swap(2, Ordering::Acquire) != 0 {
                unsafe { syscall(SYS_FUTEX_WAIT, self.state.as_ptr() as u64, 2, 0, 0) };
            }
        }
        MutexGuard { m: self }
    }
}

pub struct MutexGuard<'a, T> {
    m: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.m.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.m.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        if self.m.state.swap(0, Ordering::Release) == 2 {
            unsafe { syscall(SYS_FUTEX_WAKE, self.m.state.as_ptr() as u64, 0, 0, 0) };
        }
    }
}
