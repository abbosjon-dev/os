//! virtio-sound (virtio-snd) drayveri — PCM övoz çiqariş (noldan).
//! Boshqaruv navbati (0) va TX navbati (2). PCM: SET_PARAMS/PREPARE/START/
//! STOP/RELEASE. Namunalar S16LE sifatida davriy buferlarda uzatiladi.
//!
//! Serversiz (headless) QEMU'da tovushni eşitib bölmaydi, ammo drayver
//! qurilmani dasturlaydi va haqiqiy apparatda övoz çiqaradi.

#![allow(dead_code)]

use crate::memory::{frame, PAGE_SIZE};
use crate::sync::Mutex;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

const MMIO_BASE: usize = 0x0a00_0000;
const MMIO_STRIDE: usize = 0x200;
const MMIO_SLOTS: usize = 32;

const MAGIC: usize = 0x000;
const VERSION: usize = 0x004;
const DEVICE_ID: usize = 0x008;
const DEVICE_FEATURES_SEL: usize = 0x014;
const DEVICE_FEATURES: usize = 0x010;
const DRIVER_FEATURES: usize = 0x020;
const DRIVER_FEATURES_SEL: usize = 0x024;
const GUEST_PAGE_SIZE: usize = 0x028;
const QUEUE_SEL: usize = 0x030;
const QUEUE_NUM_MAX: usize = 0x034;
const QUEUE_NUM: usize = 0x038;
const QUEUE_ALIGN: usize = 0x03c;
const QUEUE_PFN: usize = 0x040;
const QUEUE_READY: usize = 0x044;
const QUEUE_NOTIFY: usize = 0x050;
const INTERRUPT_STATUS: usize = 0x060;
const INTERRUPT_ACK: usize = 0x064;
const STATUS: usize = 0x070;
const QUEUE_DESC_LOW: usize = 0x080;
const QUEUE_DESC_HIGH: usize = 0x084;
const QUEUE_DRIVER_LOW: usize = 0x090;
const QUEUE_DRIVER_HIGH: usize = 0x094;
const QUEUE_DEVICE_LOW: usize = 0x0a0;
const QUEUE_DEVICE_HIGH: usize = 0x0a4;

const STATUS_ACK: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;

const DESC_NEXT: u16 = 1;
const DESC_WRITE: u16 = 2;
const QSIZE: usize = 16;

const DEVICE_SOUND: u32 = 25;

// Boshqaruv kodlari
const R_PCM_SET_PARAMS: u32 = 0x0101;
const R_PCM_PREPARE: u32 = 0x0102;
const R_PCM_RELEASE: u32 = 0x0103;
const R_PCM_START: u32 = 0x0104;
const R_PCM_STOP: u32 = 0x0105;
const S_OK: u32 = 0x8000;
const FMT_S16: u8 = 5;

#[repr(C)]
struct Desc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

fn rd(base: usize, off: usize) -> u32 {
    unsafe { read_volatile((base + off) as *const u32) }
}
fn wr(base: usize, off: usize, v: u32) {
    unsafe { write_volatile((base + off) as *mut u32, v) };
}
fn barrier() {
    unsafe { core::arch::asm!("dmb ish") };
}

struct Queue {
    mem: usize,
    idx: u32,
    last_used: u16,
}

impl Queue {
    fn setup(base: usize, version: u32, idx: u32) -> Option<Queue> {
        let mem = frame::alloc_contiguous(2)?;
        unsafe { core::ptr::write_bytes(mem as *mut u8, 0, 2 * PAGE_SIZE) };
        wr(base, QUEUE_SEL, idx);
        if (rd(base, QUEUE_NUM_MAX) as usize) < QSIZE {
            return None;
        }
        wr(base, QUEUE_NUM, QSIZE as u32);
        let (desc, avail, used) = (mem as u64, (mem + 128) as u64, (mem + PAGE_SIZE) as u64);
        if version == 1 {
            wr(base, GUEST_PAGE_SIZE, PAGE_SIZE as u32);
            wr(base, QUEUE_ALIGN, PAGE_SIZE as u32);
            wr(base, QUEUE_PFN, (mem / PAGE_SIZE) as u32);
        } else {
            wr(base, QUEUE_DESC_LOW, desc as u32);
            wr(base, QUEUE_DESC_HIGH, (desc >> 32) as u32);
            wr(base, QUEUE_DRIVER_LOW, avail as u32);
            wr(base, QUEUE_DRIVER_HIGH, (avail >> 32) as u32);
            wr(base, QUEUE_DEVICE_LOW, used as u32);
            wr(base, QUEUE_DEVICE_HIGH, (used >> 32) as u32);
            wr(base, QUEUE_READY, 1);
        }
        Some(Queue { mem, idx, last_used: 0 })
    }

    /// Buferlar zanjirini yuboradi va tugaşini aylanib kutadi.
    fn submit(&mut self, base: usize, bufs: &[(usize, usize, bool)]) {
        unsafe {
            let desc = self.mem as *mut Desc;
            for (i, &(addr, len, write)) in bufs.iter().enumerate() {
                let mut flags = if write { DESC_WRITE } else { 0 };
                if i + 1 < bufs.len() {
                    flags |= DESC_NEXT;
                }
                desc.add(i).write_volatile(Desc { addr: addr as u64, len: len as u32, flags, next: (i + 1) as u16 });
            }
            let avail = (self.mem + 128) as *mut u16;
            let aidx = read_volatile(avail.add(1));
            write_volatile(avail.add(2 + (aidx as usize % QSIZE)), 0);
            barrier();
            write_volatile(avail.add(1), aidx.wrapping_add(1));
            barrier();
        }
        wr(base, QUEUE_NOTIFY, self.idx);
        // tugaşini kutamiz
        let used_idx = (self.mem + PAGE_SIZE + 2) as *const u16;
        let mut guard = 0u64;
        while unsafe { read_volatile(used_idx) } == self.last_used {
            core::hint::spin_loop();
            guard += 1;
            if guard > 100_000_000 {
                break;
            }
        }
        self.last_used = self.last_used.wrapping_add(1);
        unsafe {
            let st = read_volatile((base + INTERRUPT_STATUS) as *const u32);
            write_volatile((base + INTERRUPT_ACK) as *mut u32, st);
        }
    }
}

struct Snd {
    base: usize,
    ctrl: Queue,
    tx: Queue,
    buf: usize, // DMA bufer (boshqaruv + period uçun)
}
unsafe impl Send for Snd {}

static SND: Mutex<Option<Snd>> = Mutex::new(None);
static READY: AtomicBool = AtomicBool::new(false);
static PLAYING: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);

pub fn present() -> bool {
    READY.load(Ordering::Acquire)
}

pub fn init() -> bool {
    let base = match (0..MMIO_SLOTS)
        .map(|i| MMIO_BASE + i * MMIO_STRIDE)
        .find(|&b| rd(b, MAGIC) == 0x7472_6976 && rd(b, DEVICE_ID) == DEVICE_SOUND)
    {
        Some(b) => b,
        None => return false,
    };
    let version = rd(base, VERSION);
    wr(base, STATUS, 0);
    wr(base, STATUS, STATUS_ACK);
    wr(base, STATUS, STATUS_ACK | STATUS_DRIVER);
    // v2: VERSION_1 (bit 32) majburiy
    let features_ok = if version >= 2 {
        wr(base, DRIVER_FEATURES_SEL, 1);
        wr(base, DRIVER_FEATURES, 1);
        wr(base, STATUS, STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK);
        if rd(base, STATUS) & STATUS_FEATURES_OK == 0 {
            return false;
        }
        STATUS_FEATURES_OK
    } else {
        0
    };
    let Some(ctrl) = Queue::setup(base, version, 0) else { return false };
    let Some(tx) = Queue::setup(base, version, 2) else { return false };
    let Some(buf) = frame::alloc_contiguous(16) else { return false }; // 64 KiB period
    wr(base, STATUS, STATUS_ACK | STATUS_DRIVER | features_ok | STATUS_DRIVER_OK);
    *SND.lock() = Some(Snd { base, ctrl, tx, buf });
    READY.store(true, Ordering::Release);
    true
}

/// Namuna chastotasidan virtio rate kodi.
fn rate_code(rate: u32) -> u8 {
    match rate {
        5512 => 0,
        8000 => 1,
        11025 => 2,
        16000 => 3,
        22050 => 4,
        32000 => 5,
        44100 => 6,
        48000 => 7,
        64000 => 8,
        88200 => 9,
        96000 => 10,
        _ => 7, // standart 48000 ga yaqin
    }
}

/// Boshqaruv sörovi (kod + stream_id + qoşimça baytlar). Natija: holat (S_OK = ok).
fn ctrl(s: &mut Snd, code: u32, stream_id: u32, extra: &[u8]) -> u32 {
    let req = s.buf; // so'rov
    let resp = s.buf + 2048; // javob
    unsafe {
        write_volatile(req as *mut u32, code);
        write_volatile((req + 4) as *mut u32, stream_id);
        for (i, &b) in extra.iter().enumerate() {
            write_volatile((req + 8 + i) as *mut u8, b);
        }
        write_volatile(resp as *mut u32, 0);
    }
    s.ctrl.submit(s.base, &[(req, 8 + extra.len(), false), (resp, 4, true)]);
    unsafe { read_volatile(resp as *const u32) }
}

/// Övoz çaladi (fon oqimida). `from_ms` — boşlangʼiç örin.
pub fn play(samples: &[i16], rate: u32, channels: u16, from_ms: u32) {
    if !READY.load(Ordering::Acquire) {
        return;
    }
    STOP.store(true, Ordering::Release); // oldingisini töxtatamiz
    // Namunalarni S16LE baytlarga aylantirib, davomli buferga saqlaymiz
    let start = (from_ms as u64 * rate as u64 / 1000 * channels as u64) as usize;
    let slice = if start < samples.len() { &samples[start..] } else { &[] };
    let mut bytes = Vec::with_capacity(slice.len() * 2);
    for &s in slice {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    *JOB.lock() = Some(Job { bytes, rate, channels });
    STOP.store(false, Ordering::Release);
    PLAYING.store(true, Ordering::Release);
    // Fon oqimi bir marta ishga tuşiriladi (init da)
}

pub fn stop() {
    STOP.store(true, Ordering::Release);
    PLAYING.store(false, Ordering::Release);
}

struct Job {
    bytes: Vec<u8>,
    rate: u32,
    channels: u16,
}
static JOB: Mutex<Option<Job>> = Mutex::new(None);

/// Fon oqimi: ijro sörovlari kelganda PCM ni qurilmaga uzatadi.
pub extern "C" fn player(_: usize) {
    loop {
        if !PLAYING.load(Ordering::Acquire) {
            crate::task::sleep_ms(40);
            continue;
        }
        let job = JOB.lock().take();
        let Some(job) = job else {
            PLAYING.store(false, Ordering::Release);
            continue;
        };
        play_job(&job);
        PLAYING.store(false, Ordering::Release);
    }
}

fn play_job(job: &Job) {
    let mut guard = SND.lock();
    let Some(s) = guard.as_mut() else { return };
    let period = 8192usize; // bytes (4096 S16 namuna)
    let total = job.bytes.len();
    if total == 0 {
        return;
    }
    let buffer_bytes = total.div_ceil(period) * period;
    // SET_PARAMS: buffer_bytes(4) period_bytes(4) features(4) channels(1) format(1) rate(1) pad(1)
    let mut p = Vec::new();
    p.extend_from_slice(&(buffer_bytes as u32).to_le_bytes());
    p.extend_from_slice(&(period as u32).to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    p.push(job.channels.max(1).min(2) as u8);
    p.push(FMT_S16);
    p.push(rate_code(job.rate));
    p.push(0);
    if ctrl(s, R_PCM_SET_PARAMS, 0, &p) != S_OK {
        return;
    }
    if ctrl(s, R_PCM_PREPARE, 0, &[]) != S_OK {
        return;
    }
    if ctrl(s, R_PCM_START, 0, &[]) != S_OK {
        return;
    }
    // Davriy buferlarni uzatamiz: [xfer hdr: stream_id][PCM data][status]
    let xfer = s.buf + 4096; // stream_id (4 bayt)
    let data = s.buf + 8192; // PCM period
    let st = s.buf + 8192 + period; // status (8 bayt)
    unsafe { write_volatile(xfer as *mut u32, 0u32) };
    let mut off = 0;
    while off < total {
        if STOP.load(Ordering::Acquire) {
            break;
        }
        let n = (total - off).min(period);
        unsafe {
            core::ptr::copy_nonoverlapping(job.bytes.as_ptr().add(off), data as *mut u8, n);
            if n < period {
                core::ptr::write_bytes((data + n) as *mut u8, 0, period - n); // qolgani jim
            }
        }
        // drop guard oldidan submit — s ni band tutamiz
        s.tx.submit(s.base, &[(xfer, 4, false), (data, period, false), (st, 8, true)]);
        off += period;
    }
    let _ = ctrl(s, R_PCM_STOP, 0, &[]);
    let _ = ctrl(s, R_PCM_RELEASE, 0, &[]);
}
