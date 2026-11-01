//! UDP: datagrammalar, portlar böyiça tarqatiş.

use super::{Ip, checksum};
use crate::sync::SpinLock;
use crate::task;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

const EVENT_BASE: usize = 0x7564_7000; // "udp"

struct Bound {
    port: u16,
    inbox: VecDeque<(Ip, u16, Vec<u8>)>, // (manba IP, manba port, maʼlumot)
}

static PORTS: SpinLock<Vec<Bound>> = SpinLock::new(Vec::new());

fn event(port: u16) -> usize {
    EVENT_BASE + port as usize
}

/// Portni oçadi (kelgan datagrammalar şu yerga yiğiladi).
pub fn bind(port: u16) {
    let mut p = PORTS.lock();
    if !p.iter().any(|b| b.port == port) {
        p.push(Bound { port, inbox: VecDeque::new() });
    }
}

pub fn unbind(port: u16) {
    PORTS.lock().retain(|b| b.port != port);
}

/// Datagramma jönatadi.
pub fn send(dst: Ip, src_port: u16, dst_port: u16, data: &[u8]) -> bool {
    let len = 8 + data.len();
    let mut h = Vec::with_capacity(len);
    h.extend_from_slice(&src_port.to_be_bytes());
    h.extend_from_slice(&dst_port.to_be_bytes());
    h.extend_from_slice(&(len as u16).to_be_bytes());
    h.extend_from_slice(&[0, 0]); // checksum (ixtiyoriy; soddalik uçun 0)
    h.extend_from_slice(data);
    // UDP checksum (psevdo-sarlavha bilan)
    let src = super::our_ip();
    let pseudo = [src[0], src[1], src[2], src[3], dst[0], dst[1], dst[2], dst[3], 0, super::IP_UDP, (len >> 8) as u8, len as u8];
    let ck = checksum(&[&pseudo, &h]);
    let ck = if ck == 0 { 0xffff } else { ck };
    h[6..8].copy_from_slice(&ck.to_be_bytes());
    super::send_ipv4(dst, super::IP_UDP, &h)
}

/// Portdan kelgan navbatdagi datagramma (bölmasa — None).
pub fn recv(port: u16) -> Option<(Ip, u16, Vec<u8>)> {
    let mut p = PORTS.lock();
    p.iter_mut().find(|b| b.port == port)?.inbox.pop_front()
}

/// Datagramma kelgunça kutadi (eng köpi `timeout_ms`).
pub fn recv_wait(port: u16, timeout_ms: u64) -> Option<(Ip, u16, Vec<u8>)> {
    let start = crate::timer::uptime_ms();
    loop {
        if let Some(d) = recv(port) {
            return Some(d);
        }
        if crate::timer::uptime_ms() - start >= timeout_ms {
            return None;
        }
        let have = || PORTS.lock().iter().any(|b| b.port == port && !b.inbox.is_empty());
        task::wait_event(event(port), have);
        if crate::timer::uptime_ms() - start >= timeout_ms && !have() {
            return None;
        }
    }
}

pub fn handle(src: Ip, p: &[u8]) {
    if p.len() < 8 {
        return;
    }
    let src_port = u16::from_be_bytes([p[0], p[1]]);
    let dst_port = u16::from_be_bytes([p[2], p[3]]);
    let len = u16::from_be_bytes([p[4], p[5]]) as usize;
    if len < 8 || len > p.len() {
        return;
    }
    let data = p[8..len].to_vec();
    let mut ports = PORTS.lock();
    if let Some(b) = ports.iter_mut().find(|b| b.port == dst_port) {
        if b.inbox.len() < 64 {
            b.inbox.push_back((src, src_port, data));
        }
        drop(ports);
        task::wake_event(event(dst_port));
    }
}
