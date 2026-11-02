#![allow(dead_code)] // grafika API — keyingi bosqiçlarda işlatiladi
//! virtio-gpu 2D drayveri: ekran (scanout) uçun framebuffer yaratadi va yangilaydi.
//! Faqat boşqaruv navbati (controlq). Sinxron buyruqlar.
//!
//! Framebuffer — oddiy RAM: har bir piksel u32 (B8G8R8X8, kiçik-endian → 0x00RRGGBB).
//! Çiziş framebufferga yoziladi, keyin `flush` qurilmaga köçiradi va ekranga çiqaradi.

use super::virtio::{Transport, DEVICE_GPU};
use crate::memory::{PAGE_SIZE, frame};
use crate::sync::Mutex;
use core::ptr::{read_volatile, write_volatile};

const CMD_GET_DISPLAY_INFO: u32 = 0x0100;
const CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const CMD_SET_SCANOUT: u32 = 0x0103;
const CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const CMD_RESOURCE_FLUSH: u32 = 0x0104;
const RESP_OK_NODATA: u32 = 0x1100;
const FORMAT_B8G8R8X8: u32 = 2;
const RESOURCE_ID: u32 = 1;

struct Gpu {
    t: Transport,
    cmd: usize,  // DMA: buyruq buferi
    resp: usize, // DMA: javob buferi
    fb: usize,   // framebuffer (fizik = virtual)
    width: u32,
    height: u32,
}

unsafe impl Send for Gpu {}

static GPU: Mutex<Option<Gpu>> = Mutex::new(None);

fn wr32(addr: usize, off: usize, v: u32) {
    unsafe { write_volatile((addr + off) as *mut u32, v) };
}
fn wr64(addr: usize, off: usize, v: u64) {
    unsafe { write_volatile((addr + off) as *mut u64, v) };
}

impl Gpu {
    /// Buyruqni boşlaydi: tanani nollab, sarlavhani örnatadi. Keyin çaqiruvçi tanani yozadi.
    fn begin(&mut self, cmd_type: u32) {
        unsafe { core::ptr::write_bytes((self.cmd + 24) as *mut u8, 0, 64) };
        wr32(self.cmd, 0, cmd_type); // type
        wr32(self.cmd, 4, 0); // flags
        wr64(self.cmd, 8, 0); // fence_id
        wr32(self.cmd, 16, 0); // ctx_id
        wr32(self.cmd, 20, 0); // padding
    }

    /// Yuboradi va javob turini qaytaradi.
    fn run(&mut self, body_len: usize) -> u32 {
        self.t.submit(&[(self.cmd, 24 + body_len, false), (self.resp, 24, true)]);
        unsafe { core::ptr::read_volatile(self.resp as *const u32) }
    }

    fn rect(&self, off: usize, x: u32, y: u32, w: u32, h: u32) {
        wr32(self.cmd, off, x);
        wr32(self.cmd, off + 4, y);
        wr32(self.cmd, off + 8, w);
        wr32(self.cmd, off + 12, h);
    }

    /// Ekranning afzal körilgan (preferred) ölçamini soraydi (scanout 0).
    /// Natija: (kenglik, balandlik) yoki yöq.
    fn display_info(&mut self) -> Option<(u32, u32)> {
        self.begin(CMD_GET_DISPLAY_INFO);
        // Javob: hdr(24) + 16 ta display_one(24 bayt): rect(16)+enabled(4)+flags(4)
        self.t.submit(&[(self.cmd, 24, false), (self.resp, 24 + 16 * 24, true)]);
        let rd = |off: usize| unsafe { read_volatile((self.resp + off) as *const u32) };
        // scanout 0 — hdr dan keyin: rect(x,y,w,h), enabled
        let w = rd(24 + 8);
        let h = rd(24 + 12);
        let enabled = rd(24 + 16);
        if enabled != 0 && (320..=4096).contains(&w) && (240..=2160).contains(&h) {
            Some((w & !1, h & !1))
        } else {
            None
        }
    }
}

/// GPU ni topadi va `width`×`height` framebuffer tayyorlaydi. Natija: (width, height, fb).
pub fn init(req_w: u32, req_h: u32) -> Option<(u32, u32, usize)> {
    let t = Transport::probe(DEVICE_GPU)?;
    let cmd = frame::alloc_zeroed()?;
    let resp = frame::alloc_zeroed()?;
    // Ekranning afzal körilgan ölçamini soraymiz — har qanday ölçamga moslaşamiz.
    let mut g = Gpu { t, cmd, resp, fb: 0, width: 0, height: 0 };
    let cap = |w: u32, h: u32| (w.min(1920), h.min(1200));
    let (mut width, mut height) = g.display_info().map(|(w, h)| cap(w, h)).unwrap_or((req_w, req_h));
    // Framebuffer uçun qoşni xotira; katta ölçam ajralmasa — sorovga qaytamiz.
    let mut fb_pages = (width as usize * height as usize * 4).div_ceil(PAGE_SIZE);
    let fb = frame::alloc_contiguous(fb_pages).or_else(|| {
        width = req_w;
        height = req_h;
        fb_pages = (width as usize * height as usize * 4).div_ceil(PAGE_SIZE);
        frame::alloc_contiguous(fb_pages)
    })?;
    unsafe { core::ptr::write_bytes(fb as *mut u8, 0, fb_pages * PAGE_SIZE) };
    g.fb = fb;
    g.width = width;
    g.height = height;

    // 1) Resurs yaratamiz (2D, B8G8R8X8)
    g.begin(CMD_RESOURCE_CREATE_2D);
    wr32(g.cmd, 24, RESOURCE_ID);
    wr32(g.cmd, 28, FORMAT_B8G8R8X8);
    wr32(g.cmd, 32, width);
    wr32(g.cmd, 36, height);
    if g.run(16) != RESP_OK_NODATA {
        return None;
    }
    // 2) Framebufferni resursga biriktiramiz (1 ta mem yozuvi)
    g.begin(CMD_RESOURCE_ATTACH_BACKING);
    wr32(g.cmd, 24, RESOURCE_ID);
    wr32(g.cmd, 28, 1); // nr_entries
    wr64(g.cmd, 32, fb as u64);
    wr32(g.cmd, 40, (width * height * 4) as u32);
    wr32(g.cmd, 44, 0);
    if g.run(24) != RESP_OK_NODATA {
        return None;
    }
    // 3) Ekranga (scanout 0) bogʻlaymiz
    g.begin(CMD_SET_SCANOUT);
    g.rect(24, 0, 0, width, height);
    wr32(g.cmd, 40, 0); // scanout_id
    wr32(g.cmd, 44, RESOURCE_ID);
    if g.run(24) != RESP_OK_NODATA {
        return None;
    }
    *GPU.lock() = Some(g);
    flush();
    Some((width, height, fb))
}

pub fn present() -> bool {
    GPU.lock().is_some()
}

pub fn resolution() -> (u32, u32) {
    GPU.lock().as_ref().map_or((0, 0), |g| (g.width, g.height))
}

/// Framebufferni qurilmaga köçirib, ekranga çiqaradi.
pub fn flush() {
    let mut guard = GPU.lock();
    let Some(g) = guard.as_mut() else { return };
    let (w, h) = (g.width, g.height);
    // Transfer to host
    g.begin(CMD_TRANSFER_TO_HOST_2D);
    g.rect(24, 0, 0, w, h);
    wr64(g.cmd, 40, 0); // offset
    wr32(g.cmd, 48, RESOURCE_ID);
    wr32(g.cmd, 52, 0);
    g.run(32);
    // Flush
    g.begin(CMD_RESOURCE_FLUSH);
    g.rect(24, 0, 0, w, h);
    wr32(g.cmd, 40, RESOURCE_ID);
    wr32(g.cmd, 44, 0);
    g.run(24);
}

/// Faqat berilgan töğriburçakni yangilaydi (tezroq).
pub fn flush_rect(x: u32, y: u32, w: u32, h: u32) {
    let mut guard = GPU.lock();
    let Some(g) = guard.as_mut() else { return };
    let off = (y as u64 * g.width as u64 + x as u64) * 4;
    g.begin(CMD_TRANSFER_TO_HOST_2D);
    g.rect(24, x, y, w, h);
    wr64(g.cmd, 40, off);
    wr32(g.cmd, 48, RESOURCE_ID);
    wr32(g.cmd, 52, 0);
    g.run(32);
    g.begin(CMD_RESOURCE_FLUSH);
    g.rect(24, x, y, w, h);
    wr32(g.cmd, 40, RESOURCE_ID);
    wr32(g.cmd, 44, 0);
    g.run(24);
}
