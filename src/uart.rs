//! PL011 UART drayveri (QEMU `virt` mashinasida 0x0900_0000 manzilda).
//! Hech qanday tashqi crate yo'q — to'g'ridan-to'g'ri registrlar bilan ishlaymiz.

use core::fmt;
use core::ptr::{read_volatile, write_volatile};

const UART0_BASE: usize = 0x0900_0000;

// Registr offsetlari
const DR: usize = 0x00; // Data
const FR: usize = 0x18; // Flags
const IBRD: usize = 0x24;
const FBRD: usize = 0x28;
const LCRH: usize = 0x2c;
const CR: usize = 0x30;
const IMSC: usize = 0x38;
const ICR: usize = 0x44;

/// PL011 uzilishi: SPI 1 -> INTID 33.
pub const IRQ: u32 = 33;
const INT_RX: u32 = 1 << 4;
const INT_RT: u32 = 1 << 6; // qabul qilish taym-auti

const FR_TXFF: u32 = 1 << 5; // TX FIFO to'la
const FR_RXFE: u32 = 1 << 4; // RX FIFO bo'sh

pub struct Uart {
    base: usize,
}

impl Uart {
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    fn reg(&self, off: usize) -> *mut u32 {
        (self.base + off) as *mut u32
    }

    pub fn init(&self) {
        unsafe {
            write_volatile(self.reg(CR), 0); // o'chiramiz
            write_volatile(self.reg(ICR), 0x7ff); // uzilishlarni tozalaymiz
            // 24 MHz soat, 115200 baud: 24e6 / (16 * 115200) = 13.02
            write_volatile(self.reg(IBRD), 13);
            write_volatile(self.reg(FBRD), 1);
            write_volatile(self.reg(LCRH), (0b11 << 5) | (1 << 4)); // 8N1, FIFO
            write_volatile(self.reg(CR), (1 << 0) | (1 << 8) | (1 << 9)); // UART, TX, RX
        }
    }

    pub fn put_byte(&self, b: u8) {
        unsafe {
            while read_volatile(self.reg(FR)) & FR_TXFF != 0 {
                core::hint::spin_loop();
            }
            write_volatile(self.reg(DR), b as u32);
        }
    }

    pub fn try_get_byte(&self) -> Option<u8> {
        unsafe {
            if read_volatile(self.reg(FR)) & FR_RXFE != 0 {
                None
            } else {
                Some(read_volatile(self.reg(DR)) as u8)
            }
        }
    }
}

impl fmt::Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.put_byte(b'\r');
            }
            self.put_byte(b);
        }
        Ok(())
    }
}

/// Kiruvchi baytlar uchun halqa bufer: uzilish yozadi, asosiy tsikl o'qiydi
/// (bitta yozuvchi, bitta o'quvchi — qulfsiz).
mod rx {
    use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    const SIZE: usize = 256;
    static BUF: [AtomicU8; SIZE] = [const { AtomicU8::new(0) }; SIZE];
    static HEAD: AtomicUsize = AtomicUsize::new(0); // yozish
    static TAIL: AtomicUsize = AtomicUsize::new(0); // o'qish

    pub fn push(b: u8) {
        let h = HEAD.load(Ordering::Relaxed);
        if h.wrapping_sub(TAIL.load(Ordering::Acquire)) < SIZE {
            BUF[h % SIZE].store(b, Ordering::Relaxed);
            HEAD.store(h.wrapping_add(1), Ordering::Release);
        } // to'la bo'lsa — bayt tashlab yuboriladi
    }

    pub fn pop() -> Option<u8> {
        let t = TAIL.load(Ordering::Relaxed);
        if t == HEAD.load(Ordering::Acquire) {
            return None;
        }
        let b = BUF[t % SIZE].load(Ordering::Relaxed);
        TAIL.store(t.wrapping_add(1), Ordering::Release);
        Some(b)
    }

    pub fn is_empty() -> bool {
        TAIL.load(Ordering::Relaxed) == HEAD.load(Ordering::Acquire)
    }
}

pub fn enable_rx_irq() {
    let u = console();
    unsafe { write_volatile(u.reg(IMSC), INT_RX | INT_RT) };
    crate::gic::enable(IRQ);
}

pub fn handle_irq() {
    let u = console();
    while let Some(b) = u.try_get_byte() {
        rx::push(b);
    }
    unsafe { write_volatile(u.reg(ICR), INT_RX | INT_RT) };
}

/// Klaviaturadan kelgan keyingi bayt (bo'lmasa — None).
pub fn read_byte() -> Option<u8> {
    rx::pop()
}

pub fn has_input() -> bool {
    !rx::is_empty()
}

/// Global konsol. Hozircha bitta yadro ishlaydi, shuning uchun qulf kerak emas;
/// SMP qo'shilganda bu yerga o'zimizning spinlock'imizni qo'yamiz.
pub fn console() -> Uart {
    Uart::new(UART0_BASE)
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use fmt::Write;
    let _ = console().write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::uart::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}
