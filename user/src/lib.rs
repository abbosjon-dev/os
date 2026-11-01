//! `zamin` — Zamin OS foydalanuvçi dasturlari uçun kutubxona.
//! Tizim çaqiruvlari, `print!`/`println!` va uç tillilik.

#![no_std]

use core::arch::asm;
use core::fmt::{self, Write};

pub mod abi {
    include!("../../src/abi.rs");
}

#[inline(always)]
unsafe fn syscall(num: u64, a0: u64, a1: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!("svc #0", inlateout("x0") a0 as i64 => ret, in("x1") a1, in("x8") num, options(nostack));
    }
    ret
}

/// Jarayonni tugatadi.
pub fn exit(code: i64) -> ! {
    unsafe { syscall(abi::SYS_EXIT, code as u64, 0) };
    loop {}
}

/// Baytlarni konsolga yozadi. Natija: yozilgan baytlar soni yoki manfiy xato kodi.
pub fn write(bytes: &[u8]) -> i64 {
    unsafe { syscall(abi::SYS_WRITE, bytes.as_ptr() as u64, bytes.len() as u64) }
}

/// Xom manzil bilan yoziş (faqat tekşiruv uçun).
pub fn write_raw(addr: u64, len: u64) -> i64 {
    unsafe { syscall(abi::SYS_WRITE, addr, len) }
}

pub fn yield_now() {
    unsafe { syscall(abi::SYS_YIELD, 0, 0) };
}

pub fn sleep_ms(ms: u64) {
    unsafe { syscall(abi::SYS_SLEEP, ms, 0) };
}

pub fn uptime_ms() -> u64 {
    unsafe { syscall(abi::SYS_UPTIME, 0, 0) as u64 }
}

pub fn pid() -> u64 {
    unsafe { syscall(abi::SYS_PID, 0, 0) as u64 }
}

/// Joriy til indeksi: 0 = özbekça, 1 = русский, 2 = English.
pub fn lang() -> usize {
    (unsafe { syscall(abi::SYS_LANG, 0, 0) } as usize).min(2)
}

/// Joriy tildagi matnni tanlaydi: `tr(["Salom", "Привет", "Hello"])`.
pub fn tr(t: [&'static str; 3]) -> &'static str {
    t[lang()]
}

pub struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write(s.as_bytes());
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = Console.write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}

/// Dasturning kiriş nuqtasi: `zamin::entry!(main);` — `fn main() -> i64`.
#[macro_export]
macro_rules! entry {
    ($main:path) => {
        #[unsafe(no_mangle)]
        #[unsafe(link_section = ".text.start")]
        pub extern "C" fn _start() -> ! {
            let code: i64 = $main();
            $crate::exit(code)
        }
    };
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {}", info);
    exit(-101)
}
