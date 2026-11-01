//! Tarmoq: tarmoq holati, DNS, ping, TCP.

use crate::abi::*;
use crate::{Result, String, Vec, check, syscall};

pub type Ip = [u8; 4];

pub struct Info {
    pub ip: Ip,
    pub gateway: Ip,
    pub dns: Ip,
    pub mac: [u8; 6],
    pub configured: bool,
}

pub fn info() -> Info {
    let mut b = [0u8; 23];
    let _ = unsafe { syscall(SYS_NET_INFO, b.as_mut_ptr() as u64, 0, 0, 0) };
    Info {
        ip: b[0..4].try_into().unwrap(),
        gateway: b[4..8].try_into().unwrap(),
        dns: b[12..16].try_into().unwrap(),
        mac: b[16..22].try_into().unwrap(),
        configured: b[22] != 0,
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
