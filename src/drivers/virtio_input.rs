#![allow(dead_code)] // kiriş drayveri API — keyingi bosqiçlarda
//! virtio-input drayveri (klaviatura va planşet). virtio-mmio ni özi boşqaradi.
//! Hodisa navbati (queue 0): qurilma 8 baytli hodisalar yozadi (type u16, code u16, value u32).

use crate::memory::{PAGE_SIZE, frame};
use crate::sync::SpinLock;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};

const MMIO_BASE: usize = 0x0a00_0000;
const MMIO_STRIDE: usize = 0x200;
const MMIO_SLOTS: usize = 32;
const DEVICE_INPUT: u32 = 18;

const DEVICE_ID: usize = 0x008;
const DRIVER_FEATURES: usize = 0x020;
const DRIVER_FEATURES_SEL: usize = 0x024;
const GUEST_PAGE_SIZE: usize = 0x028;
const QUEUE_SEL: usize = 0x030;
const QUEUE_NUM: usize = 0x038;
const QUEUE_ALIGN: usize = 0x03c;
const QUEUE_PFN: usize = 0x040;
const QUEUE_READY: usize = 0x044;
const QUEUE_NOTIFY: usize = 0x050;
const STATUS: usize = 0x070;
const QUEUE_DESC_LOW: usize = 0x080;
const QUEUE_DESC_HIGH: usize = 0x084;
const QUEUE_DRIVER_LOW: usize = 0x090;
const QUEUE_DRIVER_HIGH: usize = 0x094;
const QUEUE_DEVICE_LOW: usize = 0x0a0;
const QUEUE_DEVICE_HIGH: usize = 0x0a4;

const QSIZE: usize = 64;
const EVENT_SIZE: usize = 8;

#[derive(Clone, Copy)]
pub struct Event {
    pub typ: u16,
    pub code: u16,
    pub value: u32,
}

pub const EV_KEY: u16 = 1;
pub const EV_ABS: u16 = 3;
pub const ABS_X: u16 = 0;
pub const ABS_Y: u16 = 1;
pub const BTN_LEFT: u16 = 0x110;

struct Dev {
    base: usize,
    queue: usize,
    bufs: usize,
    last_used: u16,
    avail_idx: u16,
}
unsafe impl Send for Dev {}

static DEVICES: SpinLock<Vec<Dev>> = SpinLock::new(Vec::new());

fn barrier() {
    unsafe { core::arch::asm!("dsb sy", options(nostack)) };
}

impl Dev {
    fn rd(&self, o: usize) -> u32 {
        unsafe { read_volatile((self.base + o) as *const u32) }
    }
    fn wr(&self, o: usize, v: u32) {
        unsafe { write_volatile((self.base + o) as *mut u32, v) }
    }
    fn desc(&self) -> usize {
        self.queue
    }
    fn avail(&self) -> usize {
        self.queue + QSIZE * 16
    }
    fn used(&self) -> usize {
        self.queue + PAGE_SIZE
    }

    fn setup(&mut self, version: u32) {
        self.wr(STATUS, 0);
        self.wr(STATUS, 1);
        self.wr(STATUS, 1 | 2);
        self.wr(DRIVER_FEATURES_SEL, 0);
        self.wr(DRIVER_FEATURES, 0);
        if version >= 2 {
            self.wr(DRIVER_FEATURES_SEL, 1);
            self.wr(DRIVER_FEATURES, 1);
            self.wr(STATUS, 1 | 2 | 8);
        }
        self.wr(QUEUE_SEL, 0);
        self.wr(QUEUE_NUM, QSIZE as u32);
        let (d, a, u) = (self.desc() as u64, self.avail() as u64, self.used() as u64);
        if version == 1 {
            self.wr(GUEST_PAGE_SIZE, PAGE_SIZE as u32);
            self.wr(QUEUE_ALIGN, PAGE_SIZE as u32);
            self.wr(QUEUE_PFN, (self.queue / PAGE_SIZE) as u32);
        } else {
            self.wr(QUEUE_DESC_LOW, d as u32);
            self.wr(QUEUE_DESC_HIGH, (d >> 32) as u32);
            self.wr(QUEUE_DRIVER_LOW, a as u32);
            self.wr(QUEUE_DRIVER_HIGH, (a >> 32) as u32);
            self.wr(QUEUE_DEVICE_LOW, u as u32);
            self.wr(QUEUE_DEVICE_HIGH, (u >> 32) as u32);
            self.wr(QUEUE_READY, 1);
        }
        let fok = if version >= 2 { 8 } else { 0 };
        self.wr(STATUS, 1 | 2 | fok | 4);
        self.fill();
    }

    fn fill(&mut self) {
        for i in 0..QSIZE {
            let d = self.desc() + i * 16;
            unsafe {
                write_volatile(d as *mut u64, (self.bufs + i * EVENT_SIZE) as u64);
                write_volatile((d + 8) as *mut u32, EVENT_SIZE as u32);
                write_volatile((d + 12) as *mut u16, 2); // WRITE
                write_volatile((d + 14) as *mut u16, 0);
                write_volatile((self.avail() + 4 + i * 2) as *mut u16, i as u16);
            }
        }
        barrier();
        unsafe { write_volatile((self.avail() + 2) as *mut u16, QSIZE as u16) };
        self.avail_idx = QSIZE as u16;
        barrier();
        self.wr(QUEUE_NOTIFY, 0);
    }

    fn drain(&mut self, out: &mut Vec<Event>) {
        let used = self.used();
        loop {
            let idx = unsafe { read_volatile((used + 2) as *const u16) };
            if idx == self.last_used {
                break;
            }
            barrier();
            let slot = self.last_used as usize % QSIZE;
            let id = unsafe { read_volatile((used + 4 + slot * 8) as *const u32) } as usize;
            if id < QSIZE {
                let b = self.bufs + id * EVENT_SIZE;
                out.push(Event {
                    typ: unsafe { read_volatile(b as *const u16) },
                    code: unsafe { read_volatile((b + 2) as *const u16) },
                    value: unsafe { read_volatile((b + 4) as *const u32) },
                });
                unsafe {
                    write_volatile((self.avail() + 4 + (self.avail_idx as usize % QSIZE) * 2) as *mut u16, id as u16);
                }
                self.avail_idx = self.avail_idx.wrapping_add(1);
            }
            self.last_used = self.last_used.wrapping_add(1);
        }
        barrier();
        unsafe { write_volatile((self.avail() + 2) as *mut u16, self.avail_idx) };
        barrier();
        self.wr(QUEUE_NOTIFY, 0);
    }
}

pub fn init() -> usize {
    let mut devs = DEVICES.lock();
    for i in 0..MMIO_SLOTS {
        let base = MMIO_BASE + i * MMIO_STRIDE;
        let magic = unsafe { read_volatile(base as *const u32) };
        let id = unsafe { read_volatile((base + DEVICE_ID) as *const u32) };
        if magic != 0x7472_6976 || id != DEVICE_INPUT {
            continue;
        }
        let (Some(queue), Some(bufs)) = (frame::alloc_contiguous(2), frame::alloc_zeroed()) else { continue };
        unsafe { core::ptr::write_bytes(queue as *mut u8, 0, 2 * PAGE_SIZE) };
        let version = unsafe { read_volatile((base + 0x004) as *const u32) };
        let mut d = Dev { base, queue, bufs, last_used: 0, avail_idx: 0 };
        d.setup(version);
        devs.push(d);
    }
    devs.len()
}

pub fn poll() -> Vec<Event> {
    let mut out = Vec::new();
    let mut devs = DEVICES.lock();
    for d in devs.iter_mut() {
        d.drain(&mut out);
    }
    out
}

pub fn present() -> bool {
    !DEVICES.lock().is_empty()
}
