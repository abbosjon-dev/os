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

struct Conn {
    local_port: u16,
    remote: Ip,
    remote_port: u16,
    seq: u32,
    ack: u32,
    state: St,
    inbox: VecDeque<u8>,
    got_synack: bool,
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
    let local_port = NEXT_PORT.fetch_add(1, core::sync::atomic::Ordering::Relaxed).max(49152);
    let seq = (crate::timer::uptime_ms() as u32).wrapping_mul(2654435761);
    let conn = Conn {
        local_port,
        remote,
        remote_port,
        seq,
        ack: 0,
        state: St::SynSent,
        inbox: VecDeque::new(),
        got_synack: false,
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
        send_segment(cs[id].as_ref().unwrap(), SYN, &[]);
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
                    send_segment(c, ACK, &[]);
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
    let mut cs = CONNS.lock();
    let Some(c) = cs.get_mut(id).and_then(|c| c.as_mut()) else { return false };
    if c.state != St::Open {
        return false;
    }
    for chunk in data.chunks(1400) {
        send_segment(c, PSH | ACK, chunk);
        c.seq = c.seq.wrapping_add(chunk.len() as u32);
    }
    true
}

/// Maʼlumot öqiydi (eng köpi `timeout_ms` kutadi). Böş — ulaniş yopildi.
pub fn recv(id: usize, max: usize, timeout_ms: u64) -> Vec<u8> {
    let start = crate::timer::uptime_ms();
    loop {
        {
            let mut cs = CONNS.lock();
            if let Some(c) = cs.get_mut(id).and_then(|c| c.as_mut()) {
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
        task::wait_event(EVENT, || {
            CONNS.lock().get(id).and_then(|c| c.as_ref()).is_some_and(|c| !c.inbox.is_empty() || c.state == St::Closed)
        });
    }
}

pub fn close(id: usize) {
    let mut cs = CONNS.lock();
    if let Some(c) = cs.get_mut(id).and_then(|c| c.as_mut()) {
        if c.state == St::Open {
            send_segment(c, FIN | ACK, &[]);
        }
        c.state = St::Closed;
    }
    if id < cs.len() {
        cs[id] = None;
    }
}

pub fn handle(src: Ip, p: &[u8]) {
    if p.len() < 20 {
        return;
    }
    let src_port = u16::from_be_bytes([p[0], p[1]]);
    let dst_port = u16::from_be_bytes([p[2], p[3]]);
    let seg_seq = u32::from_be_bytes([p[4], p[5], p[6], p[7]]);
    let flags = p[13];
    let data_off = (p[12] >> 4) as usize * 4;
    let payload = if data_off <= p.len() { &p[data_off..] } else { &[] };

    let mut cs = CONNS.lock();
    let Some(c) = cs.iter_mut().flatten().find(|c| c.local_port == dst_port && c.remote == src && c.remote_port == src_port) else {
        return;
    };
    if flags & RST != 0 {
        c.state = St::Closed;
        task::wake_event(EVENT);
        return;
    }
    if c.state == St::SynSent && flags & SYN != 0 && flags & ACK != 0 {
        c.seq = c.seq.wrapping_add(1);
        c.ack = seg_seq.wrapping_add(1);
        c.got_synack = true;
        task::wake_event(EVENT);
        return;
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
        task::wake_event(EVENT);
    }
}
