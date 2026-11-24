//! Tarmoq: tarmoq holati, DNS, ping, TCP.

use crate::abi::*;
use crate::{Result, String, Vec, check, syscall};

pub type Ip = [u8; 4];

pub struct Info {
    pub ip: Ip,
    pub gateway: Ip,
    pub mask: Ip,
    pub dns: Ip,
    pub mac: [u8; 6],
    pub configured: bool,
    /// Statik (qölda) rejim tanlanganmi (yöq — DHCP).
    pub static_mode: bool,
}

pub fn info() -> Info {
    let mut b = [0u8; 24];
    let _ = unsafe { syscall(SYS_NET_INFO, b.as_mut_ptr() as u64, 0, 0, 0) };
    Info {
        ip: b[0..4].try_into().unwrap(),
        gateway: b[4..8].try_into().unwrap(),
        mask: b[8..12].try_into().unwrap(),
        dns: b[12..16].try_into().unwrap(),
        mac: b[16..22].try_into().unwrap(),
        configured: b[22] != 0,
        static_mode: b[23] != 0,
    }
}

pub fn ip_string(ip: Ip) -> String {
    crate::format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

/// Domen nomidan IP (yoki IP satrini özini).
pub fn resolve(host: &str) -> Result<Ip> {
    let mut ip = [0u8; 4];
    check(unsafe { syscall(SYS_DNS, host.as_ptr() as u64, host.len() as u64, ip.as_mut_ptr() as u64, 0) })?;
    Ok(ip)
}

/// Ping. Natija: javob vaqti (ms).
pub fn ping(ip: Ip, seq: u16) -> Result<u64> {
    check(unsafe { syscall(SYS_PING, ip.as_ptr() as u64, seq as u64, 0, 0) }).map(|ms| ms)
}

pub type Ip6 = [u8; 16];

/// ICMPv6 echo (ping). Natija: javob vaqti (ms).
pub fn ping6(ip: &Ip6, seq: u16) -> Result<u64> {
    check(unsafe { syscall(SYS_PING, ip.as_ptr() as u64, seq as u64, 6, 0) })
}

/// "fec0::2", "2a03:4000::1", "[fe80::1]" → 16 bayt (IPv4 qöşimçasisiz).
pub fn parse_ipv6(s: &str) -> Option<Ip6> {
    let s = s.trim();
    let s = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')).unwrap_or(s);
    let s = s.split('%').next()?;
    fn groups(part: &str) -> Option<Vec<u16>> {
        if part.is_empty() {
            return Some(Vec::new());
        }
        part.split(':')
            .map(|g| if g.is_empty() || g.len() > 4 || !g.bytes().all(|b| b.is_ascii_hexdigit()) { None } else { u16::from_str_radix(g, 16).ok() })
            .collect()
    }
    let mut w = [0u16; 8];
    match s.split_once("::") {
        Some((a, b)) => {
            let (ga, gb) = (groups(a)?, groups(b)?);
            if b.contains("::") || ga.len() + gb.len() > 7 {
                return None;
            }
            w[..ga.len()].copy_from_slice(&ga);
            w[8 - gb.len()..].copy_from_slice(&gb);
        }
        None => {
            let g = groups(s)?;
            if g.len() != 8 {
                return None;
            }
            w.copy_from_slice(&g);
        }
    }
    let mut out = [0u8; 16];
    for (i, v) in w.iter().enumerate() {
        out[2 * i..2 * i + 2].copy_from_slice(&v.to_be_bytes());
    }
    Some(out)
}

/// IPv6 matni (RFC 5952: eng uzun nol guruhlar "::").
pub fn ip6_string(a: &Ip6) -> String {
    let w: [u16; 8] = core::array::from_fn(|i| u16::from_be_bytes([a[2 * i], a[2 * i + 1]]));
    let (mut best, mut blen, mut i) = (8, 0, 0);
    while i < 8 {
        let s = i;
        while i < 8 && w[i] == 0 {
            i += 1;
        }
        if i - s > blen && i - s >= 2 {
            best = s;
            blen = i - s;
        }
        if i == s {
            i += 1;
        }
    }
    let mut out = String::new();
    let mut i = 0;
    while i < 8 {
        if i == best {
            out.push_str("::");
            i += blen;
            continue;
        }
        if !out.is_empty() && !out.ends_with(':') {
            out.push(':');
        }
        out += &crate::format!("{:x}", w[i]);
        i += 1;
    }
    out
}

/// TCP ulanişi.
pub struct Tcp(u64);

impl Tcp {
    pub fn connect(ip: Ip, port: u16) -> Result<Tcp> {
        check(unsafe { syscall(SYS_TCP_CONNECT, ip.as_ptr() as u64, port as u64, 0, 0) }).map(Tcp)
    }

    pub fn send(&self, data: &[u8]) -> Result<usize> {
        check(unsafe { syscall(SYS_TCP_SEND, self.0, data.as_ptr() as u64, data.len() as u64, 0) }).map(|n| n as usize)
    }

    /// Maʼlumot öqiydi (eng köpi `timeout_ms`). Böş — ulaniş yopildi.
    pub fn recv(&self, max: usize, timeout_ms: u64) -> Vec<u8> {
        let mut buf = crate::vec![0u8; max];
        let n = unsafe { syscall(SYS_TCP_RECV, self.0, buf.as_mut_ptr() as u64, max as u64, timeout_ms) };
        buf.truncate(n.max(0) as usize);
        buf
    }
}

impl Drop for Tcp {
    fn drop(&mut self) {
        unsafe { syscall(SYS_TCP_CLOSE, self.0, 0, 0, 0) };
    }
}

/// TLS (HTTPS) ulanişi — şifrlangan. OGOHLANTIRIŞ: sertifikat tekşirilmaydi.
pub struct Tls(u64);

impl Tls {
    pub fn connect(ip: Ip, port: u16, hostname: &str) -> Result<Tls> {
        check(unsafe {
            syscall(SYS_TLS_CONNECT, ip.as_ptr() as u64, port as u64, hostname.as_ptr() as u64, hostname.len() as u64)
        })
        .map(Tls)
    }
    pub fn send(&self, data: &[u8]) -> Result<usize> {
        check(unsafe { syscall(SYS_TLS_SEND, self.0, data.as_ptr() as u64, data.len() as u64, 0) }).map(|n| n as usize)
    }
    pub fn recv(&self, max: usize) -> Vec<u8> {
        let mut buf = crate::vec![0u8; max];
        let n = unsafe { syscall(SYS_TLS_RECV, self.0, buf.as_mut_ptr() as u64, max as u64, 0) };
        buf.truncate(n.max(0) as usize);
        buf
    }
}

impl Drop for Tls {
    fn drop(&mut self) {
        unsafe { syscall(SYS_TLS_CLOSE, self.0, 0, 0, 0) };
    }
}
