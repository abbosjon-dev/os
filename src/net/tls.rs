//! Minimal TLS 1.3 mijozi (RFC 8446): X25519 + ChaCha20-Poly1305 + SHA-256.
//!
//! OGOHLANTIRIŞ: sertifikat TEKŞIRILMAYDI — bu şifrlangan kanal beradi, lekin
//! "örtadagi odam" (MITM) hujumidan himoya qilmaydi. Haqiqiy himoya uçun
//! X.509 sertifikat va imzo tekşiruvi kerak (keyingi bosqич).
//!
//! Faqat bitta şifr töplami: TLS_CHACHA20_POLY1305_SHA256, faqat x25519 guruh.

use super::tcp;
use crate::crypto::{aead, hkdf, sha256::Sha256};
use crate::random;
use crate::sync::Mutex;
use alloc::vec;
use alloc::vec::Vec;

const REC_HANDSHAKE: u8 = 22;
const REC_APP: u8 = 23;
const REC_CHANGE_CIPHER: u8 = 20;
const REC_ALERT: u8 = 21;

struct Keys {
    key: [u8; 32],
    iv: [u8; 12],
    seq: u64,
}

impl Keys {
    fn from(secret: &[u8; 32]) -> Keys {
        let key = hkdf::expand_label(secret, "key", &[], 32);
        let iv = hkdf::expand_label(secret, "iv", &[], 12);
        let mut k = Keys { key: [0; 32], iv: [0; 12], seq: 0 };
        k.key.copy_from_slice(&key);
        k.iv.copy_from_slice(&iv);
        k
    }
    fn nonce(&self) -> [u8; 12] {
        let mut n = self.iv;
        let s = self.seq.to_be_bytes();
        for i in 0..8 {
            n[4 + i] ^= s[i];
        }
        n
    }
}

struct Tls {
    tcp: usize,
    owner: usize,
    rx: Vec<u8>, // TCP dan öqilgan, hali ajratilmagan baytlar
    client_key: Option<Keys>,
    server_key: Option<Keys>,
    inbox: Vec<u8>, // oçilgan ilova maʼlumoti
}

static CONNS: Mutex<Vec<Option<Tls>>> = Mutex::new(Vec::new());

fn hash(sha: &Sha256) -> [u8; 32] {
    sha.clone().finish()
}

impl Tls {
    /// TCP dan kamida bitta töliq TLS yozuvini öqiydi: (tur, mazmun).
    fn read_record(&mut self) -> Option<(u8, Vec<u8>)> {
        loop {
            if self.rx.len() >= 5 {
                let len = u16::from_be_bytes([self.rx[3], self.rx[4]]) as usize;
                if self.rx.len() >= 5 + len {
                    let typ = self.rx[0];
                    let body = self.rx[5..5 + len].to_vec();
                    self.rx.drain(..5 + len);
                    return Some((typ, body));
                }
            }
            let chunk = tcp::recv(self.tcp, 8192, 5000);
            if chunk.is_empty() {
                return None;
            }
            self.rx.extend_from_slice(&chunk);
        }
    }

    /// Şifrlangan yozuvni oçadi: (içki tur, içki mazmun).
    fn decrypt(&mut self, body: &[u8]) -> Option<(u8, Vec<u8>)> {
        let k = self.server_key.as_mut()?;
        if body.len() < 16 {
            return None;
        }
        let mut data = body[..body.len() - 16].to_vec();
        let tag: [u8; 16] = body[body.len() - 16..].try_into().ok()?;
        let mut aad = [REC_APP, 0x03, 0x03, 0, 0];
        aad[3..5].copy_from_slice(&(body.len() as u16).to_be_bytes());
        let nonce = k.nonce();
        if !aead::open(&k.key, &nonce, &aad, &mut data, &tag) {
            return None;
        }
        k.seq += 1;
        // Oxiridagi nol töldirişni olib taşlab, haqiqiy turni topamiz
        while let Some(&0) = data.last() {
            data.pop();
        }
        let typ = data.pop()?;
        Some((typ, data))
    }

    fn send_record(&mut self, typ: u8, payload: &[u8]) {
        let mut rec = Vec::with_capacity(5 + payload.len());
        rec.push(typ);
        rec.extend_from_slice(&[0x03, 0x03]);
        rec.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        rec.extend_from_slice(payload);
        tcp::send(self.tcp, &rec);
    }

    /// Şifrlangan ilova yozuvini jönatadi (içki tur bilan).
    fn send_encrypted(&mut self, inner_type: u8, data: &[u8]) {
        let Some(k) = self.client_key.as_mut() else { return };
        let mut plain = data.to_vec();
        plain.push(inner_type);
        let total = plain.len() + 16;
        let mut aad = [REC_APP, 0x03, 0x03, 0, 0];
        aad[3..5].copy_from_slice(&(total as u16).to_be_bytes());
        let nonce = k.nonce();
        let tag = aead::seal(&k.key, &nonce, &aad, &mut plain);
        k.seq += 1;
        plain.extend_from_slice(&tag);
        self.send_record(REC_APP, &plain);
    }
}

/// TLS handshake. Natija: ulaniş raqami (handle) yoki None.
pub fn connect(ip: super::Ip, port: u16, hostname: &str) -> Option<usize> {
    let tcp_id = tcp::connect(ip, port)?;
    let mut t = Tls { tcp: tcp_id, owner: crate::task::current_pid(), rx: Vec::new(), client_key: None, server_key: None, inbox: Vec::new() };

    // X25519 kalit jufti
    let secret: [u8; 32] = random::bytes();
    let mut pubkey = [0u8; 32];
    crate::crypto::x25519::public_key(&mut pubkey, &secret);

    let mut transcript = Sha256::new();

    // ClientHello
    let ch = build_client_hello(&pubkey, hostname);
    transcript.update(&ch);
    t.send_record(REC_HANDSHAKE, &ch);

    // ServerHello (oçiq yozuv)
    let (typ, sh) = t.read_record()?;
    if typ == REC_ALERT {
        tcp::close(tcp_id);
        return None;
    }
    if typ != REC_HANDSHAKE || sh.first() != Some(&2) {
        tcp::close(tcp_id);
        return None;
    }
    transcript.update(&sh);
    let server_pub = parse_server_hello(&sh)?;

    // Umumiy sir va kalitlar jadvali
    let mut shared = [0u8; 32];
    crate::crypto::x25519::scalarmult(&mut shared, &secret, &server_pub);

    let zero = [0u8; 32];
    let early = hkdf::extract(&[], &zero);
    let empty_hash = Sha256::new().finish();
    let derived = hkdf::derive_secret(&early, "derived", &empty_hash);
    let handshake_secret = hkdf::extract(&derived, &shared);
    let th = hash(&transcript); // ClientHello..ServerHello
    let c_hs = hkdf::derive_secret(&handshake_secret, "c hs traffic", &th);
    let s_hs = hkdf::derive_secret(&handshake_secret, "s hs traffic", &th);
    t.client_key = Some(Keys::from(&c_hs));
    t.server_key = Some(Keys::from(&s_hs));

    // Server handshake yozuvlari (şifrlangan): EncryptedExtensions, Certificate*, Finished
    let mut saw_finished = false;
    let mut cert_msg: Option<Vec<u8>> = None;
    let mut hs_buf: Vec<u8> = Vec::new();
    while !saw_finished {
        let (rtyp, body) = t.read_record()?;
        match rtyp {
            REC_CHANGE_CIPHER => continue, // eʼtiborsiz
            REC_APP => {
                let Some((inner, data)) = t.decrypt(&body) else {
                    tcp::close(tcp_id);
                    return None;
                };
                if inner == REC_ALERT {
                    tcp::close(tcp_id);
                    return None;
                }
                if inner != REC_HANDSHAKE {
                    continue;
                }
                hs_buf.extend_from_slice(&data);
                // hs_buf dagi töliq handshake xabarlarini ajratamiz
                while hs_buf.len() >= 4 {
                    let mlen = u32::from_be_bytes([0, hs_buf[1], hs_buf[2], hs_buf[3]]) as usize;
                    if hs_buf.len() < 4 + mlen {
                        break;
                    }
                    let msg: Vec<u8> = hs_buf.drain(..4 + mlen).collect();
                    let msg_type = msg[0];
                    transcript.update(&msg);
                    if msg_type == 11 {
                        cert_msg = Some(msg.clone()); // server Certificate
                    }
                    if msg_type == 20 {
                        saw_finished = true; // server Finished
                    }
                }
            }
            _ => {
                tcp::close(tcp_id);
                return None;
            }
        }
    }

    // Sertifikatni tekşiramiz: amal muddati va host nomi.
    // (Imzo zanjiri — CA işonçi — hozirça tekşirilmaydi; hujjatlaştirilgan keyingi qadam.)
    match &cert_msg {
        Some(cm) => match super::x509::validate(cm, hostname, &crate::time::now()) {
            Ok(info) => {
                let name = info.names.first().map(|s| s.as_str()).unwrap_or(hostname);
                crate::tprintln!(crate::i18n::msg::TLS_CERT_OK, name);
            }
            Err(e) => {
                crate::tprintln!(crate::i18n::msg::TLS_CERT_BAD, format_args!("{:?}", e));
                tcp::close(tcp_id);
                return None;
            }
        },
        None => {
            crate::tprintln!(crate::i18n::msg::TLS_CERT_BAD, "sertifikat yöq");
            tcp::close(tcp_id);
            return None;
        }
    }

    // Transkript server Finished gaça
    let th_sf = hash(&transcript);

    // Client Finished
    let finished_key = hkdf::expand_label(&c_hs, "finished", &[], 32);
    let mut fk = [0u8; 32];
    fk.copy_from_slice(&finished_key);
    let verify = crate::crypto::hmac::hmac_sha256(&fk, &th_sf);
    let mut fin = vec![20u8, 0, 0, 32];
    fin.extend_from_slice(&verify);
    // (ChangeCipherSpec — middlebox mosligi uçun)
    t.send_record(REC_CHANGE_CIPHER, &[1]);
    t.send_encrypted(REC_HANDSHAKE, &fin);

    // Ilova kalitlari
    let derived2 = hkdf::derive_secret(&handshake_secret, "derived", &empty_hash);
    let master = hkdf::extract(&derived2, &zero);
    let c_ap = hkdf::derive_secret(&master, "c ap traffic", &th_sf);
    let s_ap = hkdf::derive_secret(&master, "s ap traffic", &th_sf);
    t.client_key = Some(Keys::from(&c_ap));
    t.server_key = Some(Keys::from(&s_ap));

    let id = {
        let mut cs = CONNS.lock();
        match cs.iter().position(|c| c.is_none()) {
            Some(i) => {
                cs[i] = Some(t);
                i
            }
            None => {
                cs.push(Some(t));
                cs.len() - 1
            }
        }
    };
    Some(id)
}

pub fn send(id: usize, data: &[u8]) -> bool {
    let me = crate::task::current_pid();
    let mut cs = CONNS.lock();
    let Some(t) = cs.get_mut(id).and_then(|c| c.as_mut()).filter(|t| t.owner == me) else { return false };
    for chunk in data.chunks(16000) {
        t.send_encrypted(REC_APP, chunk);
    }
    true
}

/// Ilova maʼlumotini öqiydi (böş — ulaniş yopildi).
pub fn recv(id: usize, max: usize) -> Vec<u8> {
    let me = crate::task::current_pid();
    let mut cs = CONNS.lock();
    let Some(t) = cs.get_mut(id).and_then(|c| c.as_mut()).filter(|t| t.owner == me) else { return Vec::new() };
    if t.inbox.is_empty() {
        loop {
            match t.read_record() {

                Some((REC_APP, body)) => match t.decrypt(&body) {
                    Some((REC_APP, data)) => {
                        t.inbox.extend_from_slice(&data);
                        break;
                    }
                    Some((REC_ALERT, _)) | None => break, // yopiş / xato
                    Some(_) => {} // handshake (session ticket) — eʼtiborsiz
                },
                Some((REC_CHANGE_CIPHER, _)) => continue,
                _ => break,
            }
        }
    }
    let n = max.min(t.inbox.len());
    t.inbox.drain(..n).collect()
}

pub fn close(id: usize) {
    let me = crate::task::current_pid();
    let mut cs = CONNS.lock();
    if let Some(t) = cs.get_mut(id).and_then(|c| c.as_mut()).filter(|t| t.owner == me) {
        let tcp_id = t.tcp;
        *cs.get_mut(id).unwrap() = None;
        drop(cs);
        tcp::close(tcp_id);
    }
}

pub fn close_owned(pid: usize) {
    let mut cs = CONNS.lock();
    for slot in cs.iter_mut() {
        if let Some(t) = slot.as_ref().filter(|t| t.owner == pid) {
            tcp::close(t.tcp);
            *slot = None;
        }
    }
}

fn build_client_hello(pubkey: &[u8; 32], hostname: &str) -> Vec<u8> {
    let mut ext = Vec::new();
    // server_name (SNI)
    let mut sni = Vec::new();
    sni.extend_from_slice(&((hostname.len() + 3) as u16).to_be_bytes());
    sni.push(0); // host_name
    sni.extend_from_slice(&(hostname.len() as u16).to_be_bytes());
    sni.extend_from_slice(hostname.as_bytes());
    push_ext(&mut ext, 0x0000, &sni);
    // supported_groups: x25519
    push_ext(&mut ext, 0x000a, &[0x00, 0x02, 0x00, 0x1d]);
    // signature_algorithms (kerak, lekin tekşirmaymiz)
    push_ext(&mut ext, 0x000d, &[0x00, 0x06, 0x04, 0x03, 0x08, 0x04, 0x08, 0x07]);
    // supported_versions: TLS 1.3
    push_ext(&mut ext, 0x002b, &[0x02, 0x03, 0x04]);
    // key_share: x25519
    let mut ks = Vec::new();
    ks.extend_from_slice(&(36u16).to_be_bytes()); // client_shares uzunligi
    ks.extend_from_slice(&[0x00, 0x1d, 0x00, 0x20]);
    ks.extend_from_slice(pubkey);
    push_ext(&mut ext, 0x0033, &ks);

    let mut body = Vec::new();
    body.extend_from_slice(&[0x03, 0x03]); // legacy_version
    body.extend_from_slice(&random::bytes::<32>()); // random
    body.push(32); // session_id uzunligi
    body.extend_from_slice(&random::bytes::<32>());
    body.extend_from_slice(&[0x00, 0x02, 0x13, 0x03]); // cipher_suites: CHACHA20
    body.extend_from_slice(&[0x01, 0x00]); // compression: null
    body.extend_from_slice(&(ext.len() as u16).to_be_bytes());
    body.extend_from_slice(&ext);

    let mut msg = vec![1u8]; // ClientHello
    msg.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
    msg.extend_from_slice(&body);
    msg
}

fn push_ext(out: &mut Vec<u8>, typ: u16, data: &[u8]) {
    out.extend_from_slice(&typ.to_be_bytes());
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
}

/// ServerHello dan server x25519 oçiq kalitini ajratadi.
fn parse_server_hello(sh: &[u8]) -> Option<[u8; 32]> {
    // sh: [1]type [3]len [2]ver [32]random [1]sid_len [sid] [2]cipher [1]comp [2]extlen [ext...]
    let mut i = 4 + 2 + 32;
    let sid_len = *sh.get(i)? as usize;
    i += 1 + sid_len + 2 + 1; // sid + cipher + comp
    let ext_len = u16::from_be_bytes([*sh.get(i)?, *sh.get(i + 1)?]) as usize;
    i += 2;
    let end = i + ext_len;
    while i + 4 <= end {
        let etyp = u16::from_be_bytes([sh[i], sh[i + 1]]);
        let elen = u16::from_be_bytes([sh[i + 2], sh[i + 3]]) as usize;
        let edata = sh.get(i + 4..i + 4 + elen)?;
        if etyp == 0x0033 {
            // key_share: group(2) + len(2) + key
            let klen = u16::from_be_bytes([edata[2], edata[3]]) as usize;
            if klen == 32 {
                return edata.get(4..36)?.try_into().ok();
            }
        }
        i += 4 + elen;
    }
    None
}
