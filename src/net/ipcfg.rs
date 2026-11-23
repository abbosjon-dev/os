//! Statik IPv4 sozlamalari: matnni tahlil qiliş, tekşiruv, diskdagi "kalit=qiymat" formati.
//!
//! Hammasi sof funksiyalar (global holatga tegmaydi) — self-test va GUI şularni işlatadi.

use super::Ip;
use alloc::format;
use alloc::string::String;

/// Qölda kiritilgan tarmoq sozlamalari.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StaticCfg {
    pub ip: Ip,
    pub mask: Ip,
    pub gw: Ip,
    pub dns: Ip,
}

/// "a.b.c.d" → oktetlar. Bölaklar: aniq 4 ta, har biri 1..3 raqam, 0..255,
/// boşida ortiqça nol yöq ("010" — rad etiladi, sakkizlik bilan adaşmaslik uçun).
pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let s = s.trim();
    let mut out = [0u8; 4];
    let mut it = s.split('.');
    for o in out.iter_mut() {
        let p = it.next()?;
        if p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit()) || (p.len() > 1 && p.starts_with('0')) {
            return None;
        }
        let v: u16 = p.parse().ok()?;
        if v > 255 {
            return None;
        }
        *o = v as u8;
    }
    if it.next().is_some() { None } else { Some(out) }
}

/// Prefiks uzunligidan (0..=32) niqob.
pub fn prefix_mask(n: u8) -> Ip {
    let m: u32 = if n == 0 { 0 } else { u32::MAX << (32 - n.min(32) as u32) };
    m.to_be_bytes()
}

/// Niqob uzluksiz bölsa (1 lar, keyin 0 lar) — prefiks uzunligi.
pub fn mask_prefix(mask: Ip) -> Option<u8> {
    let m = u32::from_be_bytes(mask);
    let n = m.leading_ones();
    if m.checked_shl(n).unwrap_or(0) != 0 { None } else { Some(n as u8) }
}

/// Niqob: "255.255.255.0", "/24" yoki "24".
pub fn parse_mask(s: &str) -> Option<Ip> {
    let s = s.trim();
    let p = s.strip_prefix('/').unwrap_or(s);
    if !p.is_empty() && p.len() <= 2 && p.bytes().all(|b| b.is_ascii_digit()) {
        let n: u8 = p.parse().ok()?;
        return if n <= 32 { Some(prefix_mask(n)) } else { None };
    }
    if s.starts_with('/') { None } else { parse_ipv4(s) }
}

/// "10.0.2.15/24" → (IP, Some(niqob)); "10.0.2.15" → (IP, None).
pub fn parse_cidr(s: &str) -> Option<(Ip, Option<Ip>)> {
    match s.trim().split_once('/') {
        Some((a, n)) => Some((parse_ipv4(a)?, Some(parse_mask(&format!("/{}", n))?))),
        None => Some((parse_ipv4(s)?, None)),
    }
}

fn is_unicast(ip: Ip) -> bool {
    ip != [0, 0, 0, 0] && ip[0] != 127 && ip[0] < 224 && ip[0] != 0
}

/// IP, niqob va şlyuzni tekşiradi. Şlyuz boşqa tarmoqda bölişi mumkin (VPS: /32 yoki
/// on-link bölmagan şlyuz) — unda ARP to'g'ridan-to'g'ri şlyuzga yuboriladi.
pub fn validate_static(ip: Ip, mask: Ip, gw: Ip) -> Result<(), &'static str> {
    let n = mask_prefix(mask).ok_or("Niqob uzluksiz emas (masalan 255.255.255.0 yoki /24)")?;
    if n == 0 {
        return Err("Niqob 0.0.0.0 bölişi mumkin emas");
    }
    if !is_unicast(ip) {
        return Err("IP manzil notöğri (0.0.0.0, 127.x, multicast emas)");
    }
    let (a, m) = (u32::from_be_bytes(ip), u32::from_be_bytes(mask));
    if n <= 30 {
        if a & !m == 0 {
            return Err("IP — tarmoq manzili (xost qismi 0)");
        }
        if a & !m == !m {
            return Err("IP — broadcast manzili");
        }
    }
    if !is_unicast(gw) {
        return Err("Şlyuz manzili notöğri");
    }
    if gw == ip {
        return Err("Şlyuz IP manzilning özi bölmasin");
    }
    let g = u32::from_be_bytes(gw);
    if n <= 30 && g & m == a & m && (g & !m == 0 || g & !m == !m) {
        return Err("Şlyuz — tarmoq yoki broadcast manzili");
    }
    Ok(())
}

/// Keyingi qadam: manzil bizning tarmoqda bölsa — özi, aks holda şlyuz.
/// Şlyuz on-link bölmasa ham (niqob /32 yoki boşqa tarmoq) — şlyuzning özi (ARP unga).
pub fn next_hop_for(ip: Ip, mask: Ip, gw: Ip, dst: Ip) -> Ip {
    let same = (0..4).all(|i| ip[i] & mask[i] == dst[i] & mask[i]);
    if same || gw == [0, 0, 0, 0] { dst } else { gw }
}

pub fn ip_str(ip: Ip) -> String {
    format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

/// Diskdagi qatorlar (config.rs sozlamalar matniga qöşiladi).
pub fn to_text(c: Option<&StaticCfg>) -> String {
    match c {
        None => String::from("tarmoq=dhcp\n"),
        Some(c) => format!(
            "tarmoq=statik\ntarmoq_ip={}\ntarmoq_niqob={}\ntarmoq_shlyuz={}\ntarmoq_dns={}\n",
            ip_str(c.ip),
            ip_str(c.mask),
            ip_str(c.gw),
            ip_str(c.dns)
        ),
    }
}

/// Sozlamalar matnidan (birinçi "[hisob]" gaça) tarmoq rejimi. Statik, lekin manzillar
/// notöğri/yöq bölsa — DHCP (tizim ulanmay qolmasin).
pub fn from_text(text: &str) -> Option<StaticCfg> {
    let (mut mode, mut ip, mut mask, mut gw, mut dns) = (false, None, None, None, None);
    for line in text.lines() {
        if line.trim() == "[hisob]" {
            break;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match k {
            "tarmoq" => mode = v == "statik",
            "tarmoq_ip" => ip = parse_ipv4(v),
            "tarmoq_niqob" => mask = parse_mask(v),
            "tarmoq_shlyuz" => gw = parse_ipv4(v),
            "tarmoq_dns" => dns = parse_ipv4(v),
            _ => {}
        }
    }
    let c = StaticCfg { ip: ip?, mask: mask?, gw: gw?, dns: dns? };
    (mode && validate_static(c.ip, c.mask, c.gw).is_ok() && is_unicast(c.dns)).then_some(c)
}

/// GUI maydonlaridan (IP, niqob, şlyuz, DNS) sozlama yasaydi; xato — tuşunarli matn.
/// IP maydonida "/24" bölsa va niqob maydoni böş bölsa — niqob şundan olinadi.
pub fn from_fields(ip: &str, mask: &str, gw: &str, dns: &str) -> Result<StaticCfg, &'static str> {
    let (ip, pfx) = parse_cidr(ip).ok_or("IP manzil notöğri (masalan 10.0.2.15)")?;
    let mask = if mask.trim().is_empty() {
        pfx.ok_or("Niqobni kiriting (masalan 255.255.255.0 yoki /24)")?
    } else {
        parse_mask(mask).ok_or("Niqob notöğri (masalan 255.255.255.0 yoki /24)")?
    };
    let gw = parse_ipv4(gw).ok_or("Şlyuz manzili notöğri (masalan 10.0.2.2)")?;
    let dns = parse_ipv4(dns).ok_or("DNS manzili notöğri (masalan 10.0.2.3)")?;
    validate_static(ip, mask, gw)?;
    if !is_unicast(dns) {
        return Err("DNS manzili notöğri");
    }
    Ok(StaticCfg { ip, mask, gw, dns })
}

/// Self-test: tahlil, tekşiruv, next_hop va disk formati aylanişi. Natija: sinovlar soni.
pub fn self_test() -> Result<usize, String> {
    let mut n = 0;
    let mut check = |ok: bool, what: &str| -> Result<(), String> {
        n += 1;
        if ok { Ok(()) } else { Err(format!("{}", what)) }
    };
    // parse_ipv4
    check(parse_ipv4("10.0.2.15") == Some([10, 0, 2, 15]), "parse 10.0.2.15")?;
    check(parse_ipv4(" 192.168.1.1 ") == Some([192, 168, 1, 1]), "parse boşliqlar")?;
    check(parse_ipv4("255.255.255.255") == Some([255; 4]), "parse 255")?;
    for bad in ["", "1.2.3", "1.2.3.4.5", "256.1.1.1", "1..2.3", "a.b.c.d", "01.2.3.4", "1.2.3.4/", "-1.2.3.4", "1.2.3.1000"] {
        check(parse_ipv4(bad).is_none(), &format!("parse rad etmadi: {:?}", bad))?;
    }
    // niqob va prefiks
    check(parse_mask("/24") == Some([255, 255, 255, 0]), "niqob /24")?;
    check(parse_mask("24") == Some([255, 255, 255, 0]), "niqob 24")?;
    check(parse_mask("/32") == Some([255; 4]), "niqob /32")?;
    check(parse_mask("255.255.240.0") == Some([255, 255, 240, 0]), "niqob nuqtali")?;
    check(parse_mask("/33").is_none() && parse_mask("/").is_none(), "niqob /33")?;
    check(mask_prefix([255, 255, 240, 0]) == Some(20) && mask_prefix([0; 4]) == Some(0), "prefiks")?;
    check(mask_prefix([255, 0, 255, 0]).is_none() && mask_prefix([255, 255, 255, 1]).is_none(), "uzluksiz emas")?;
    check(parse_cidr("10.0.2.15/24") == Some(([10, 0, 2, 15], Some([255, 255, 255, 0]))), "cidr")?;
    check(parse_cidr("10.0.2.15/40").is_none(), "cidr /40")?;
    // validate_static
    let m24 = [255, 255, 255, 0];
    check(validate_static([10, 0, 2, 15], m24, [10, 0, 2, 2]).is_ok(), "10.0.2.15/24 gw 10.0.2.2")?;
    check(validate_static([0, 0, 0, 0], m24, [10, 0, 2, 2]).is_err(), "0.0.0.0")?;
    check(validate_static([10, 0, 2, 0], m24, [10, 0, 2, 2]).is_err(), "tarmoq manzili")?;
    check(validate_static([10, 0, 2, 255], m24, [10, 0, 2, 2]).is_err(), "broadcast")?;
    check(validate_static([10, 0, 2, 15], [255, 0, 255, 0], [10, 0, 2, 2]).is_err(), "uzluksiz bölmagan niqob")?;
    check(validate_static([10, 0, 2, 15], [0; 4], [10, 0, 2, 2]).is_err(), "niqob 0")?;
    check(validate_static([10, 0, 2, 15], m24, [0, 0, 0, 0]).is_err(), "şlyuz 0")?;
    check(validate_static([10, 0, 2, 15], m24, [10, 0, 2, 15]).is_err(), "şlyuz = IP")?;
    check(validate_static([10, 0, 2, 15], m24, [10, 0, 2, 255]).is_err(), "şlyuz broadcast")?;
    check(validate_static([224, 0, 0, 1], m24, [10, 0, 2, 2]).is_err(), "multicast")?;
    // VPS: /32 va boşqa tarmoqdagi şlyuz qabul qilinadi
    check(validate_static([203, 0, 113, 7], [255; 4], [203, 0, 113, 1]).is_ok(), "/32")?;
    check(validate_static([203, 0, 113, 7], m24, [198, 51, 100, 1]).is_ok(), "on-link bölmagan şlyuz")?;
    // next_hop
    check(next_hop_for([10, 0, 2, 15], m24, [10, 0, 2, 2], [10, 0, 2, 3]) == [10, 0, 2, 3], "hop: mahalliy")?;
    check(next_hop_for([10, 0, 2, 15], m24, [10, 0, 2, 2], [8, 8, 8, 8]) == [10, 0, 2, 2], "hop: internet")?;
    check(next_hop_for([203, 0, 113, 7], [255; 4], [203, 0, 113, 1], [203, 0, 113, 9]) == [203, 0, 113, 1], "hop: /32")?;
    check(next_hop_for([203, 0, 113, 7], [255; 4], [203, 0, 113, 1], [203, 0, 113, 1]) == [203, 0, 113, 1], "hop: /32 şlyuzning özi")?;
    // GUI maydonlari
    check(from_fields("10.0.2.15/24", "", "10.0.2.2", "10.0.2.3").is_ok_and(|c| c.mask == m24), "maydon: ip/24")?;
    check(from_fields("10.0.2.15", "", "10.0.2.2", "10.0.2.3").is_err(), "maydon: niqobsiz")?;
    check(from_fields("10.0.2.15", "/24", "10.0.2.2", "").is_err(), "maydon: DNS siz")?;
    // disk formati: saqlaş → öqiş
    let c = StaticCfg { ip: [10, 0, 2, 15], mask: m24, gw: [10, 0, 2, 2], dns: [10, 0, 2, 3] };
    let text = format!("til=uz\n{}[hisob]\nlogin=a\ntarmoq=dhcp\n", to_text(Some(&c)));
    check(from_text(&text) == Some(c), "aylaniş: statik")?;
    check(from_text(&to_text(None)).is_none(), "aylaniş: dhcp")?;
    check(from_text("tarmoq=statik\ntarmoq_ip=10.0.2.0\ntarmoq_niqob=255.255.255.0\ntarmoq_shlyuz=10.0.2.2\ntarmoq_dns=10.0.2.3\n").is_none(), "buzuq statik → dhcp")?;
    check(from_text("til=uz\n").is_none(), "eski format → dhcp")?;
    Ok(n)
}
