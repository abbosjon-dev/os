//! Sinxronizatsiya primitivlari (özimizning spinlock).
//!
//! Qulf uşlab turilganda uzilişlar (IRQ) öçirib qöyiladi. Aks holda:
//! oqim qulfni oladi → taymer uzilişi → boşqa oqimga ötiladi → u ham şu qulfni
//! kutadi → bitta yadroda abadiy kutiş (deadlock). Şu sababli qulf ostida
//! jarayon almaştirilmaydi.
//!
//! Eslatma: AArch64 da atomik buyruqlar (ldxr/stxr) faqat MMU yoqilgandan keyin
//! (oddiy, keşlanadigan xotirada) işonçli işlaydi.

use core::arch::asm;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

/// IRQ larni öçiradi va avvalgi holatni qaytaradi.
pub fn irq_save() -> u64 {
    let daif: u64;
    unsafe {
        asm!("mrs {}, daif", "msr daifset, #2", out(reg) daif, options(nomem, nostack));
    }
    daif
}

/// `irq_save` dan oldingi holatni tiklaydi.
pub fn irq_restore(daif: u64) {
    unsafe { asm!("msr daif, {}", in(reg) daif, options(nomem, nostack)) };
}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self { locked: AtomicBool::new(false), data: UnsafeCell::new(data) }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let daif = irq_save();
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        SpinLockGuard { lock: self, daif }
    }
}

impl<T> SpinLock<T> {
    /// Faqat panic işlovçisi uçun: qulf egasi heç qaçon qaytmaydi.
    pub unsafe fn force_unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    daif: u64,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        irq_restore(self.daif);
    }
}
