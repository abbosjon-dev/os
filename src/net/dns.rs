//! DNS mijozi: domen nomidan IPv4 (A yozuvi) ni topadi.

use super::{Ip, udp};
use alloc::vec::Vec;

/// `host` (masalan "zamin.app") uçun IP. DNS server sozlanmagan bölsa — None.
pub fn resolve(host: &str) -> Option<Ip> {
    // Allaqaçon IP bölsa — özini qaytaramiz
    if let Some(ip) = parse_ip(host) {
        return Some(ip);
    }
    let server = super::dns_server();
    if server == [0, 0, 0, 0] {
        return None;
    }
    let port = 40000 + (crate::timer::uptime_ms() as u16 % 20000);
    udp::bind(port);
    let id = crate::timer::uptime_ms() as u16 | 1;

    let mut q = Vec::new();
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]); // RD=1, 1 sörov
    for label in host.split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    q.extend_from_slice(&[0, 1, 0, 1]); // A, IN

    let mut result = None;
    for _ in 0..3 {
        udp::send(server, port, 53, &q);
        let Some((_, _, resp)) = udp::recv_wait(port, 1500) else { continue };
        result = parse_response(&resp, id);
        if result.is_some() {
            break;
        }
    }
    udp::unbind(port);
    result
}

fn parse_ip(s: &str) -> Option<Ip> {
    let mut it = s.split('.');
    let mut ip = [0u8; 4];
    for b in ip.iter_mut() {
        *b = it.next()?.parse().ok()?;
    }
    if it.next().is_some() { None } else { Some(ip) }
}

fn parse_response(p: &[u8], id: u16) -> Option<Ip> {
    if p.len() < 12 || u16::from_be_bytes([p[0], p[1]]) != id || p[3] & 0x0f != 0 {
        return None;
    }
    let questions = u16::from_be_bytes([p[4], p[5]]);
    let answers = u16::from_be_bytes([p[6], p[7]]);
    let mut i = 12;
    // Sörov(lar)ni ötkazib yuboramiz
    for _ in 0..questions {
        i = skip_name(p, i)?;
        i += 4; // tur + sinf
    }
    for _ in 0..answers {
        i = skip_name(p, i)?;
        let rtype = u16::from_be_bytes([*p.get(i)?, *p.get(i + 1)?]);
        let rdlen = u16::from_be_bytes([*p.get(i + 8)?, *p.get(i + 9)?]) as usize;
        let rdata = i + 10;
        if rtype == 1 && rdlen == 4 {
            return Some(p.get(rdata..rdata + 4)?.try_into().ok()?);
        }
        i = rdata + rdlen;
    }
    None
}

fn skip_name(p: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *p.get(i)?;
        if len & 0xc0 == 0xc0 {
            return Some(i + 2); // körsatkiç (pointer)
        }
        if len == 0 {
            return Some(i + 1);
        }
        i += 1 + len as usize;
    }
}
