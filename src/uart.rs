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
const ICR: usize = 0x44;

const FR_TXFF: u32 = 1 << 5; // TX FIFO to'la
#[allow(dead_code)]
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

    #[allow(dead_code)] // keyingi bosqichda shell uchun kerak bo'ladi
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
