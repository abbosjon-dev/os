//! Oqimlar: bitta jarayon içida parallel işlaydigan kod (turli yadrolarda).
//!
//! ```
//! let h = zamin::thread::spawn(|| hisobla(1, 1000));
//! let natija = h.join();
//! ```

use crate::abi::{SYS_THREAD_CREATE, SYS_THREAD_EXIT, SYS_THREAD_JOIN, SYS_TID};
use crate::{Box, Vec, syscall};
use core::sync::atomic::{AtomicI64, Ordering};

const STACK_SIZE: usize = 64 * 1024;

struct Start {
    f: Box<dyn FnOnce() -> i64 + Send>,
}

extern "C" fn thread_entry(arg: usize) -> ! {
    let start = unsafe { Box::from_raw(arg as *mut Start) };
    let code = (start.f)();
    unsafe { syscall(SYS_THREAD_EXIT, code as u64, 0, 0, 0) };
    loop {}
}

pub struct JoinHandle {
    tid: u64,
    /// Oqim steki: oqim tugagunça yaşaşi kerak
    _stack: Vec<u8>,
}

/// Yangi oqim. Natija `join()` orqali olinadi.
pub fn spawn(f: impl FnOnce() -> i64 + Send + 'static) -> JoinHandle {
    let stack = crate::vec![0u8; STACK_SIZE];
    let top = (stack.as_ptr() as usize + STACK_SIZE) & !15;
    let start = Box::into_raw(Box::new(Start { f: Box::new(f) }));
    let tid = unsafe { syscall(SYS_THREAD_CREATE, thread_entry as *const () as u64, top as u64, start as u64, 0) };
    if tid < 0 {
        drop(unsafe { Box::from_raw(start) });
        panic!("oqim yaratib bölmadi: {}", tid);
    }
    JoinHandle { tid: tid as u64, _stack: stack }
}

impl JoinHandle {
    pub fn tid(&self) -> u64 {
        self.tid
    }

    /// Oqim tugaşini kutadi. Natija: uning qaytargan qiymati.
    pub fn join(self) -> i64 {
        let r = unsafe { syscall(SYS_THREAD_JOIN, self.tid, 0, 0, 0) };
        drop(self._stack);
        r
    }
}

/// Joriy oqim raqami.
pub fn current_tid() -> u64 {
    unsafe { syscall(SYS_TID, 0, 0, 0, 0) as u64 }
}

/// Bir neçta oqim natijasini yiğiş uçun oddiy hisoblagiç.
pub struct Counter(AtomicI64);

impl Counter {
    pub const fn new() -> Counter {
        Counter(AtomicI64::new(0))
    }
    pub fn add(&self, v: i64) {
        self.0.fetch_add(v, Ordering::Relaxed);
    }
    pub fn get(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}
