//! DHCP mijozi: tarmoqdan IP, gateway va DNS oladi (DISCOVER→OFFER→REQUEST→ACK).

use super::{Ip, udp};
use crate::task;
use alloc::vec;
use alloc::vec::Vec;

const SERVER_PORT: u16 = 67;
const CLIENT_PORT: u16 = 68;
const MAGIC: [u8; 4] = [99, 130, 83, 99];

fn build(xid: u32, msg_type: u8, requested: Option<Ip>, server: Option<Ip>) -> Vec<u8> {
    let mut p = vec![0u8; 240];
    p[0] = 1; // BOOTREQUEST
    p[1] = 1; // Ethernet
    p[2] = 6; // MAC uzunligi
    p[4..8].copy_from_slice(&xid.to_be_bytes());
    p[10] = 0x80; // broadcast bayroği
    p[28..34].copy_from_slice(&super::our_mac());
    p[236..240].copy_from_slice(&MAGIC);
    p.extend_from_slice(&[53, 1, msg_type]); // DHCP message type
    if let Some(ip) = requested {
        p.extend_from_slice(&[50, 4]);
        p.extend_from_slice(&ip);
    }
    if let Some(s) = server {
        p.extend_from_slice(&[54, 4]);
        p.extend_from_slice(&s);
    }
    // Sörov: netmask, router, DNS
    p.extend_from_slice(&[55, 3, 1, 3, 6]);
    p.push(255); // oxiri
    p
}

struct Offer {
    ip: Ip,
    server: Ip,
    gateway: Ip,
    mask: Ip,
    dns: Ip,
}

fn parse(p: &[u8], xid: u32) -> Option<(u8, Offer)> {
    if p.len() < 240 || p[236..240] != MAGIC || u32::from_be_bytes(p[4..8].try_into().ok()?) != xid {
        return None;
    }
    let yiaddr: Ip = p[16..20].try_into().ok()?;
    let mut o = Offer { ip: yiaddr, server: [0; 4], gateway: [0; 4], mask: [255, 255, 255, 0], dns: [0; 4] };
    let mut msg_type = 0;
    let mut i = 240;
    while i + 2 <= p.len() {
        let (opt, len) = (p[i], p[i + 1] as usize);
        if opt == 255 {
            break;
        }
        let val = p.get(i + 2..i + 2 + len)?;
        match opt {
            53 => msg_type = val[0],
            54 if len >= 4 => o.server = val[..4].try_into().unwrap(),
            1 if len >= 4 => o.mask = val[..4].try_into().unwrap(),
            3 if len >= 4 => o.gateway = val[..4].try_into().unwrap(),
            6 if len >= 4 => o.dns = val[..4].try_into().unwrap(),
            _ => {}
        }
        i += 2 + len;
    }
    Some((msg_type, o))
}

/// Töliq DHCP almaşinuvi. Natija: muvaffaqiyatlimi.
pub fn configure() -> bool {
    udp::bind(CLIENT_PORT);
    let xid = 0x5a_00_00_00 | (crate::timer::uptime_ms() as u32 & 0xffffff);
    let bcast = [255, 255, 255, 255];

    for _ in 0..4 {
        udp::send(bcast, CLIENT_PORT, SERVER_PORT, &build(xid, 1, None, None)); // DISCOVER
        let Some((_, _, data)) = udp::recv_wait(CLIENT_PORT, 1000) else { continue };
        let Some((53 | 2, offer)) = parse(&data, xid) else { continue };
        if offer.ip == [0, 0, 0, 0] {
            continue;
        }
        // REQUEST
        udp::send(bcast, CLIENT_PORT, SERVER_PORT, &build(xid, 3, Some(offer.ip), Some(offer.server)));
        let deadline = crate::timer::uptime_ms() + 2000;
        while crate::timer::uptime_ms() < deadline {
            let Some((_, _, data)) = udp::recv_wait(CLIENT_PORT, 1000) else { break };
            if let Some((5, ack)) = parse(&data, xid) {
                super::set_config(ack.ip, ack.gateway, ack.mask, ack.dns);
                udp::unbind(CLIENT_PORT);
                return true;
            }
        }
    }
    udp::unbind(CLIENT_PORT);
    false
}

/// Fon oqimi: yuklanişda DHCP ni sinaydi.
pub extern "C" fn worker(_: usize) {
    task::sleep_ms(100); // tarmoq oqimi tayyor bölsin
    if configure() {
        let ip = super::our_ip();
        crate::tprintln!(crate::i18n::msg::NET_READY, format_ip(ip), format_ip(super::gateway()), format_ip(super::dns_server()));
    } else {
        crate::tprintln!(crate::i18n::msg::NET_NO_DHCP);
    }
}

pub fn format_ip(ip: Ip) -> alloc::string::String {
    alloc::format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}
