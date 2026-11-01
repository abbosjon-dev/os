//! Zamin tarmoq stegi (TCP/IP) — noldan yozilgan.
//!
//! Qatlamlar: Ethernet → ARP / IPv4 → ICMP, UDP, TCP. Yuqorida DHCP, DNS.
//! Bitta "tarmoq oqimi" kelgan kadrlarni qabul qilib, qatlamlarga tarqatadi.
//! Jönatiş istalgan oqimdan (drayver sinxron jönatadi).

pub mod dhcp;
pub mod dns;
pub mod tcp;
pub mod udp;

use crate::drivers::virtio_net as nic;
use crate::sync::SpinLock;
use crate::task;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

pub type Ip = [u8; 4];
pub type Mac = [u8; 6];

const ETH_ARP: u16 = 0x0806;
const ETH_IPV4: u16 = 0x0800;
pub const IP_ICMP: u8 = 1;
pub const IP_TCP: u8 = 6;
pub const IP_UDP: u8 = 17;

const EVENT: usize = 0x6e65_7400; // "net"

// --- Tarmoq sozlamalari ---
static IP: SpinLock<Ip> = SpinLock::new([0, 0, 0, 0]);
static GATEWAY: SpinLock<Ip> = SpinLock::new([0, 0, 0, 0]);
static NETMASK: SpinLock<Ip> = SpinLock::new([255, 255, 255, 0]);
static DNS: SpinLock<Ip> = SpinLock::new([0, 0, 0, 0]);
static CONFIGURED: AtomicUsize = AtomicUsize::new(0);

pub fn our_ip() -> Ip {
    *IP.lock()
}
pub fn gateway() -> Ip {
    *GATEWAY.lock()
}
pub fn dns_server() -> Ip {
    *DNS.lock()
}
pub fn our_mac() -> Mac {
    nic::mac()
}
pub fn is_configured() -> bool {
    CONFIGURED.load(Ordering::Acquire) != 0
}
#[allow(dead_code)]
pub fn present() -> bool {
    nic::present()
}

pub fn set_config(ip: Ip, gw: Ip, mask: Ip, dns: Ip) {
    *IP.lock() = ip;
    *GATEWAY.lock() = gw;
    *NETMASK.lock() = mask;
    if dns != [0, 0, 0, 0] {
        *DNS.lock() = dns;
    }
    CONFIGURED.store(1, Ordering::Release);
}

/// Internetdagi (yoki mahalliy) manzil uçun keyingi qadam IP si (gateway yoki özi).
fn next_hop(dst: Ip) -> Ip {
    let (ip, mask, gw) = (our_ip(), *NETMASK.lock(), gateway());
    let same = (0..4).all(|i| ip[i] & mask[i] == dst[i] & mask[i]);
    if same { dst } else { gw }
}

// --- Checksum (IPv4/ICMP/TCP/UDP) ---
pub fn checksum(parts: &[&[u8]]) -> u16 {
    let mut sum: u32 = 0;
    let mut leftover: Option<u8> = None;
    for part in parts {
        let mut data = *part;
        if let Some(hi) = leftover.take() {
            if let Some((&lo, rest)) = data.split_first() {
                sum += u16::from_be_bytes([hi, lo]) as u32;
                data = rest;
            } else {
                leftover = Some(hi);
            }
        }
        let mut i = 0;
        while i + 1 < data.len() {
            sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
            i += 2;
        }
        if i < data.len() {
            leftover = Some(data[i]);
        }
    }
    if let Some(hi) = leftover {
        sum += u16::from_be_bytes([hi, 0]) as u32;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

// --- Ethernet ---
fn send_eth(dst: Mac, ethertype: u16, payload: &[u8]) {
    let mut f = Vec::with_capacity(14 + payload.len());
    f.extend_from_slice(&dst);
    f.extend_from_slice(&our_mac());
    f.extend_from_slice(&ethertype.to_be_bytes());
    f.extend_from_slice(payload);
    nic::send(&f);
}

// --- ARP ---
const ARP_MAX: usize = 32;
/// (IP, MAC, qöşilgan vaqt ms). Eski yozuvlar eskiradi va qayta söraladi.
static ARP_CACHE: SpinLock<Vec<(Ip, Mac, u64)>> = SpinLock::new(Vec::new());
const ARP_TTL_MS: u64 = 60_000;

fn arp_lookup(ip: Ip) -> Option<Mac> {
    let now = crate::timer::uptime_ms();
    ARP_CACHE.lock().iter().find(|(i, _, t)| *i == ip && now.saturating_sub(*t) < ARP_TTL_MS).map(|(_, m, _)| *m)
}

fn arp_insert(ip: Ip, mac: Mac) {
    let now = crate::timer::uptime_ms();
    let mut c = ARP_CACHE.lock();
    if let Some(e) = c.iter_mut().find(|(i, _, _)| *i == ip) {
        e.1 = mac;
        e.2 = now;
    } else {
        // Böş joy yoki eng eski yozuv örniga
        if c.len() >= ARP_MAX {
            if let Some(oldest) = (0..c.len()).min_by_key(|&k| c[k].2) {
                c.remove(oldest);
            }
        }
        c.push((ip, mac, now));
    }
}

fn send_arp(op: u16, target_ip: Ip, target_mac: Mac) {
    let mut p = Vec::with_capacity(28);
    p.extend_from_slice(&[0, 1]); // Ethernet
    p.extend_from_slice(&ETH_IPV4.to_be_bytes());
    p.extend_from_slice(&[6, 4]);
    p.extend_from_slice(&op.to_be_bytes());
    p.extend_from_slice(&our_mac());
    p.extend_from_slice(&our_ip());
    p.extend_from_slice(&target_mac);
    p.extend_from_slice(&target_ip);
    let dst = if op == 1 { [0xff; 6] } else { target_mac };
    send_eth(dst, ETH_ARP, &p);
}

/// IP uçun MAC ni topadi (keşda bölmasa — ARP söraydi va kutadi).
fn resolve(ip: Ip) -> Option<Mac> {
    let hop = next_hop(ip);
    if let Some(m) = arp_lookup(hop) {
        return Some(m);
    }
    for _ in 0..20 {
        send_arp(1, hop, [0; 6]);
        task::sleep_ms(50);
        if let Some(m) = arp_lookup(hop) {
            return Some(m);
        }
    }
    None
}

fn handle_arp(p: &[u8]) {
    if p.len() < 28 {
        return;
    }
    let op = u16::from_be_bytes([p[6], p[7]]);
    let sender_mac: Mac = p[8..14].try_into().unwrap();
    let sender_ip: Ip = p[14..18].try_into().unwrap();
    let target_ip: Ip = p[24..28].try_into().unwrap();
    arp_insert(sender_ip, sender_mac);
    if op == 1 && is_configured() && target_ip == our_ip() {
        send_arp(2, sender_ip, sender_mac); // bizga sörov — javob beramiz
    }
}

// --- IPv4 ---
static IP_ID: AtomicU32 = AtomicU32::new(1);

pub fn send_ipv4(dst: Ip, proto: u8, payload: &[u8]) -> bool {
    let mac = if dst == [255, 255, 255, 255] {
        [0xff; 6] // broadcast — ARP kerak emas
    } else {
        match resolve(dst) {
            Some(m) => m,
            None => return false,
        }
    };
    let total = 20 + payload.len();
    let id = IP_ID.fetch_add(1, Ordering::Relaxed) as u16;
    let mut h = Vec::with_capacity(total);
    h.extend_from_slice(&[0x45, 0x00]);
    h.extend_from_slice(&(total as u16).to_be_bytes());
    h.extend_from_slice(&id.to_be_bytes());
    h.extend_from_slice(&[0x40, 0x00]); // Don't fragment
    h.extend_from_slice(&[64, proto]);
    h.extend_from_slice(&[0, 0]); // checksum keyin
    h.extend_from_slice(&our_ip());
    h.extend_from_slice(&dst);
    let ck = checksum(&[&h]);
    h[10..12].copy_from_slice(&ck.to_be_bytes());
    h.extend_from_slice(payload);
    send_eth(mac, ETH_IPV4, &h);
    true
}

fn handle_ipv4(p: &[u8]) {
    if p.len() < 20 || p[0] >> 4 != 4 {
        return;
    }
    let ihl = (p[0] & 0xf) as usize * 4;
    let proto = p[9];
    let src: Ip = p[12..16].try_into().unwrap();
    let dst: Ip = p[16..20].try_into().unwrap();
    let total = u16::from_be_bytes([p[2], p[3]]) as usize;
    if total > p.len() || ihl > total {
        return;
    }
    // Bizga yoki translyatsiya qilingan manzilga (DHCP tugagunça — hammasi)
    if is_configured() && dst != our_ip() && dst != [255, 255, 255, 255] {
        return;
    }
    let payload = &p[ihl..total];
    match proto {
        IP_ICMP => handle_icmp(src, payload),
        IP_UDP => udp::handle(src, payload),
        IP_TCP => tcp::handle(src, payload),
        _ => {}
    }
}

// --- ICMP (ping) ---
static PING_REPLY: SpinLock<Option<(Ip, u16, u16)>> = SpinLock::new(None); // (manba, id, seq)

fn handle_icmp(src: Ip, p: &[u8]) {
    if p.len() < 8 {
        return;
    }
    match p[0] {
        8 => {
            // Echo sörov → javob (type 0)
            let mut reply = p.to_vec();
            reply[0] = 0;
            reply[2] = 0;
            reply[3] = 0;
            let ck = checksum(&[&reply]);
            reply[2..4].copy_from_slice(&ck.to_be_bytes());
            send_ipv4(src, IP_ICMP, &reply);
        }
        0 => {
            let id = u16::from_be_bytes([p[4], p[5]]);
            let seq = u16::from_be_bytes([p[6], p[7]]);
            *PING_REPLY.lock() = Some((src, id, seq));
            task::wake_event(EVENT);
        }
        _ => {}
    }
}

/// `dst` ga ping yuboradi. Natija: javob vaqti (ms) yoki `None` (javob yöq).
pub fn ping(dst: Ip, seq: u16) -> Option<u64> {
    let id = 0x5a00 | (seq & 0xff);
    let mut pkt = Vec::with_capacity(16);
    pkt.extend_from_slice(&[8, 0, 0, 0]);
    pkt.extend_from_slice(&id.to_be_bytes());
    pkt.extend_from_slice(&seq.to_be_bytes());
    pkt.extend_from_slice(b"zamin-os-ping-00");
    let ck = checksum(&[&pkt]);
    pkt[2..4].copy_from_slice(&ck.to_be_bytes());
    *PING_REPLY.lock() = None;
    let start = crate::timer::uptime_ms();
    if !send_ipv4(dst, IP_ICMP, &pkt) {
        return None;
    }
    while crate::timer::uptime_ms() - start < 1000 {
        if let Some((s, rid, rseq)) = *PING_REPLY.lock() {
            if s == dst && rid == id && rseq == seq {
                return Some(crate::timer::uptime_ms() - start);
            }
        }
        task::sleep_ms(5);
    }
    None
}

// --- Tarmoq oqimi ---
pub fn wake() {
    task::wake_event(EVENT);
}

fn dispatch(frame: &[u8]) {
    if frame.len() < 14 {
        return;
    }
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    let payload = &frame[14..];
    match ethertype {
        ETH_ARP => handle_arp(payload),
        ETH_IPV4 => handle_ipv4(payload),
        _ => {}
    }
}

extern "C" fn worker(_: usize) {
    loop {
        // Uziliş orqali kelganlar
        while let Some(frame) = nic::recv() {
            dispatch(&frame);
        }
        // Uzilişga tayanmay, navbatni özimiz ham tekşiramiz (işonçli)
        for frame in nic::poll() {
            dispatch(&frame);
        }
        tcp::tick(); // vaqtida tasdiq kelmagan TCP segmentlarini qayta jönatiş
        task::sleep_ms(2);
    }
}

/// Tarmoqni işga tuşiradi: oqim + DHCP (fon).
pub fn init() {
    if !nic::present() {
        return;
    }
    task::spawn_kernel("tarmoq", worker, 0).expect("tarmoq oqimi");
}
