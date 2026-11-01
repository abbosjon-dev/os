//! `zamin` — Zamin OS foydalanuvçi dasturlari uçun kutubxona.
//!
//! - tizim çaqiruvlari (`exit`, `sleep_ms`, `spawn`, `wait` ...)
//! - `print!` / `println!` va uç tillilik (`tr`)
//! - xotira ajratuvçi: `Vec`, `String`, `Box`, `format!` işlaydi
//! - fayllar (`fs`), Zamin kanallari (`Channel`)
//! - dastur argumentlari (`args`)
//! - oqimlar (`thread::spawn`) va futex asosidagi `sync::Mutex`

#![no_std]

extern crate alloc;

pub mod fs;
pub mod net;
mod heap;
pub mod sync;
pub mod thread;
pub mod time;

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
            ERR_BROKEN_PIPE => ["quvur uzildi", "канал разорван", "broken pipe"],
            ERR_INTERRUPTED => ["töxtatildi", "прервано", "interrupted"],
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
    unsafe { syscall5(num, a0, a1, a2, a3, 0) }
}

#[inline(always)]
pub(crate) unsafe fn syscall5(num: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
    unsafe { syscall6(num, a0, a1, a2, a3, a4, abi::INHERIT) }
}

#[inline(always)]
pub(crate) unsafe fn syscall6(num: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            inlateout("x0") a0 as i64 => ret,
            in("x1") a1, in("x2") a2, in("x3") a3, in("x4") a4, in("x5") a5, in("x8") num,
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
/// Standart dastaklar otadan meros bölib ötadi.
pub fn spawn(path: &str, args: &str) -> Result<u64> {
    spawn_with(path, args, [abi::INHERIT; 3])
}

/// Dasturni berilgan stdin/stdout/stderr dastaklari bilan işga tuşiradi
/// (`abi::INHERIT` — otaning şu raqamli dastagi). Quvur va yönaltiriş uçun.
pub fn spawn_with(path: &str, args: &str, stdio: [u64; 3]) -> Result<u64> {
    let packed = (stdio[0] & 0xffff) | (stdio[1] & 0xffff) << 16 | (stdio[2] & 0xffff) << 32;
    check(unsafe {
        syscall5(
            abi::SYS_SPAWN,
            path.as_ptr() as u64,
            path.len() as u64,
            args.as_ptr() as u64,
            args.len() as u64,
            packed,
        )
    })
}

/// Qanday işga tuşiriladi: qöşimça dastak (bolada 3-raqam) va qafas.
pub struct SpawnExtra {
    pub handle: Option<u64>,
    /// Qafas: bola faqat joriy papka içida işlaydi, dastur işga tuşira olmaydi
    pub sandbox: bool,
}

/// Töliq variant: stdin/stdout/stderr, qöşimça dastak va qafas.
pub fn spawn_full(path: &str, args: &str, stdio: [u64; 3], extra: SpawnExtra) -> Result<u64> {
    let packed = (stdio[0] & 0xffff) | (stdio[1] & 0xffff) << 16 | (stdio[2] & 0xffff) << 32;
    let x5 = extra.handle.unwrap_or(abi::INHERIT) & 0xffff | if extra.sandbox { abi::SPAWN_SANDBOX } else { 0 };
    check(unsafe {
        syscall6(abi::SYS_SPAWN, path.as_ptr() as u64, path.len() as u64, args.as_ptr() as u64, args.len() as u64, packed, x5)
    })
}

/// Fayl ruxsatlarini özgartiradi: `chmod("skript", 0o755)`.
pub fn chmod(path: &str, mode: u16) -> Result<()> {
    check(unsafe { syscall(abi::SYS_CHMOD, path.as_ptr() as u64, path.len() as u64, mode as u64, 0) }).map(|_| ())
}

/// Dastakni yopadi.
pub fn close(handle: u64) {
    unsafe { syscall(abi::SYS_CLOSE, handle, 0, 0, 0) };
}

/// Quvur: (öqiş dastagi, yoziş dastagi).
pub fn pipe() -> Result<(u64, u64)> {
    let mut out = [0u64; 2];
    check(unsafe { syscall(abi::SYS_PIPE, out.as_mut_ptr() as u64, 0, 0, 0) })?;
    Ok((out[0], out[1]))
}

/// Dastakdan öqiydi (stdin = 0). Natija: öqilgan baytlar (0 — oxiri).
pub fn read(handle: u64, buf: &mut [u8]) -> Result<usize> {
    check(unsafe { syscall(abi::SYS_READ, handle, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }).map(|n| n as usize)
}

/// Dastakka yozadi.
pub fn write_to(handle: u64, data: &[u8]) -> Result<usize> {
    check(unsafe { syscall(abi::SYS_FWRITE, handle, data.as_ptr() as u64, data.len() as u64, 0) }).map(|n| n as usize)
}

/// Standart kiriş buferi (konsol bir urinişda butun qatorni beradi).
static mut STDIN_BUF: Vec<u8> = Vec::new();

fn stdin_buf() -> &'static mut Vec<u8> {
    unsafe { &mut *(&raw mut STDIN_BUF) }
}

/// Standart kirişdan bitta qator (`None` — kiriş tugadi).
/// Konsoldan: foydalanuvçi Enter bosgunça; quvurdan: navbatdagi qator.
pub fn read_line() -> Option<String> {
    let buf = stdin_buf();
    loop {
        if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            return Some(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned());
        }
        let mut chunk = vec![0u8; 4096];
        match read(abi::STDIN, &mut chunk) {
            Ok(n) if n > 0 => buf.extend_from_slice(&chunk[..n]),
            _ => {
                if buf.is_empty() {
                    return None;
                }
                let rest = core::mem::take(buf);
                return Some(String::from_utf8_lossy(&rest).into_owned());
            }
        }
    }
}

/// Standart kirişni oxirigaça öqiydi.
pub fn read_all_stdin() -> Vec<u8> {
    let mut out = core::mem::take(stdin_buf());
    let mut buf = vec![0u8; 4096];
    while let Ok(n) = read(abi::STDIN, &mut buf) {
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    out
}

/// Bola jarayon tugaganmi (kutmaydi): `Ok(Some(kod))` — tugagan, `Ok(None)` — hali işlayapti.
pub fn try_wait(pid: u64) -> Result<Option<i64>> {
    let r = unsafe { syscall(abi::SYS_WAIT, pid, 1, 0, 0) };
    match r {
        abi::ERR_RUNNING => Ok(None),
        abi::ERR_NOT_CHILD | abi::ERR_NOT_FOUND => Err(Error(r)),
        code => Ok(Some(code)),
    }
}

/// Muhit özgaruvçisi.
pub fn env(name: &str) -> Option<String> {
    let mut buf = vec![0u8; 1024];
    let n = check(unsafe {
        syscall(abi::SYS_GETENV, name.as_ptr() as u64, name.len() as u64, buf.as_mut_ptr() as u64, buf.len() as u64)
    })
    .ok()? as usize;
    buf.truncate(n);
    String::from_utf8(buf).ok()
}

pub fn set_env(name: &str, value: &str) -> Result<()> {
    check(unsafe {
        syscall(abi::SYS_SETENV, name.as_ptr() as u64, name.len() as u64, value.as_ptr() as u64, value.len() as u64)
    })
    .map(|_| ())
}

/// Joriy papkani özgartiradi (nisbiy yöllar şunga nisbatan oçiladi).
pub fn chdir(path: &str) -> Result<()> {
    check(unsafe { syscall(abi::SYS_CHDIR, path.as_ptr() as u64, path.len() as u64, 0, 0) }).map(|_| ())
}

/// Signal yuboradi: `signal(pid, abi::SIG_USER)`; 9/15 — jarayonni töxtatadi.
pub fn signal(pid: u64, sig: u64) -> Result<()> {
    check(unsafe { syscall(abi::SYS_SIGNAL, pid, sig, 0, 0) }).map(|_| ())
}

/// Kelgan signallar bitmaskasi (öqilgaç tozalanadi). `pending() & (1 << SIG_USER)`.
pub fn signals_pending() -> u32 {
    unsafe { syscall(abi::SYS_SIGPENDING, 0, 0, 0, 0) as u32 }
}

/// Konsolning oldingi plandagi jarayoni (Ctrl+C uni töxtatadi). Faqat terminal uçun.
pub fn set_foreground(pid: u64) {
    unsafe { syscall(abi::SYS_FOREGROUND, pid, 0, 0, 0) };
}

/// Tizim buyruği (faqat işonçli terminal uçun).
pub fn admin(line: &str) -> Result<()> {
    check(unsafe { syscall(abi::SYS_ADMIN, line.as_ptr() as u64, line.len() as u64, 0, 0) }).map(|_| ())
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
    /// Nomsiz kanal: uni faqat dastak orqali (masalan, bolaga) beriş mumkin.
    pub fn create() -> Result<Channel> {
        check(unsafe { syscall(abi::SYS_CHAN_CREATE, 0, 0, 0, 0) }).map(Channel)
    }

    /// Dastak raqami (bolaga beriş uçun).
    pub fn handle(&self) -> u64 {
        self.0
    }

    /// Ota bergan kanal (bolada 3-dastak).
    pub fn inherited() -> Channel {
        Channel(3)
    }

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

/// Qator avval buferga yiğiladi va bitta tizim çaqiruvida yoziladi — boşqa
/// jarayonlarning çiqişi bilan harflar aralaşib ketmaydi.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let mut s = String::new();
    if fmt::write(&mut s, args).is_ok() {
        write(s.as_bytes());
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::_print(format_args!($($arg)*)) };
}

/// Xato xabarlari — standart xato oqimiga (quvurga emas).
pub struct ErrConsole;

impl Write for ErrConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let _ = write_to(abi::STDERR, s.as_bytes());
        Ok(())
    }
}

#[doc(hidden)]
pub fn _eprint(args: fmt::Arguments) {
    let mut s = String::new();
    if fmt::write(&mut s, args).is_ok() {
        let _ = write_to(abi::STDERR, s.as_bytes());
    }
}

#[macro_export]
macro_rules! eprintln {
    ($($arg:tt)*) => { $crate::_eprint(format_args!("{}\n", format_args!($($arg)*))) };
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
