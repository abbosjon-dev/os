//! IPv6 asosi: EUI-64 link-local, NDP (Neighbor Solicitation/Advertisement, Router
//! Solicitation/Advertisement), SLAAC, statik manzil (Netcup /64, şlyuz fe80::1), ICMPv6 echo.
//!
//! MLD yuborilmaydi; kelgan kadrlar MAC böyiça dasturiy filtrlanadi: bizning MAC,
//! 33:33:00:00:00:01 (barça tugunlar) va 33:33:ff:xx:xx:xx (solicited-node, bizning manzillar).
//! Sof funksiyalar (tahlil, format, nazorat summasi) self-test da tekşiriladi.

use super::{Mac, checksum};
use crate::sync::SpinLock;
use crate::task;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub type Ip6 = [u8; 16];

pub const ETH_IPV6: u16 = 0x86dd;
pub const NH_ICMP6: u8 = 58;

const ICMP6_ECHO_REQ: u8 = 128;
const ICMP6_ECHO_REP: u8 = 129;
const ND_RS: u8 = 133;
const ND_RA: u8 = 134;
const ND_NS: u8 = 135;
const ND_NA: u8 = 136;

const OPT_SLLA: u8 = 1;
const OPT_TLLA: u8 = 2;
const OPT_PREFIX: u8 = 3;

pub const NA_SOLICITED: u8 = 0x40;
pub const NA_OVERRIDE: u8 = 0x20;

pub const UNSPEC: Ip6 = [0; 16];
pub const ALL_NODES: Ip6 = [0xff, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
pub const ALL_ROUTERS: Ip6 = [0xff, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2];

// ---------------- Sof funksiyalar ----------------

/// MAC dan EUI-64 interfeys identifikatori (U/L biti almaştiriladi, ortada ff:fe).
pub fn eui64(mac: Mac) -> [u8; 8] {
    [mac[0] ^ 2, mac[1], mac[2], 0xff, 0xfe, mac[3], mac[4], mac[5]]
}

/// Prefiks (birinçi 64 bit) + EUI-64.
pub fn with_iid(prefix: &Ip6, mac: Mac) -> Ip6 {
    let mut a = [0u8; 16];
    a[..8].copy_from_slice(&prefix[..8]);
    a[8..].copy_from_slice(&eui64(mac));
    a
}

/// fe80::/64 + EUI-64.
pub fn link_local(mac: Mac) -> Ip6 {
    let mut p = UNSPEC;
    p[0] = 0xfe;
    p[1] = 0x80;
    with_iid(&p, mac)
}

/// Solicited-node multicast manzil: ff02::1:ffXX:XXXX (manzilning oxirgi 24 biti).
pub fn solicited_node(a: &Ip6) -> Ip6 {
    [0xff, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0xff, a[13], a[14], a[15]]
}

/// IPv6 multicast manzil uçun Ethernet MAC: 33:33 + oxirgi 32 bit.
pub fn multicast_mac(a: &Ip6) -> Mac {
    [0x33, 0x33, a[12], a[13], a[14], a[15]]
}

pub fn is_multicast(a: &Ip6) -> bool {
    a[0] == 0xff
}
pub fn is_link_local(a: &Ip6) -> bool {
    a[0] == 0xfe && a[1] & 0xc0 == 0x80
}
fn is_unicast(a: &Ip6) -> bool {
    let mut lo = UNSPEC;
    lo[15] = 1;
    *a != UNSPEC && *a != lo && !is_multicast(a)
}

/// Ikki manzil birinçi `plen` biti böyiça bir xilmi.
pub fn same_prefix(a: &Ip6, b: &Ip6, plen: u8) -> bool {
    let plen = plen.min(128) as usize;
    let (full, rem) = (plen / 8, plen % 8);
    if a[..full] != b[..full] {
        return false;
    }
    rem == 0 || (a[full] ^ b[full]) & (0xffu8 << (8 - rem)) == 0
}

/// ICMPv6 nazorat summasi (psevdo-sarlavha: manba, manzil, uzunlik u32, 3 nol, keyingi sarlavha).
pub fn checksum6(src: &Ip6, dst: &Ip6, nh: u8, payload: &[u8]) -> u16 {
    let len = (payload.len() as u32).to_be_bytes();
    let tail = [len[0], len[1], len[2], len[3], 0, 0, 0, nh];
    checksum(&[src, dst, &tail, payload])
}

/// Matndan IPv6: "fec0::2", "2a03:4000:6::1", "[fe80::1%eth0]". IPv4 qöşimçasi qabul qilinmaydi.
pub fn parse(s: &str) -> Option<Ip6> {
    let s = s.trim();
    let s = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')).unwrap_or(s);
    let s = s.split('%').next()?;
    if s.is_empty() {
        return None;
    }
    let groups = |part: &str| -> Option<Vec<u16>> {
        if part.is_empty() {
            return Some(Vec::new());
        }
        part.split(':')
            .map(|g| {
                if g.is_empty() || g.len() > 4 || !g.bytes().all(|b| b.is_ascii_hexdigit()) {
                    None
                } else {
                    u16::from_str_radix(g, 16).ok()
                }
            })
            .collect()
    };
    let mut w = [0u16; 8];
    match s.split_once("::") {
        Some((a, b)) => {
            if b.contains("::") {
                return None;
            }
            let (ga, gb) = (groups(a)?, groups(b)?);
            if ga.len() + gb.len() > 7 {
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

/// "manzil/prefiks" (prefiks yöq — 64).
pub fn parse_cidr(s: &str) -> Option<(Ip6, u8)> {
    match s.trim().split_once('/') {
        Some((a, n)) => {
            if n.is_empty() || n.len() > 3 || !n.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let n: u8 = n.parse().ok()?;
            (1..=128).contains(&n).then_some(())?;
            Some((parse(a)?, n))
        }
        None => Some((parse(s)?, 64)),
    }
}

/// RFC 5952 matni: kiçik harflar, eng uzun (≥2) nol guruhlar ketma-ketligi "::".
pub fn fmt(a: &Ip6) -> String {
    let w: [u16; 8] = core::array::from_fn(|i| u16::from_be_bytes([a[2 * i], a[2 * i + 1]]));
    let (mut best, mut blen, mut i) = (8, 0, 0);
    while i < 8 {
        if w[i] == 0 {
            let s = i;
            while i < 8 && w[i] == 0 {
                i += 1;
            }
            if i - s > blen && i - s >= 2 {
                best = s;
                blen = i - s;
            }
        } else {
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
        out += &format!("{:x}", w[i]);
        i += 1;
    }
    out
}

/// NDP variantlari ([type, len(8 bayt birligida), ...]) — (tur, ma'lumot) juftlari.
fn options(mut o: &[u8]) -> Option<Vec<(u8, &[u8])>> {
    let mut v = Vec::new();
    while !o.is_empty() {
        if o.len() < 2 || o[1] == 0 || o.len() < o[1] as usize * 8 {
            return None;
        }
        let n = o[1] as usize * 8;
        v.push((o[0], &o[2..n]));
        o = &o[n..];
    }
    Some(v)
}

fn opt_mac(opts: &[(u8, &[u8])], kind: u8) -> Option<Mac> {
    opts.iter().find(|(t, d)| *t == kind && d.len() >= 6).map(|(_, d)| d[..6].try_into().unwrap())
}

/// Neighbor Solicitation: (maqsad, manba MAC varianti).
pub fn parse_ns(b: &[u8]) -> Option<(Ip6, Option<Mac>)> {
    if b.len() < 24 || b[0] != ND_NS || b[1] != 0 {
        return None;
    }
    let target: Ip6 = b[8..24].try_into().unwrap();
    let opts = options(&b[24..])?;
    Some((target, opt_mac(&opts, OPT_SLLA)))
}

/// Neighbor Advertisement: (maqsad, maqsad MAC varianti, bayroqlar R/S/O).
pub fn parse_na(b: &[u8]) -> Option<(Ip6, Option<Mac>, u8)> {
    if b.len() < 24 || b[0] != ND_NA || b[1] != 0 {
        return None;
    }
    let target: Ip6 = b[8..24].try_into().unwrap();
    if is_multicast(&target) {
        return None;
    }
    let opts = options(&b[24..])?;
    Some((target, opt_mac(&opts, OPT_TLLA), b[4] & 0xe0))
}

/// Router Advertisement tahlili.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Ra {
    /// Standart şlyuz sifatida yaroqlilik (s); 0 — şlyuz emas.
    pub lifetime: u16,
    /// SLAAC uçun yaroqli prefiks (A bayrog'i, uzunlik 64, valid > 0).
    pub prefix: Option<(Ip6, u8)>,
    pub slla: Option<Mac>,
}

pub fn parse_ra(b: &[u8]) -> Option<Ra> {
    if b.len() < 16 || b[0] != ND_RA || b[1] != 0 {
        return None;
    }
    let opts = options(&b[16..])?;
    let prefix = opts.iter().find_map(|(t, d)| {
        if *t != OPT_PREFIX || d.len() < 30 {
            return None;
        }
        let (plen, flags) = (d[0], d[1]);
        let valid = u32::from_be_bytes([d[2], d[3], d[4], d[5]]);
        let preferred = u32::from_be_bytes([d[6], d[7], d[8], d[9]]);
        let p: Ip6 = d[14..30].try_into().unwrap();
        (flags & 0x40 != 0 && plen == 64 && valid > 0 && preferred <= valid && !is_link_local(&p) && !is_multicast(&p)).then_some((p, plen))
    });
    Some(Ra { lifetime: u16::from_be_bytes([b[6], b[7]]), prefix, slla: opt_mac(&opts, OPT_SLLA) })
}

fn ll_opt(kind: u8, mac: Mac) -> [u8; 8] {
    [kind, 1, mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]]
}

/// NS (nazorat summasi 0 — jönatişda töldiriladi). `slla` — manba noaniq bölmasa.
pub fn build_ns(target: &Ip6, slla: Option<Mac>) -> Vec<u8> {
    let mut b = alloc::vec![ND_NS, 0, 0, 0, 0, 0, 0, 0];
    b.extend_from_slice(target);
    if let Some(m) = slla {
        b.extend_from_slice(&ll_opt(OPT_SLLA, m));
    }
    b
}

pub fn build_na(target: &Ip6, flags: u8, tlla: Mac) -> Vec<u8> {
    let mut b = alloc::vec![ND_NA, 0, 0, 0, flags, 0, 0, 0];
    b.extend_from_slice(target);
    b.extend_from_slice(&ll_opt(OPT_TLLA, tlla));
    b
}

fn build_rs(slla: Mac) -> Vec<u8> {
    let mut b = alloc::vec![ND_RS, 0, 0, 0, 0, 0, 0, 0];
    b.extend_from_slice(&ll_opt(OPT_SLLA, slla));
    b
}

/// Kadr manzil MAC i bizgami: özimizniki, barça tugunlar yoki manzillarimizdan birining
/// solicited-node guruhi (33:33:ff:xx:xx:xx).
pub fn accept_mac(dst: &Mac, ours: &Mac, addrs: &[Ip6]) -> bool {
    if dst == ours {
        return true;
    }
    if *dst == multicast_mac(&ALL_NODES) {
        return true;
    }
    addrs.iter().any(|a| *dst == multicast_mac(&solicited_node(a)))
}

/// IPv6 manzil (dst) bizgami: özimizning manzil, ff02::1 yoki solicited-node guruhimiz.
pub fn accept_dst(dst: &Ip6, addrs: &[Ip6]) -> bool {
    *dst == ALL_NODES || addrs.iter().any(|a| a == dst || solicited_node(a) == *dst)
}

// ---------------- Statik sozlama (Netcup /64) ----------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Static6 {
    pub addr: Ip6,
    pub plen: u8,
    pub gw: Ip6,
}

pub fn validate(c: &Static6) -> Result<(), &'static str> {
    if !is_unicast(&c.addr) {
        return Err("IPv6 manzil notöğri (::, ::1 yoki multicast emas)");
    }
    if !(1..=128).contains(&c.plen) {
        return Err("IPv6 prefiks 1..128 bölişi kerak (masalan /64)");
    }
    if !is_unicast(&c.gw) {
        return Err("IPv6 şlyuz notöğri (masalan fe80::1)");
    }
    if c.gw == c.addr {
        return Err("IPv6 şlyuz manzilning özi bölmasin");
    }
    Ok(())
}

/// GUI maydonlari: ikkalasi böş — SLAAC (Ok(None)).
pub fn from_fields(addr: &str, gw: &str) -> Result<Option<Static6>, &'static str> {
    if addr.trim().is_empty() && gw.trim().is_empty() {
        return Ok(None);
    }
    let (addr, plen) = parse_cidr(addr).ok_or("IPv6 manzil notöğri (masalan 2a03:4000::1/64)")?;
    let gw = parse(gw).ok_or("IPv6 şlyuzni kiriting (masalan fe80::1)")?;
    let c = Static6 { addr, plen, gw };
    validate(&c)?;
    Ok(Some(c))
}

pub fn to_text(c: Option<&Static6>) -> String {
    match c {
        None => String::from("tarmoq6=slaac\n"),
        Some(c) => format!("tarmoq6=statik\ntarmoq6_ip={}/{}\ntarmoq6_shlyuz={}\n", fmt(&c.addr), c.plen, fmt(&c.gw)),
    }
}

/// Sozlamalar matnidan (birinçi "[hisob]" gaça). Buzuq bölsa — SLAAC.
pub fn from_text(text: &str) -> Option<Static6> {
    let (mut on, mut ip, mut gw) = (false, None, None);
    for line in text.lines() {
        if line.trim() == "[hisob]" {
            break;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match k {
            "tarmoq6" => on = v == "statik",
            "tarmoq6_ip" => ip = parse_cidr(v),
            "tarmoq6_shlyuz" => gw = parse(v),
            _ => {}
        }
    }
    let (addr, plen) = ip?;
    let c = Static6 { addr, plen, gw: gw? };
    (on && validate(&c).is_ok()).then_some(c)
}

// ---------------- Holat ----------------

struct State {
    ll: Option<Ip6>,
    slaac: Option<(Ip6, u8)>,
    stat: Option<Static6>,
    router: Option<Ip6>,
}

static ST: SpinLock<State> = SpinLock::new(State { ll: None, slaac: None, stat: None, router: None });
static PREF: SpinLock<Option<Static6>> = SpinLock::new(None);
/// (manzil, MAC, vaqt ms)
static NCACHE: SpinLock<Vec<(Ip6, Mac, u64)>> = SpinLock::new(Vec::new());
const NC_MAX: usize = 32;
const NC_TTL_MS: u64 = 60_000;
static PING6_REPLY: SpinLock<Option<(Ip6, u16, u16)>> = SpinLock::new(None);

pub fn static_pref() -> Option<Static6> {
    *PREF.lock()
}
/// Diskdan yoki Sozlamalardan: rejimni örnatadi (qöllamaydi).
pub fn set_static_pref(c: Option<Static6>) {
    *PREF.lock() = c;
}

/// Statik manzilni darhol qöllaydi (`None` — faqat SLAAC).
pub fn apply_static(c: Option<Static6>) {
    NCACHE.lock().clear();
    ST.lock().stat = c;
    if let Some(c) = c {
        crate::println!("[net ] IPv6: statik {}/{}, şlyuz {}", fmt(&c.addr), c.plen, fmt(&c.gw));
    }
}

pub fn link_local_addr() -> Option<Ip6> {
    ST.lock().ll
}
/// Global manzil (statik ustun) va prefiks.
pub fn global_addr() -> Option<(Ip6, u8)> {
    let s = ST.lock();
    s.stat.map(|c| (c.addr, c.plen)).or(s.slaac)
}
/// Standart şlyuz: statik bölsa — u, aks holda RA yuborgan router.
pub fn gateway6() -> Option<Ip6> {
    let s = ST.lock();
    s.stat.map(|c| c.gw).or(s.router)
}

fn our_addrs() -> Vec<Ip6> {
    let s = ST.lock();
    let mut v = Vec::with_capacity(3);
    v.extend(s.ll);
    v.extend(s.slaac.map(|x| x.0));
    v.extend(s.stat.map(|c| c.addr));
    v
}

/// Manba manzil: link-local manzilga — link-local, boşqalarga — global (bölsa).
fn select_src(dst: &Ip6) -> Ip6 {
    let ll = ST.lock().ll.unwrap_or(UNSPEC);
    if is_link_local(dst) || (is_multicast(dst) && dst[1] & 0x0f <= 2) {
        return ll;
    }
    global_addr().map(|g| g.0).unwrap_or(ll)
}

fn next_hop(dst: &Ip6) -> Option<Ip6> {
    if is_link_local(dst) || is_multicast(dst) {
        return Some(*dst);
    }
    let on_link = {
        let s = ST.lock();
        s.stat.is_some_and(|c| same_prefix(&c.addr, dst, c.plen)) || s.slaac.is_some_and(|(a, p)| same_prefix(&a, dst, p))
    };
    if on_link { Some(*dst) } else { gateway6() }
}

fn nc_lookup(ip: &Ip6) -> Option<Mac> {
    let now = crate::timer::uptime_ms();
    NCACHE.lock().iter().find(|(i, _, t)| i == ip && now.saturating_sub(*t) < NC_TTL_MS).map(|(_, m, _)| *m)
}

fn nc_insert(ip: Ip6, mac: Mac) {
    if !is_unicast(&ip) {
        return;
    }
    let now = crate::timer::uptime_ms();
    let mut c = NCACHE.lock();
    if let Some(e) = c.iter_mut().find(|(i, _, _)| *i == ip) {
        e.1 = mac;
        e.2 = now;
        return;
    }
    if c.len() >= NC_MAX {
        if let Some(k) = (0..c.len()).min_by_key(|&k| c[k].2) {
            c.remove(k);
        }
    }
    c.push((ip, mac, now));
}

// ---------------- Jönatiş ----------------

fn send_raw(mac: Mac, src: &Ip6, dst: &Ip6, nh: u8, hop: u8, payload: &[u8]) {
    let mut p = Vec::with_capacity(40 + payload.len());
    p.extend_from_slice(&[0x60, 0, 0, 0]);
    p.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    p.extend_from_slice(&[nh, hop]);
    p.extend_from_slice(src);
    p.extend_from_slice(dst);
    p.extend_from_slice(payload);
    super::send_eth(mac, ETH_IPV6, &p);
}

/// ICMPv6 xabarni (nazorat summasi joyi 0) töldirib jönatadi.
fn send_icmp6(mac: Mac, src: &Ip6, dst: &Ip6, hop: u8, mut msg: Vec<u8>) {
    msg[2] = 0;
    msg[3] = 0;
    let ck = checksum6(src, dst, NH_ICMP6, &msg);
    msg[2..4].copy_from_slice(&ck.to_be_bytes());
    send_raw(mac, src, dst, NH_ICMP6, hop, &msg);
}

/// Qöşni MAC ini topadi (keşda bölmasa — NS yuborib kutadi). Tarmoq oqimidan çaqirilmasin.
fn resolve(dst: &Ip6) -> Option<Mac> {
    let hop = next_hop(dst)?;
    if is_multicast(&hop) {
        return Some(multicast_mac(&hop));
    }
    if let Some(m) = nc_lookup(&hop) {
        return Some(m);
    }
    let src = select_src(&hop);
    let sn = solicited_node(&hop);
    for _ in 0..20 {
        send_icmp6(multicast_mac(&sn), &src, &sn, 255, build_ns(&hop, Some(super::our_mac())));
        task::sleep_ms(50);
        if let Some(m) = nc_lookup(&hop) {
            return Some(m);
        }
    }
    None
}

/// `dst` ga ICMPv6 echo. Natija: javob vaqti (ms) yoki `None`.
pub fn ping6(dst: &Ip6, seq: u16) -> Option<u64> {
    let id = 0x5a60 | (seq & 0xf);
    let mut pkt = alloc::vec![ICMP6_ECHO_REQ, 0, 0, 0];
    pkt.extend_from_slice(&id.to_be_bytes());
    pkt.extend_from_slice(&seq.to_be_bytes());
    pkt.extend_from_slice(b"zamin-os-ping6-0");
    *PING6_REPLY.lock() = None;
    let start = crate::timer::uptime_ms();
    let mac = resolve(dst)?;
    let src = select_src(dst);
    send_icmp6(mac, &src, dst, 64, pkt);
    while crate::timer::uptime_ms() - start < 1500 {
        if let Some((s, rid, rseq)) = *PING6_REPLY.lock() {
            if s == *dst && rid == id && rseq == seq {
                return Some(crate::timer::uptime_ms() - start);
            }
        }
        task::sleep_ms(5);
    }
    None
}

// ---------------- Qabul ----------------

/// Ethernet kadridan IPv6 paketi (tarmoq oqimida). Javoblar keşsiz — kelgan kadr MAC iga.
pub(super) fn handle(dst_mac: &Mac, src_mac: Mac, p: &[u8]) {
    let ours = our_addrs();
    if !accept_mac(dst_mac, &super::our_mac(), &ours) {
        return;
    }
    if p.len() < 40 || p[0] >> 4 != 6 {
        return;
    }
    let plen = u16::from_be_bytes([p[4], p[5]]) as usize;
    let (nh, hop) = (p[6], p[7]);
    let src: Ip6 = p[8..24].try_into().unwrap();
    let dst: Ip6 = p[24..40].try_into().unwrap();
    if 40 + plen > p.len() || nh != NH_ICMP6 || !accept_dst(&dst, &ours) {
        return;
    }
    let m = &p[40..40 + plen];
    if m.len() < 4 || checksum6(&src, &dst, NH_ICMP6, m) != 0 {
        return;
    }
    match m[0] {
        ICMP6_ECHO_REQ if m.len() >= 8 => {
            let mut r = m.to_vec();
            r[0] = ICMP6_ECHO_REP;
            let from = if is_multicast(&dst) { select_src(&src) } else { dst };
            send_icmp6(src_mac, &from, &src, 64, r);
        }
        ICMP6_ECHO_REP if m.len() >= 8 => {
            *PING6_REPLY.lock() = Some((src, u16::from_be_bytes([m[4], m[5]]), u16::from_be_bytes([m[6], m[7]])));
            super::wake();
        }
        ND_NS if hop == 255 => {
            let Some((target, slla)) = parse_ns(m) else { return };
            if !ours.contains(&target) {
                return;
            }
            let me = super::our_mac();
            if src == UNSPEC {
                // DAD — barça tugunlarga javob
                send_icmp6(multicast_mac(&ALL_NODES), &target, &ALL_NODES, 255, build_na(&target, NA_OVERRIDE, me));
                return;
            }
            let to = slla.unwrap_or(src_mac);
            nc_insert(src, to);
            send_icmp6(to, &target, &src, 255, build_na(&target, NA_SOLICITED | NA_OVERRIDE, me));
        }
        ND_NA if hop == 255 => {
            if let Some((target, tlla, _)) = parse_na(m) {
                nc_insert(target, tlla.unwrap_or(src_mac));
                super::wake();
            }
        }
        ND_RA if hop == 255 && is_link_local(&src) => {
            let Some(ra) = parse_ra(m) else { return };
            nc_insert(src, ra.slla.unwrap_or(src_mac));
            let mac = super::our_mac();
            let (new_addr, new_router) = {
                let mut s = ST.lock();
                let mut na = None;
                if let Some((pfx, pl)) = ra.prefix {
                    let a = with_iid(&pfx, mac);
                    if s.slaac.map(|x| x.0) != Some(a) {
                        s.slaac = Some((a, pl));
                        na = Some((a, pl));
                    }
                }
                let nr = if ra.lifetime > 0 && s.router != Some(src) {
                    s.router = Some(src);
                    true
                } else {
                    if ra.lifetime == 0 && s.router == Some(src) {
                        s.router = None;
                    }
                    false
                };
                (na, nr)
            };
            if let Some((a, pl)) = new_addr {
                crate::println!("[net ] IPv6: SLAAC {}/{}, şlyuz {} (RA)", fmt(&a), pl, fmt(&src));
            } else if new_router {
                crate::println!("[net ] IPv6: şlyuz {} (RA)", fmt(&src));
            }
            super::wake();
        }
        _ => {}
    }
}

/// Statik IPv6: şlyuzga ping (NDP + ICMPv6 echo) — natija jurnalga.
pub fn report_static(c: Static6) {
    match ping6(&c.gw, 1) {
        Some(ms) => crate::println!("[net ] IPv6: ping {} — javob {} ms", fmt(&c.gw), ms),
        None => crate::println!("[net ] IPv6: ping {} — javob yöq", fmt(&c.gw)),
    }
}

/// Fon: Sozlamalardan qöllangan statik IPv6 ni tekşiradi.
pub extern "C" fn check_worker(_: usize) {
    if let Some(c) = static_pref() {
        report_static(c);
    }
}

/// Fon oqimi: link-local manzil, statik sozlama, Router Solicitation (SLAAC uçun).
pub extern "C" fn worker(_: usize) {
    task::sleep_ms(150);
    let mac = super::our_mac();
    let ll = link_local(mac);
    ST.lock().ll = Some(ll);
    crate::println!("[net ] IPv6: link-local {}, solicited-node {}", fmt(&ll), fmt(&solicited_node(&ll)));
    if let Some(c) = static_pref() {
        apply_static(Some(c));
        report_static(c);
    }
    for _ in 0..3 {
        send_icmp6(multicast_mac(&ALL_ROUTERS), &ll, &ALL_ROUTERS, 255, build_rs(mac));
        for _ in 0..20 {
            task::sleep_ms(100);
            if ST.lock().router.is_some() {
                return;
            }
        }
    }
    if ST.lock().router.is_none() {
        crate::println!("[net ] IPv6: Router Advertisement kelmadi (faqat link-local{})", if static_pref().is_some() { " va statik" } else { "" });
    }
}

// ---------------- Self-test ----------------

pub fn self_test() -> Result<usize, String> {
    let mut n = 0;
    let mut check = |ok: bool, what: &str| -> Result<(), String> {
        n += 1;
        if ok { Ok(()) } else { Err(String::from(what)) }
    };
    let mac: Mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    // EUI-64 va link-local
    check(eui64(mac) == [0x50, 0x54, 0x00, 0xff, 0xfe, 0x12, 0x34, 0x56], "EUI-64")?;
    let ll = link_local(mac);
    check(fmt(&ll) == "fe80::5054:ff:fe12:3456", "link-local fmt")?;
    check(is_link_local(&ll) && !is_multicast(&ll), "link-local turi")?;
    // solicited-node
    let sn = solicited_node(&ll);
    check(fmt(&sn) == "ff02::1:ff12:3456", "solicited-node")?;
    check(multicast_mac(&sn) == [0x33, 0x33, 0xff, 0x12, 0x34, 0x56], "solicited-node MAC")?;
    check(accept_mac(&[0x33, 0x33, 0xff, 0x12, 0x34, 0x56], &[2; 6], &[ll]), "MAC filtr: solicited-node")?;
    check(accept_mac(&[0x33, 0x33, 0, 0, 0, 1], &[2; 6], &[]), "MAC filtr: ff02::1")?;
    check(!accept_mac(&[0x33, 0x33, 0xff, 0x12, 0x34, 0x57], &[2; 6], &[ll]), "MAC filtr: begona guruh")?;
    check(!accept_mac(&[0x33, 0x33, 0, 0, 0, 0x16], &[2; 6], &[ll]), "MAC filtr: MLD")?;
    check(accept_dst(&sn, &[ll]) && !accept_dst(&ALL_ROUTERS, &[ll]), "dst filtr")?;
    // tahlil va format (RFC 5952)
    let cases = [
        ("fec0::2", "fec0::2"),
        ("::", "::"),
        ("::1", "::1"),
        ("FE80:0:0:0:0:0:0:1", "fe80::1"),
        ("1:0:0:2:0:0:0:3", "1:0:0:2::3"),
        ("2001:db8:0:1:1:1:1:1", "2001:db8:0:1:1:1:1:1"),
        ("2a03:4000:6:1234::", "2a03:4000:6:1234::"),
        ("[fe80::1%eth0]", "fe80::1"),
    ];
    for (i, o) in cases {
        check(parse(i).map(|a| fmt(&a)).as_deref() == Some(o), &format!("parse/fmt {}", i))?;
    }
    for bad in ["", ":", "1::2::3", "12345::", "g::1", "1:2:3:4:5:6:7", "1:2:3:4:5:6:7:8:9", "1:2:3:4:5:6:7::8", "10.0.2.2", "::ffff:1.2.3.4"] {
        check(parse(bad).is_none(), &format!("parse rad etmadi: {:?}", bad))?;
    }
    check(parse_cidr("2a03:4000::1/64").is_some_and(|(_, p)| p == 64) && parse_cidr("fec0::5").is_some_and(|(_, p)| p == 64), "cidr")?;
    check(parse_cidr("::1/0").is_none() && parse_cidr("::1/129").is_none() && parse_cidr("::1/").is_none(), "cidr rad")?;
    let g = parse("fec0::5054:ff:fe12:3456").unwrap();
    check(same_prefix(&g, &parse("fec0::2").unwrap(), 64) && !same_prefix(&g, &parse("fec1::2").unwrap(), 64), "prefiks /64")?;
    check(same_prefix(&g, &parse("fec0::").unwrap(), 12) && !same_prefix(&g, &parse("fed0::").unwrap(), 12), "prefiks /12")?;
    // ICMPv6 nazorat summasi: ma'lum vektor (echo, fe80::5054:ff:fe12:3456 → fe80::2)
    let echo = [128u8, 0, 0, 0, 0x5a, 0x61, 0, 1, b'z', b'a', b'm', b'i', b'n'];
    let dst2 = parse("fe80::2").unwrap();
    check(checksum6(&ll, &dst2, NH_ICMP6, &echo) == 0x4eca, "ICMPv6 summasi (vektor)")?;
    let mut e2 = echo;
    e2[2..4].copy_from_slice(&0x4ecau16.to_be_bytes());
    check(checksum6(&ll, &dst2, NH_ICMP6, &e2) == 0, "ICMPv6 summasi tekşiruvi")?;
    // NA tahlili (qölda yiğilgan bayt)
    let mut na = alloc::vec![136u8, 0, 0, 0, 0x60, 0, 0, 0];
    na.extend_from_slice(&parse("fec0::2").unwrap());
    na.extend_from_slice(&[2, 1, 0x52, 0x55, 0x0a, 0, 2, 2]);
    check(parse_na(&na) == Some((parse("fec0::2").unwrap(), Some([0x52, 0x55, 0x0a, 0, 2, 2]), NA_SOLICITED | NA_OVERRIDE)), "NA tahlili")?;
    check(parse_na(&build_na(&g, NA_SOLICITED, mac)) == Some((g, Some(mac), NA_SOLICITED)), "NA aylaniş")?;
    let mut bad_na = na.clone();
    bad_na[25] = 0; // variant uzunligi 0 — rad
    check(parse_na(&bad_na).is_none() && parse_na(&na[..20]).is_none(), "buzuq NA")?;
    check(parse_ns(&build_ns(&g, Some(mac))) == Some((g, Some(mac))), "NS aylaniş")?;
    // RA: fec0::/64 (A), lifetime 1800, SLLA
    let mut ra = alloc::vec![134u8, 0, 0, 0, 64, 0, 0x07, 0x08, 0, 0, 0, 0, 0, 0, 0, 0];
    ra.extend_from_slice(&[3, 4, 64, 0xc0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]);
    ra.extend_from_slice(&parse("fec0::").unwrap());
    ra.extend_from_slice(&[1, 1, 0x52, 0x55, 0x0a, 0, 2, 2]);
    let r = parse_ra(&ra).ok_or("RA tahlili")?;
    check(r.lifetime == 1800 && r.slla == Some([0x52, 0x55, 0x0a, 0, 2, 2]), "RA lifetime/SLLA")?;
    check(r.prefix == Some((parse("fec0::").unwrap(), 64)), "RA prefiksi")?;
    check(with_iid(&r.prefix.unwrap().0, mac) == g, "SLAAC manzili")?;
    ra[19] = 0x80; // A bayrog'isiz — SLAAC yöq
    check(parse_ra(&ra).is_some_and(|r| r.prefix.is_none()), "RA: A bayrog'isiz")?;
    // statik sozlama (Netcup)
    let c = from_fields("2a03:4000:6:1234::1/64", "fe80::1").map_err(String::from)?.ok_or("statik böş")?;
    check(c.plen == 64 && fmt(&c.gw) == "fe80::1", "statik maydonlar")?;
    check(from_fields("", "") == Ok(None), "böş — SLAAC")?;
    check(from_fields("ff02::1/64", "fe80::1").is_err() && from_fields("2a03::1/64", "").is_err() && from_fields("2a03::1", "2a03::1").is_err(), "statik rad")?;
    let text = format!("til=uz\n{}[hisob]\ntarmoq6=slaac\n", to_text(Some(&c)));
    check(from_text(&text) == Some(c), "aylaniş: statik")?;
    check(from_text(&to_text(None)).is_none() && from_text("til=uz\n").is_none(), "aylaniş: slaac")?;
    Ok(n)
}
