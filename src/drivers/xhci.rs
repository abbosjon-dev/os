//! xHCI (USB 3) kontroller drayveri — faqat HID klaviatura/planşet/siçqonça uçun.
//!
//! Netcup kabi libvirt/KVM ARM VPS lar VNC kiritişini `qemu-xhci` + `usb-kbd` + `usb-tablet`
//! orqali beradi. Kontroller PCI sanovidan (sinf 0x0C0330) topiladi, BAR0 MMIO. Tuzilmalar:
//! DCBAA, buyruq halqasi, hodisa halqasi (+ ERST, 1 segment), har qurilmaga EP0 va
//! interrupt-IN uzatiş halqalari. Uzilişlar işlatilmaydi: hodisa halqasi sörov (poll)
//! bilan öqiladi (virtio-input-pci dagi kabi). Hamma xotira — identifikatsion xaritalangan
//! RAM sahifalari (VA = PA), şuning uçun DMA manzili sifatida tög'ridan-tög'ri beriladi.

use super::usb_hid;
use super::virtio_input::Event;
use crate::memory::frame;
use crate::sync::SpinLock;
use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};

const RING: usize = 256; // TRB soni (bitta 4 KiB sahifa)

// TRB turlari
const TRB_NORMAL: u32 = 1;
const TRB_SETUP: u32 = 2;
const TRB_DATA: u32 = 3;
const TRB_STATUS: u32 = 4;
const TRB_LINK: u32 = 6;
const TRB_ENABLE_SLOT: u32 = 9;
const TRB_ADDRESS_DEV: u32 = 11;
const TRB_CONFIG_EP: u32 = 12;
const TRB_EVAL_CTX: u32 = 13;
const EV_TRANSFER: u32 = 32;
const EV_CMD_DONE: u32 = 33;

// Tugalliş kodlari
const CC_SUCCESS: u32 = 1;
const CC_SHORT: u32 = 13;

// PORTSC
const PORT_CCS: u32 = 1 << 0;
const PORT_PED: u32 = 1 << 1;
const PORT_PR: u32 = 1 << 4;
const PORT_PRC: u32 = 1 << 21;
const PORT_RW1C: u32 = 0x00fe_0000 | PORT_PED; // özgariş bitlari + PED (1 yozilsa öçadi)

fn barrier() {
    unsafe { core::arch::asm!("dsb sy", options(nostack)) };
}
fn rd32(a: usize) -> u32 {
    unsafe { read_volatile(a as *const u32) }
}
fn wr32(a: usize, v: u32) {
    unsafe { write_volatile(a as *mut u32, v) }
}
fn wr64(a: usize, v: u64) {
    wr32(a, v as u32);
    wr32(a + 4, (v >> 32) as u32);
}
fn now() -> u64 {
    crate::timer::uptime_ms()
}
/// `cond` bajarilgunça (yoki `ms` ötgunça) kutadi.
fn wait_until(ms: u64, mut cond: impl FnMut() -> bool) -> bool {
    let t0 = now();
    loop {
        if cond() {
            return true;
        }
        if now() - t0 > ms {
            return false;
        }
        core::hint::spin_loop();
    }
}
fn page() -> Option<usize> {
    frame::alloc_zeroed()
}

/// Ishlab çiqaruvçi halqasi (buyruq yoki uzatiş): oxirgi TRB — boşiga Link (Toggle Cycle).
struct Ring {
    base: usize,
    idx: usize,
    cycle: u32,
}

impl Ring {
    fn new() -> Option<Ring> {
        Some(Ring { base: page()?, idx: 0, cycle: 1 })
    }
    /// TRB qöşadi; uning manzilini qaytaradi.
    fn push(&mut self, param: u64, status: u32, control: u32) -> usize {
        let t = self.base + self.idx * 16;
        unsafe {
            write_volatile(t as *mut u64, param);
            write_volatile((t + 8) as *mut u32, status);
        }
        barrier();
        wr32(t + 12, (control & !1) | self.cycle);
        self.idx += 1;
        if self.idx == RING - 1 {
            let l = self.base + self.idx * 16;
            unsafe {
                write_volatile(l as *mut u64, self.base as u64);
                write_volatile((l + 8) as *mut u32, 0);
            }
            barrier();
            wr32(l + 12, TRB_LINK << 10 | 1 << 1 | self.cycle);
            self.idx = 0;
            self.cycle ^= 1;
        }
        barrier();
        t
    }
    fn dequeue_ptr(&self) -> u64 {
        (self.base + self.idx * 16) as u64 | self.cycle as u64
    }
}

#[derive(Clone, Copy)]
struct Trb {
    status: u32,
    control: u32,
}
impl Trb {
    fn typ(&self) -> u32 {
        (self.control >> 10) & 0x3f
    }
    fn cc(&self) -> u32 {
        self.status >> 24
    }
    fn slot(&self) -> u32 {
        self.control >> 24
    }
    fn ep(&self) -> u32 {
        (self.control >> 16) & 0x1f
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Keyboard,
    Tablet,
    Mouse,
}

struct Hid {
    slot: u32,
    dci: u32,
    kind: Kind,
    ring: Ring,
    buf: usize,
    len: usize,
    prev: [u8; 8],
    pos: (u32, u32),
}

struct Hc {
    op: usize,
    db: usize,
    ir0: usize, // 0-interrupter registrlari
    ctx64: bool,
    dcbaa: usize,
    cmd: Ring,
    ev: usize,
    ev_idx: usize,
    ev_cycle: u32,
    devs: Vec<Hid>,
}
unsafe impl Send for Hc {}

static HC: SpinLock<Option<Hc>> = SpinLock::new(None);

impl Hc {
    fn ctx_size(&self) -> usize {
        if self.ctx64 { 64 } else { 32 }
    }
    fn portsc(&self, port: usize) -> usize {
        self.op + 0x400 + 0x10 * (port - 1)
    }
    fn ring_db(&self, slot: u32, target: u32) {
        barrier();
        wr32(self.db + 4 * slot as usize, target);
    }

    /// Hodisa halqasidan bitta hodisa (bölsa) — ERDP ni yangilaydi.
    fn next_event(&mut self) -> Option<Trb> {
        let t = self.ev + self.ev_idx * 16;
        let control = rd32(t + 12);
        if control & 1 != self.ev_cycle {
            return None;
        }
        barrier();
        let e = Trb { status: rd32(t + 8), control };
        self.ev_idx += 1;
        if self.ev_idx == RING {
            self.ev_idx = 0;
            self.ev_cycle ^= 1;
        }
        // ERDP: keyingi öqiladigan joy + EHB (1 yozib tozalanadi)
        wr64(self.ir0 + 0x18, (self.ev + self.ev_idx * 16) as u64 | 1 << 3);
        Some(e)
    }

    /// Berilgan turdagi hodisani kutadi (boşqalari taşlanadi — init paytida faqat
    /// port özgarişi hodisalari bölişi mumkin).
    fn wait_event(&mut self, typ: u32, ms: u64) -> Option<Trb> {
        let t0 = now();
        loop {
            while let Some(e) = self.next_event() {
                if e.typ() == typ {
                    return Some(e);
                }
            }
            if now() - t0 > ms {
                return None;
            }
            core::hint::spin_loop();
        }
    }

    fn command(&mut self, param: u64, control: u32) -> Option<Trb> {
        self.cmd.push(param, 0, control);
        self.ring_db(0, 0);
        let e = self.wait_event(EV_CMD_DONE, 500)?;
        (e.cc() == CC_SUCCESS).then_some(e)
    }

    /// EP0 orqali boşqaruv uzatişi. `buf` — DMA bufer (IN bölsa javob yoziladi).
    fn control(&mut self, ep0: &mut Ring, slot: u32, req: [u8; 8], buf: usize) -> Option<usize> {
        let setup = u64::from_le_bytes(req);
        let len = u16::from_le_bytes([req[6], req[7]]) as u32;
        let dir_in = req[0] & 0x80 != 0;
        let trt = if len == 0 { 0 } else if dir_in { 3 } else { 2 };
        ep0.push(setup, 8, TRB_SETUP << 10 | 1 << 6 | trt << 16);
        if len > 0 {
            ep0.push(buf as u64, len, TRB_DATA << 10 | (dir_in as u32) << 16);
        }
        let st_in = len == 0 || !dir_in;
        ep0.push(0, 0, TRB_STATUS << 10 | 1 << 5 | (st_in as u32) << 16);
        self.ring_db(slot, 1);
        let t0 = now();
        loop {
            let e = self.wait_event(EV_TRANSFER, 500)?;
            if e.slot() == slot && e.ep() == 1 {
                // xato (stall) yoki status bosqiçi tugadi
                return matches!(e.cc(), CC_SUCCESS | CC_SHORT).then_some(len as usize);
            }
            if now() - t0 > 500 {
                return None;
            }
        }
    }
}

const USB_DESC_DEVICE: u16 = 1;
const USB_DESC_CONFIG: u16 = 2;

fn get_desc(hc: &mut Hc, ep0: &mut Ring, slot: u32, typ: u16, len: u16, buf: usize) -> Option<()> {
    let v = (typ << 8).to_le_bytes();
    let l = len.to_le_bytes();
    hc.control(ep0, slot, [0x80, 6, v[0], v[1], 0, 0, l[0], l[1]], buf).map(|_| ())
}

/// Interfeys tanlovi: (sinf turi, interfeys raqami, EP manzili, max paket, interval, boot?).
struct Pick {
    kind: Kind,
    iface: u8,
    ep: u8,
    mps: u16,
    interval: u8,
    boot: bool,
}

/// Konfiguratsiya deskriptoridan birinçi HID interfeysi va uning interrupt-IN nuqtasini topadi.
fn parse_config(d: &[u8]) -> Option<Pick> {
    let mut i = 0;
    let mut cur: Option<(Kind, u8, bool)> = None;
    while i + 2 <= d.len() {
        let (len, typ) = (d[i] as usize, d[i + 1]);
        if len < 2 || i + len > d.len() {
            break;
        }
        let b = &d[i..i + len];
        if typ == 4 && len >= 9 {
            cur = (b[5] == 3).then(|| {
                let boot = b[6] == 1;
                let kind = match (boot, b[7]) {
                    (true, 1) => Kind::Keyboard,
                    (true, 2) => Kind::Mouse,
                    _ => Kind::Tablet, // QEMU usb-tablet: HID, boot emas, absolyut
                };
                (kind, b[2], boot)
            });
        } else if typ == 5 && len >= 7 {
            if let Some((kind, iface, boot)) = cur {
                if b[2] & 0x80 != 0 && b[3] & 3 == 3 {
                    let mps = u16::from_le_bytes([b[4], b[5]]) & 0x7ff;
                    return Some(Pick { kind, iface, ep: b[2], mps, interval: b[6], boot });
                }
            }
        }
        i += len;
    }
    None
}

/// Bitta portdagi qurilmani sozlaydi (reset → slot → manzil → deskriptorlar → konfiguratsiya).
fn setup_port(hc: &mut Hc, port: usize, max_slots: u32) -> Option<Hid> {
    let pa = hc.portsc(port);
    let sc = rd32(pa);
    if sc & PORT_CCS == 0 {
        return None;
    }
    // USB2 portlarini reset qilamiz (USB3 lar özi yoqiladi)
    if sc & PORT_PED == 0 {
        wr32(pa, (sc & !PORT_RW1C) | PORT_PR);
        wait_until(200, || rd32(pa) & PORT_PRC != 0);
        let s = rd32(pa);
        wr32(pa, (s & !PORT_RW1C) | PORT_PRC);
        if !wait_until(100, || rd32(pa) & PORT_PED != 0) {
            return None;
        }
    }
    let speed = (rd32(pa) >> 10) & 0xf;
    let e = hc.command(0, TRB_ENABLE_SLOT << 10)?;
    let slot = e.slot();
    if slot == 0 || slot > max_slots {
        return None;
    }
    let out_ctx = page()?;
    let input = page()?;
    let mut ep0 = Ring::new()?;
    let buf = page()?;
    unsafe { write_volatile((hc.dcbaa + slot as usize * 8) as *mut u64, out_ctx as u64) };
    let cs = hc.ctx_size();
    let mps0: u32 = match speed {
        2 => 8,   // low
        3 => 64,  // high
        4.. => 512, // super
        _ => 64,  // full (deskriptordan aniqlanadi)
    };
    // Input context: [boşqaruv][slot][EP0][EP1 OUT][EP1 IN]...
    let ctl = input;
    let slot_ctx = input + cs;
    let ep_ctx = |dci: u32| input + cs * (1 + dci as usize);
    wr32(ctl + 4, 0b11); // A0 (slot) | A1 (EP0)
    wr32(slot_ctx, 1 << 27 | speed << 20);
    wr32(slot_ctx + 4, (port as u32) << 16);
    let e0 = ep_ctx(1);
    wr32(e0 + 4, mps0 << 16 | 4 << 3 | 3 << 1); // control, CErr=3
    wr64(e0 + 8, ep0.dequeue_ptr());
    wr32(e0 + 16, 8);
    barrier();
    hc.command(input as u64, TRB_ADDRESS_DEV << 10 | slot << 24)?;

    // Qurilma deskriptori
    get_desc(hc, &mut ep0, slot, USB_DESC_DEVICE, 18, buf)?;
    let real_mps = match unsafe { read_volatile((buf + 7) as *const u8) } {
        m if speed >= 4 => 1u32 << m.min(9),
        m => m as u32,
    };
    if real_mps != 0 && real_mps != mps0 {
        wr32(ctl, 0);
        wr32(ctl + 4, 0b10); // A1
        wr32(e0 + 4, real_mps << 16 | 4 << 3 | 3 << 1);
        barrier();
        hc.command(input as u64, TRB_EVAL_CTX << 10 | slot << 24)?;
    }
    // Konfiguratsiya deskriptori (avval 9 bayt, keyin töliq)
    get_desc(hc, &mut ep0, slot, USB_DESC_CONFIG, 9, buf)?;
    let total = unsafe { read_volatile((buf + 2) as *const u16) }.clamp(9, 1024);
    let cfg_val = unsafe { read_volatile((buf + 5) as *const u8) };
    get_desc(hc, &mut ep0, slot, USB_DESC_CONFIG, total, buf)?;
    let desc: Vec<u8> = (0..total as usize).map(|i| unsafe { read_volatile((buf + i) as *const u8) }).collect();
    let pick = parse_config(&desc)?;

    // Configure Endpoint: interrupt-IN
    let epn = (pick.ep & 0xf) as u32;
    let dci = epn * 2 + 1;
    let ring = Ring::new()?;
    unsafe { core::ptr::write_bytes(input as *mut u8, 0, 4096) };
    wr32(ctl + 4, 1 | 1 << dci);
    wr32(slot_ctx, dci << 27 | speed << 20);
    wr32(slot_ctx + 4, (port as u32) << 16);
    let interval = if speed >= 3 {
        (pick.interval.clamp(1, 16) - 1) as u32
    } else {
        // FS/LS: bInterval ms → 2^n × 125 µs
        (31 - (pick.interval.max(1) as u32).leading_zeros() + 3).clamp(3, 10)
    };
    let mps = (pick.mps as u32).clamp(1, 64);
    let ec = ep_ctx(dci);
    wr32(ec, interval << 16);
    wr32(ec + 4, mps << 16 | 7 << 3 | 3 << 1); // interrupt IN, CErr=3
    wr64(ec + 8, ring.dequeue_ptr());
    wr32(ec + 16, mps << 16 | mps); // max ESIT payload, average TRB length
    barrier();
    hc.command(input as u64, TRB_CONFIG_EP << 10 | slot << 24)?;

    // SET_CONFIGURATION, boot protokoli (bölsa) va SET_IDLE(0)
    hc.control(&mut ep0, slot, [0x00, 9, cfg_val, 0, 0, 0, 0, 0], buf)?;
    if pick.boot {
        let _ = hc.control(&mut ep0, slot, [0x21, 0x0b, 0, 0, pick.iface, 0, 0, 0], buf);
    }
    let _ = hc.control(&mut ep0, slot, [0x21, 0x0a, 0, 0, pick.iface, 0, 0, 0], buf);
    Some(Hid {
        slot,
        dci,
        kind: pick.kind,
        ring,
        buf,
        len: mps as usize,
        prev: [0; 8],
        pos: (usb_hid::ABS_MAX / 2, usb_hid::ABS_MAX / 2),
    })
}

fn queue_in(hc: &Hc, d: &mut Hid) {
    d.ring.push(d.buf as u64, d.len as u32, TRB_NORMAL << 10 | 1 << 5 | 1 << 2); // IOC | ISP
    hc.ring_db(d.slot, d.dci);
}

/// xHCI kontrollerini işga tuşiradi. Natija: jurnal uçun qisqa matn (masalan,
/// "xHCI: 8 port, usb-kbd, usb-tablet") va topilgan HID qurilmalar soni.
pub fn init() -> Option<(String, usize)> {
    let pci = super::pci::xhci_nth(0)?;
    let base = pci.bar0;
    let cap_len = rd32(base) & 0xff;
    let hcs1 = rd32(base + 4);
    let hcs2 = rd32(base + 8);
    let hcc1 = rd32(base + 0x10);
    let (max_slots, max_ports) = (hcs1 & 0xff, (hcs1 >> 24) as usize);
    let op = base + cap_len as usize;
    let db = base + (rd32(base + 0x14) & !3) as usize;
    let rt = base + (rd32(base + 0x18) & !0x1f) as usize;

    // BIOS (UEFI) egaligini olamiz: USB Legacy Support kengaytmasi (ID 1)
    let mut x = ((hcc1 >> 16) as usize) << 2;
    while x != 0 {
        let a = base + x;
        let v = rd32(a);
        if v & 0xff == 1 {
            wr32(a, v | 1 << 24);
            wait_until(200, || rd32(a) & (1 << 16) == 0);
            wr32(a + 4, 0); // SMI larni öçiramiz
        }
        let next = ((v >> 8) & 0xff) as usize;
        x = if next == 0 { 0 } else { x + next * 4 };
    }

    // Töxtatiş va reset
    wr32(op, rd32(op) & !1);
    wait_until(100, || rd32(op + 4) & 1 != 0);
    wr32(op, 1 << 1);
    if !wait_until(500, || rd32(op) & (1 << 1) == 0 && rd32(op + 4) & (1 << 11) == 0) {
        crate::println!("  xHCI: reset tugamadi");
        return None;
    }
    let slots = max_slots.min(32);
    wr32(op + 0x38, slots);

    // DCBAA (+ scratchpad buferlari, kerak bölsa)
    let dcbaa = page()?;
    let nscr = ((hcs2 >> 27) & 0x1f) | ((hcs2 >> 21) & 0x1f) << 5;
    if nscr > 0 {
        let arr = page()?;
        for i in 0..(nscr as usize).min(512) {
            unsafe { write_volatile((arr + i * 8) as *mut u64, page()? as u64) };
        }
        unsafe { write_volatile(dcbaa as *mut u64, arr as u64) };
    }
    wr64(op + 0x30, dcbaa as u64);
    let cmd = Ring::new()?;
    wr64(op + 0x18, cmd.base as u64 | 1);

    // Hodisa halqasi + ERST (1 segment)
    let ev = page()?;
    let erst = page()?;
    unsafe {
        write_volatile(erst as *mut u64, ev as u64);
        write_volatile((erst + 8) as *mut u32, RING as u32);
    }
    let ir0 = rt + 0x20;
    wr32(ir0 + 0x08, 1); // ERSTSZ
    wr64(ir0 + 0x18, ev as u64);
    wr64(ir0 + 0x10, erst as u64);
    wr32(ir0, 0); // IMAN: uzilişlar öçiq (sörov)
    barrier();

    let mut hc = Hc {
        op, db, ir0,
        ctx64: hcc1 & (1 << 2) != 0,
        dcbaa, cmd, ev, ev_idx: 0, ev_cycle: 1, devs: Vec::new(),
    };
    // Işga tuşiramiz (R/S), portlar barqarorlaşişini kutamiz
    wr32(op, 1);
    wait_until(100, || rd32(op + 4) & 1 == 0);
    let t0 = now();
    while now() - t0 < 50 {
        while hc.next_event().is_some() {}
    }
    for port in 1..=max_ports {
        // Port quvvati (PP) öçiq bölsa — yoqamiz
        let pa = hc.portsc(port);
        let sc = rd32(pa);
        if sc & (1 << 9) == 0 {
            wr32(pa, (sc & !PORT_RW1C) | 1 << 9);
        }
    }
    wait_until(20, || false);
    let mut names = Vec::new();
    for port in 1..=max_ports {
        if let Some(d) = setup_port(&mut hc, port, slots) {
            names.push(match d.kind {
                Kind::Keyboard => "usb-kbd",
                Kind::Tablet => "usb-tablet",
                Kind::Mouse => "usb-mouse",
            });
            hc.devs.push(d);
        }
    }
    // Interrupt-IN uzatişlarini navbatga qöyamiz
    let mut devs = core::mem::take(&mut hc.devs);
    for d in devs.iter_mut() {
        queue_in(&hc, d);
    }
    hc.devs = devs;
    let n = hc.devs.len();
    let mut msg = alloc::format!("xHCI: {} port", max_ports);
    for nm in &names {
        msg.push_str(", ");
        msg.push_str(nm);
    }
    if names.is_empty() {
        msg.push_str(", HID qurilma yöq");
    }
    msg.push_str(&alloc::format!(" (PCI {:04x}:{:04x})", pci.vendor, pci.device));
    *HC.lock() = Some(hc);
    Some((msg, n))
}

/// Hodisa halqasini öqiydi: kelgan HID hisobotlarini virtio-input hodisalariga aylantiradi.
pub fn poll(out: &mut Vec<Event>) {
    let mut g = HC.lock();
    let Some(hc) = g.as_mut() else { return };
    let mut devs = core::mem::take(&mut hc.devs);
    while let Some(e) = hc.next_event() {
        if e.typ() != EV_TRANSFER {
            continue;
        }
        let Some(d) = devs.iter_mut().find(|d| d.slot == e.slot() && d.dci == e.ep()) else { continue };
        if matches!(e.cc(), CC_SUCCESS | CC_SHORT) {
            let got = d.len.saturating_sub((e.status & 0xff_ffff) as usize).min(64);
            let mut r = [0u8; 64];
            for (i, b) in r.iter_mut().enumerate().take(got) {
                *b = unsafe { read_volatile((d.buf + i) as *const u8) };
            }
            let r = &r[..got];
            match d.kind {
                Kind::Keyboard if got >= 8 => {
                    let cur: [u8; 8] = r[..8].try_into().unwrap_or([0; 8]);
                    out.extend(usb_hid::keyboard_events(&d.prev, &cur));
                    if !cur[2..].iter().any(|&u| (1..=3).contains(&u)) {
                        d.prev = cur;
                    }
                }
                Kind::Tablet if got >= 5 => {
                    out.extend(usb_hid::tablet_events(d.prev[0], r));
                    d.prev[0] = r[0];
                }
                Kind::Mouse if got >= 3 => {
                    out.extend(usb_hid::mouse_events(d.prev[0], r, &mut d.pos));
                    d.prev[0] = r[0];
                }
                _ => {}
            }
        }
        queue_in(hc, d);
    }
    hc.devs = devs;
}

pub fn present() -> bool {
    HC.lock().as_ref().is_some_and(|h| !h.devs.is_empty())
}
