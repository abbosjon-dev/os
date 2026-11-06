//! Minimal TCP mijozi: ulanadi, maʼlumot jönatadi/oladi, yopadi.
//! Bitta oyna, qayta jönatiş yöq (QEMU user-net işonçli) — oddiy, lekin işlaydi.

use super::{Ip, checksum};
use crate::sync::SpinLock;
use crate::task;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

const EVENT: usize = 0x7463_7000; // "tcp"
const MAX_CONN: usize = 16;

#[derive(Clone, Copy, PartialEq)]
enum St {
    SynSent,
    Open,
    Closed,
}

/// Tasdiq kutayotgan jönatilgan segment (qayta jönatiş uçun).
struct Unacked {
    seq: u32,
    flags: u8,
    data: Vec<u8>,
    sent_ms: u64,
    retries: u8,
}

struct Conn {
    /// Egasi (jarayon PID): faqat u bu ulanişni öqiy/yoza oladi
    owner: usize,
    local_port: u16,
    remote: Ip,
    remote_port: u16,
    seq: u32,
    ack: u32,
    state: St,
    inbox: VecDeque<u8>,
    got_synack: bool,
    /// Tasdiq kutayotgan segmentlar (eng eskisidan)
    unacked: VecDeque<Unacked>,
    /// Peer tasdiqlagan eng katta seq
    snd_una: u32,
}

const RTO_MS: u64 = 200;
const MAX_RETRIES: u8 = 6;

/// Seq maydonini egallaydigan segment: saqlanadi va kerak bölsa qayta jönatiladi.
fn send_tracked(c: &mut Conn, flags: u8, data: Vec<u8>) {
    send_segment(c, flags, &data);
    let seq = c.seq;
    let consumed = data.len() as u32 + if flags & (SYN | FIN) != 0 { 1 } else { 0 };
    c.seq = c.seq.wrapping_add(consumed);
    if consumed > 0 {
        c.unacked.push_back(Unacked { seq, flags, data, sent_ms: crate::timer::uptime_ms(), retries: 0 });
    }
}

/// Peer `ack` gaça bölgan segmentlarni tasdiqlangan deb olib taşlaydi.
fn acknowledge(c: &mut Conn, ack: u32) {
    c.snd_una = ack;
    while let Some(u) = c.unacked.front() {
        let end = u.seq.wrapping_add(u.data.len() as u32 + if u.flags & (SYN | FIN) != 0 { 1 } else { 0 });
        // end <= ack (wrapping): tasdiqlangan
        if ack.wrapping_sub(end) < 0x8000_0000 {
            c.unacked.pop_front();
        } else {
            break;
        }
    }
}

/// Davriy tekşiruv (tarmoq oqimidan): vaqtida tasdiq kelmagan segmentlarni qayta jönatadi.
pub fn tick() {
    if tick_locked() {
        task::wake_event(EVENT); // qulfdan taşqarida (qarang: handle)
    }
}

fn tick_locked() -> bool {
    let now = crate::timer::uptime_ms();
    let mut wake = false;
    let mut cs = CONNS.lock();
    for slot in cs.iter_mut() {
        let Some(c) = slot.as_mut() else { continue };
        if c.state == St::Closed {
            continue;
        }
        if let Some(u) = c.unacked.front_mut() {
            if now.saturating_sub(u.sent_ms) >= RTO_MS {
                if u.retries >= MAX_RETRIES {
                    c.state = St::Closed;
                    wake = true;
                    continue;
                }
                u.retries += 1;
                u.sent_ms = now;
                let (seq, flags, data) = (u.seq, u.flags, u.data.clone());
                // seq ni vaqtinça örnatib qayta jönatamiz
                let saved = c.seq;
                c.seq = seq;
                send_segment(c, flags, &data);
                c.seq = saved;
            }
        }
    }
    wake
}

static CONNS: SpinLock<Vec<Option<Conn>>> = SpinLock::new(Vec::new());
static NEXT_PORT: core::sync::atomic::AtomicU16 = core::sync::atomic::AtomicU16::new(49152);

const FIN: u8 = 1;
const SYN: u8 = 2;
const RST: u8 = 4;
const PSH: u8 = 8;
const ACK: u8 = 16;

fn send_segment(c: &Conn, flags: u8, payload: &[u8]) {
    let src = super::our_ip();
    let mut seg = Vec::with_capacity(20 + payload.len());
    seg.extend_from_slice(&c.local_port.to_be_bytes());
    seg.extend_from_slice(&c.remote_port.to_be_bytes());
    seg.extend_from_slice(&c.seq.to_be_bytes());
    seg.extend_from_slice(&c.ack.to_be_bytes());
    seg.extend_from_slice(&[0x50, flags]); // data offset 5, bayroqlar
    seg.extend_from_slice(&[0xff, 0xff]); // oyna
    seg.extend_from_slice(&[0, 0, 0, 0]); // checksum + urgent
    seg.extend_from_slice(payload);
    let len = seg.len();
    let pseudo = [src[0], src[1], src[2], src[3], c.remote[0], c.remote[1], c.remote[2], c.remote[3], 0, super::IP_TCP, (len >> 8) as u8, len as u8];
    let ck = checksum(&[&pseudo, &seg]);
    seg[16..18].copy_from_slice(&ck.to_be_bytes());
    super::send_ipv4(c.remote, super::IP_TCP, &seg);
}

/// Ulaniş. Natija: ulaniş raqami (handle) yoki None.
pub fn connect(remote: Ip, remote_port: u16) -> Option<usize> {
    // ARP ni qulfdan TAŞQARIDA hal qilamiz: aks holda CONNS qulfi uşlangan holda
    // kutib qolinadi va tarmoq oqimi (tcp::tick) shu qulfda tiqilib qoladi.
    super::resolve(remote)?;
    let local_port = NEXT_PORT.fetch_add(1, core::sync::atomic::Ordering::Relaxed).max(49152);
    let seq = (crate::timer::uptime_ms() as u32).wrapping_mul(2654435761);
    let conn = Conn {
        owner: task::current_pid(),
        local_port,
        remote,
        remote_port,
        seq,
        ack: 0,
        state: St::SynSent,
        inbox: VecDeque::new(),
        got_synack: false,
        unacked: VecDeque::new(),
        snd_una: seq,
    };
    let id = {
        let mut cs = CONNS.lock();
        let id = match cs.iter().position(|c| c.is_none()) {
            Some(i) => {
                cs[i] = Some(conn);
                i
            }
            None if cs.len() < MAX_CONN => {
                cs.push(Some(conn));
                cs.len() - 1
            }
            None => return None,
        };
        send_tracked(cs[id].as_mut().unwrap(), SYN, Vec::new());
        id
    };
    // SYN-ACK kutamiz
    let start = crate::timer::uptime_ms();
    while crate::timer::uptime_ms() - start < 3000 {
        {
            let mut cs = CONNS.lock();
            if let Some(c) = cs[id].as_mut() {
                if c.got_synack {
                    c.got_synack = false;
                    c.state = St::Open;
                    send_segment(c, ACK, &[]); // toza ACK (seq maydonini egallamaydi)
                    return Some(id);
                }
                if c.state == St::Closed {
                    break;
                }
            }
        }
        task::sleep_ms(10);
    }
    CONNS.lock()[id] = None;
    None
}

pub fn send(id: usize, data: &[u8]) -> bool {
    let me = task::current_pid();
    let mut cs = CONNS.lock();
    let Some(c) = cs.get_mut(id).and_then(|c| c.as_mut()) else { return false };
    if c.owner != me || c.state != St::Open {
        return false;
    }
    for chunk in data.chunks(1400) {
        send_tracked(c, PSH | ACK, chunk.to_vec());
    }
    true
}

/// Maʼlumot öqiydi (eng köpi `timeout_ms` kutadi). Böş — ulaniş yopildi.
pub fn recv(id: usize, max: usize, timeout_ms: u64) -> Vec<u8> {
    let me = task::current_pid();
    let start = crate::timer::uptime_ms();
    loop {
        {
            let mut cs = CONNS.lock();
            if let Some(c) = cs.get_mut(id).and_then(|c| c.as_mut()).filter(|c| c.owner == me) {
                if !c.inbox.is_empty() {
                    let n = max.min(c.inbox.len());
                    return c.inbox.drain(..n).collect();
                }
                if c.state == St::Closed {
                    return Vec::new();
                }
            } else {
                return Vec::new();
            }
        }
        if crate::timer::uptime_ms() - start >= timeout_ms {
            return Vec::new();
        }
        // wait_event da vaqt çeklovi yöq: paket kelmasa, hech kim uyğotmaydi va
        // `timeout_ms` tekşirilmay qoladi. Şuning uçun qisqa uyqu bilan kutamiz.
        task::sleep_ms(3);
    }
}

pub fn close(id: usize) {
    let me = task::current_pid();
    let mut cs = CONNS.lock();
    if let Some(c) = cs.get_mut(id).and_then(|c| c.as_mut()) {
        if c.owner != me {
            return; // boşqaning ulanişini yopa olmaymiz
        }
        if c.state == St::Open {
            send_segment(c, FIN | ACK, &[]);
        }
        cs[id] = None;
    }
}

/// Jarayon tugaganda uning barça ulanişlarini yopadi.
pub fn close_owned(pid: usize) {
    let mut cs = CONNS.lock();
    for slot in cs.iter_mut() {
        if let Some(c) = slot.as_mut().filter(|c| c.owner == pid) {
            if c.state == St::Open {
                send_segment(c, FIN | ACK, &[]);
            }
            *slot = None;
        }
    }
}

/// Kelgan TCP segmenti. Uyğotiş (wake_event) CONNS qulfi böşatilgandan KEYIN:
/// rejalaştiruvçi qulfi (SCHED) → CONNS tartibi bilan to'qnaşib, ABBA tiqilishi bölmasin.
pub fn handle(src: Ip, p: &[u8]) {
    if handle_locked(src, p) {
        task::wake_event(EVENT);
    }
}

fn handle_locked(src: Ip, p: &[u8]) -> bool {
    if p.len() < 20 {
        return false;
    }
    let src_port = u16::from_be_bytes([p[0], p[1]]);
    let dst_port = u16::from_be_bytes([p[2], p[3]]);
    let seg_seq = u32::from_be_bytes([p[4], p[5], p[6], p[7]]);
    let flags = p[13];
    let data_off = (p[12] >> 4) as usize * 4;
    let payload = if data_off <= p.len() { &p[data_off..] } else { &[] };

    let mut cs = CONNS.lock();
    let Some(c) = cs.iter_mut().flatten().find(|c| c.local_port == dst_port && c.remote == src && c.remote_port == src_port) else {
        return false;
    };
    if flags & RST != 0 {
        c.state = St::Closed;
        return true;
    }
    let seg_ack = u32::from_be_bytes([p[8], p[9], p[10], p[11]]);
    if flags & ACK != 0 {
        acknowledge(c, seg_ack);
    }
    if c.state == St::SynSent && flags & SYN != 0 && flags & ACK != 0 {
        c.ack = seg_seq.wrapping_add(1);
        c.got_synack = true;
        return true;
    }
    if c.state == St::Open {
        if !payload.is_empty() {
            c.ack = c.ack.wrapping_add(payload.len() as u32);
            for &b in payload {
                if c.inbox.len() < 64 * 1024 {
                    c.inbox.push_back(b);
                }
            }
            send_segment(c, ACK, &[]); // tasdiq
        }
        if flags & FIN != 0 {
            c.ack = c.ack.wrapping_add(1);
            send_segment(c, ACK, &[]);
            c.state = St::Closed;
        }
        return true;
    }
    false
}
