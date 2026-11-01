//! `zamin` — Zamin OS foydalanuvçi dasturlari uçun kutubxona.
//!
//! - tizim çaqiruvlari (`exit`, `sleep_ms`, `spawn`, `wait` ...)
//! - `print!` / `println!` va uç tillilik (`tr`)
//! - xotira ajratuvçi: `Vec`, `String`, `Box`, `format!` işlaydi
//! - fayllar (`fs`), Zamin kanallari (`Channel`)
//! - dastur argumentlari (`args`)

#![no_std]

extern crate alloc;

pub mod fs;
mod heap;

use core::arch::asm;
use core::fmt::{self, Write};

pub mod abi {
    include!("../../src/abi.rs");
}

pub use alloc::{boxed::Box, format, string::String, string::ToString, vec, vec::Vec};

/// Tizim xatosi (manfiy kod, `abi::ERR_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error(pub i64);

impl Error {
    /// Xatoning joriy tildagi tavsifi.
    pub fn text(self) -> &'static str {
        use abi::*;
        tr(match self.0 {
            ERR_BAD_ADDRESS => ["notöğri manzil", "неверный адрес", "bad address"],
            ERR_NOT_FOUND => ["topilmadi", "не найдено", "not found"],
            ERR_EXISTS => ["allaqaçon mavjud", "уже существует", "already exists"],
            ERR_NO_MEMORY => ["xotira yetmadi", "недостаточно памяти", "out of memory"],
            ERR_BAD_HANDLE => ["notöğri dastak", "неверный дескриптор", "bad handle"],
            ERR_FULL => ["töla", "переполнено", "full"],
            ERR_TOO_BIG => ["juda katta", "слишком большой", "too big"],
            ERR_NOT_DIR => ["papka emas", "не папка", "not a directory"],
            ERR_IS_DIR => ["bu papka", "это папка", "is a directory"],
            ERR_NOT_EMPTY => ["papka böş emas", "папка не пуста", "directory not empty"],
            ERR_DENIED => ["ruxsat yöq", "доступ запрещён", "permission denied"],
            ERR_NOT_CHILD => ["bola jarayon emas", "не дочерний процесс", "not a child process"],
            ERR_IO => ["disk xatosi", "ошибка диска", "disk error"],
            ERR_CORRUPT => [
                "diskdagi maʼlumot buzilgan (tekşiruvdan ötmadi)",
                "данные на диске повреждены (не прошли проверку)",
                "data on disk is corrupted (failed verification)",
            ],
            _ => ["nomaʼlum xato", "неизвестная ошибка", "unknown error"],
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ({})", self.text(), self.0)
    }
}

pub type Result<T> = core::result::Result<T, Error>;

#[inline(always)]
pub(crate) unsafe fn syscall(num: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            inlateout("x0") a0 as i64 => ret,
            in("x1") a1, in("x2") a2, in("x3") a3, in("x8") num,
            options(nostack),
        );
    }
    ret
}

pub(crate) fn check(r: i64) -> Result<u64> {
    if r < 0 { Err(Error(r)) } else { Ok(r as u64) }
}

/// Jarayonni tugatadi.
pub fn exit(code: i64) -> ! {
    unsafe { syscall(abi::SYS_EXIT, code as u64, 0, 0, 0) };
    loop {}
}

/// Baytlarni konsolga yozadi. Natija: yozilgan baytlar soni yoki manfiy xato kodi.
pub fn write(bytes: &[u8]) -> i64 {
    unsafe { syscall(abi::SYS_WRITE, bytes.as_ptr() as u64, bytes.len() as u64, 0, 0) }
}

/// Xom manzil bilan yoziş (faqat tekşiruv uçun).
pub fn write_raw(addr: u64, len: u64) -> i64 {
    unsafe { syscall(abi::SYS_WRITE, addr, len, 0, 0) }
}

pub fn yield_now() {
    unsafe { syscall(abi::SYS_YIELD, 0, 0, 0, 0) };
}

pub fn sleep_ms(ms: u64) {
    unsafe { syscall(abi::SYS_SLEEP, ms, 0, 0, 0) };
}

pub fn uptime_ms() -> u64 {
    unsafe { syscall(abi::SYS_UPTIME, 0, 0, 0, 0) as u64 }
}

pub fn pid() -> u64 {
    unsafe { syscall(abi::SYS_PID, 0, 0, 0, 0) as u64 }
}

/// Joriy til indeksi: 0 = özbekça, 1 = русский, 2 = English.
pub fn lang() -> usize {
    (unsafe { syscall(abi::SYS_LANG, 0, 0, 0, 0) } as usize).min(2)
}

/// Joriy tildagi matnni tanlaydi: `tr(["Salom", "Привет", "Hello"])`.
pub fn tr(t: [&'static str; 3]) -> &'static str {
    t[lang()]
}

/// Dasturni işga tuşiradi: `spawn("/dasturlar/salom", "")`. Natija: PID.
pub fn spawn(path: &str, args: &str) -> Result<u64> {
    check(unsafe {
        syscall(abi::SYS_SPAWN, path.as_ptr() as u64, path.len() as u64, args.as_ptr() as u64, args.len() as u64)
    })
}

/// Bola jarayon tugaşini kutadi. Natija: uning çiqiş kodi.
pub fn wait(pid: u64) -> Result<i64> {
    let r = unsafe { syscall(abi::SYS_WAIT, pid, 0, 0, 0) };
    // Çiqiş kodi manfiy bölişi ham mumkin, şuning uçun faqat maʼlum xatolarni ajratamiz.
    if r == abi::ERR_NOT_CHILD || r == abi::ERR_NOT_FOUND { Err(Error(r)) } else { Ok(r) }
}

// --- Argumentlar ---

static mut ARGS: &str = "";

/// Dasturga berilgan argumentlar qatori (masalan, `işlat hisobla 1000` → "1000").
pub fn args() -> &'static str {
    unsafe { ARGS }
}

#[doc(hidden)]
pub unsafe fn _init(args_ptr: *const u8, args_len: usize) {
    unsafe {
        let bytes = core::slice::from_raw_parts(args_ptr, args_len);
        ARGS = core::str::from_utf8(bytes).unwrap_or("");
    }
}

// --- Zamin kanallari ---

/// Nomli xabar kanali. Bir neçta jarayon bitta nom orqali ulanadi.
pub struct Channel(u64);

impl Channel {
    pub fn open(name: &str) -> Result<Channel> {
        check(unsafe { syscall(abi::SYS_CHAN_OPEN, name.as_ptr() as u64, name.len() as u64, 0, 0) }).map(Channel)
    }

    pub fn send(&self, data: &[u8]) -> Result<()> {
        check(unsafe { syscall(abi::SYS_CHAN_SEND, self.0, data.as_ptr() as u64, data.len() as u64, 0) }).map(|_| ())
    }

    /// Xabar kelgunça kutadi. Natija: xabar uzunligi.
    pub fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        check(unsafe { syscall(abi::SYS_CHAN_RECV, self.0, buf.as_mut_ptr() as u64, buf.len() as u64, 0) })
            .map(|n| n as usize)
    }

    /// Xabarni matn sifatida oladi.
    pub fn recv_string(&self) -> Result<String> {
        let mut buf = vec![0u8; abi::MAX_MESSAGE];
        let n = self.recv(&mut buf)?;
        buf.truncate(n);
        Ok(String::from_utf8(buf).unwrap_or_default())
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        unsafe { syscall(abi::SYS_CLOSE, self.0, 0, 0, 0) };
    }
}

// --- Konsol ---

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
        pub extern "C" fn _start(args_ptr: *const u8, args_len: usize) -> ! {
            unsafe { $crate::_init(args_ptr, args_len) };
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
