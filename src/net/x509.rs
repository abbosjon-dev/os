//! X.509 (DER) sertifikatini tahlil qiliş va tekşiriş — noldan yozilgan.
//!
//! Hozirça: sertifikat tuzilişini oçadi, amal qiliş muddati (notBefore/notAfter)
//! va host nomini (Subject CN hamda SubjectAltName dNSName, jumladan `*.` joker)
//! tekşiradi. BU — imzo zanjirini (CA ildiz işonç) tekşirmaydi; u keyingi qadam
//! sifatida hujjatlaştirilgan. Şunga qaramay muddati ötgan yoki boşqa domenga
//! berilgan sertifikatni rad etadi.

use crate::time::DateTime;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum X509Error {
    Malformed,
    Expired,
    NotYetValid,
    HostnameMismatch,
    NoCert,
}

/// DER TLV: tag, qiymat (içki baytlar), va qolgani.
fn tlv(b: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    if b.len() < 2 {
        return None;
    }
    let tag = b[0];
    let (len, hdr) = if b[1] & 0x80 == 0 {
        (b[1] as usize, 2)
    } else {
        let n = (b[1] & 0x7f) as usize;
        if n == 0 || n > 4 || b.len() < 2 + n {
            return None;
        }
        let mut l = 0usize;
        for i in 0..n {
            l = (l << 8) | b[2 + i] as usize;
        }
        (l, 2 + n)
    };
    if b.len() < hdr + len {
        return None;
    }
    Some((tag, &b[hdr..hdr + len], &b[hdr + len..]))
}

/// Ketma-ket TLV larni ajratuvçi.
struct Seq<'a>(&'a [u8]);
impl<'a> Iterator for Seq<'a> {
    type Item = (u8, &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        let (tag, val, rest) = tlv(self.0)?;
        self.0 = rest;
        Some((tag, val))
    }
}

const TAG_SEQ: u8 = 0x30;
const TAG_SET: u8 = 0x31;
const TAG_UTCTIME: u8 = 0x17;
const TAG_GENTIME: u8 = 0x18;
const CTX0: u8 = 0xa0; // [0] EXPLICIT (version)
const CTX3: u8 = 0xa3; // [3] EXPLICIT (extensions)
const OID_CN: &[u8] = &[0x55, 0x04, 0x03]; // 2.5.4.3
const OID_SAN: &[u8] = &[0x55, 0x1d, 0x11]; // 2.5.29.17

/// Sertifikatdan kerakli maydonlar.
pub struct CertInfo {
    pub not_before: [u16; 6], // yil, oy, kun, soat, daqiqa, soniya
    pub not_after: [u16; 6],
    pub names: Vec<String>, // CN + SAN dNSName
}

/// TLS 1.3 Certificate xabaridan (msg_type=11) birinçi (leaf) sertifikat DER ini ajratadi.
fn leaf_der(cert_msg: &[u8]) -> Option<&[u8]> {
    // 4 bayt handshake sarlavhasi (tur+uzunlik) allaqaçon olib taşlangan deb hisoblaymiz,
    // ammo xavfsizlik uçun ikkala holatni ham qöllaymiz.
    let body = if cert_msg.first() == Some(&11) { &cert_msg[4..] } else { cert_msg };
    // certificate_request_context: 1 bayt uzunlik + kontekst
    if body.is_empty() {
        return None;
    }
    let ctx_len = body[0] as usize;
    let rest = body.get(1 + ctx_len..)?;
    // certificate_list: 3 bayt umumiy uzunlik
    if rest.len() < 3 {
        return None;
    }
    let list = rest.get(3..)?;
    // Birinçi CertificateEntry: 3 bayt cert_data uzunligi + DER
    if list.len() < 3 {
        return None;
    }
    let clen = ((list[0] as usize) << 16) | ((list[1] as usize) << 8) | list[2] as usize;
    list.get(3..3 + clen)
}

fn parse_time(tag: u8, b: &[u8]) -> Option<[u16; 6]> {
    let s = core::str::from_utf8(b).ok()?;
    let d = |x: &str| x.parse::<u16>().ok();
    let (yr, rest) = if tag == TAG_UTCTIME {
        // YYMMDDHHMMSSZ
        if s.len() < 12 {
            return None;
        }
        let yy = d(&s[0..2])?;
        (if yy < 50 { 2000 + yy } else { 1900 + yy }, &s[2..])
    } else if tag == TAG_GENTIME {
        // YYYYMMDDHHMMSSZ
        if s.len() < 14 {
            return None;
        }
        (d(&s[0..4])?, &s[4..])
    } else {
        return None;
    };
    Some([yr, d(&rest[0..2])?, d(&rest[2..4])?, d(&rest[4..6])?, d(&rest[6..8])?, d(&rest[8..10])?])
}

/// Subject (Name) dan Common Name (CN) ni ajratadi.
fn subject_cn(name: &[u8], out: &mut Vec<String>) {
    for (_, rdn) in Seq(name).filter(|(t, _)| *t == TAG_SET) {
        for (_, atv) in Seq(rdn).filter(|(t, _)| *t == TAG_SEQ) {
            let mut it = Seq(atv);
            let oid = it.next();
            let val = it.next();
            if let (Some((0x06, o)), Some((_, v))) = (oid, val) {
                if o == OID_CN {
                    if let Ok(s) = core::str::from_utf8(v) {
                        out.push(String::from(s));
                    }
                }
            }
        }
    }
}

/// Extensions dan SubjectAltName dNSName larni ajratadi.
fn subject_alt_names(exts: &[u8], out: &mut Vec<String>) {
    // exts: [3] EXPLICIT SEQUENCE OF Extension
    let Some((_, inner, _)) = tlv(exts) else { return }; // SEQUENCE
    for (_, ext) in Seq(inner).filter(|(t, _)| *t == TAG_SEQ) {
        let mut it = Seq(ext);
        let Some((0x06, oid)) = it.next() else { continue };
        if oid != OID_SAN {
            continue;
        }
        // keyingi: (ixtiyoriy BOOLEAN critical), keyin OCTET STRING
        let octet = it.find(|(t, _)| *t == 0x04);
        let Some((_, os)) = octet else { continue };
        // OCTET STRING içida SEQUENCE OF GeneralName
        let Some((_, gseq, _)) = tlv(os) else { continue };
        for (t, v) in Seq(gseq) {
            if t == 0x82 {
                // dNSName (kontekst [2], IA5String)
                if let Ok(s) = core::str::from_utf8(v) {
                    out.push(String::from(s));
                }
            }
        }
    }
}

/// Leaf sertifikatni tahlil qiladi.
pub fn parse(cert_msg: &[u8]) -> Result<CertInfo, X509Error> {
    let der = leaf_der(cert_msg).ok_or(X509Error::NoCert)?;
    // Certificate ::= SEQUENCE { tbsCertificate, sigAlg, sigValue }
    let (tag, cert, _) = tlv(der).ok_or(X509Error::Malformed)?;
    if tag != TAG_SEQ {
        return Err(X509Error::Malformed);
    }
    let (ttag, tbs, _) = tlv(cert).ok_or(X509Error::Malformed)?;
    if ttag != TAG_SEQ {
        return Err(X509Error::Malformed);
    }
    // TBSCertificate maydonlari tartibi:
    // [0]version?, serial, sigAlg, issuer, validity, subject, spki, ... [3]extensions?
    let mut fields = Seq(tbs);
    let mut field = fields.next();
    if matches!(field, Some((CTX0, _))) {
        field = fields.next(); // version ni ötkazamiz
    }
    // serial, sigAlg, issuer ni ötkazamiz
    for _ in 0..3 {
        field = fields.next();
    }
    // validity SEQUENCE { notBefore, notAfter }
    let (_, validity) = field.filter(|(t, _)| *t == TAG_SEQ).ok_or(X509Error::Malformed)?;
    let mut vit = Seq(validity);
    let (nbt, nb) = vit.next().ok_or(X509Error::Malformed)?;
    let (nat, na) = vit.next().ok_or(X509Error::Malformed)?;
    let not_before = parse_time(nbt, nb).ok_or(X509Error::Malformed)?;
    let not_after = parse_time(nat, na).ok_or(X509Error::Malformed)?;

    // subject
    let (_, subject) = fields.next().filter(|(t, _)| *t == TAG_SEQ).ok_or(X509Error::Malformed)?;
    let mut names = Vec::new();
    subject_cn(subject, &mut names);

    // spki ni ötkazamiz, keyin [3] extensions ni izlaymiz
    for (t, v) in fields {
        if t == CTX3 {
            subject_alt_names(v, &mut names);
        }
    }

    Ok(CertInfo { not_before, not_after, names })
}

fn cmp_time(a: &[u16; 6], b: &[u16; 6]) -> core::cmp::Ordering {
    a.cmp(b)
}

fn host_matches(pattern: &str, host: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    let h = host.trim().to_ascii_lowercase();
    if let Some(suffix) = p.strip_prefix("*.") {
        // joker faqat bitta eng çap yorliqni qoplaydi
        if let Some((_, rest)) = h.split_once('.') {
            return rest == suffix;
        }
        return false;
    }
    p == h
}

/// Sertifikatni amal muddati va host nomi böyiça tekşiradi.
/// DIQQAT: imzo zanjiri (CA işonçi) hozirça tekşirilmaydi.
pub fn validate(cert_msg: &[u8], hostname: &str, now: &DateTime) -> Result<CertInfo, X509Error> {
    let info = parse(cert_msg)?;
    let n = [now.year as u16, now.month as u16, now.day as u16, now.hour as u16, now.minute as u16, now.second as u16];
    if cmp_time(&n, &info.not_before) == core::cmp::Ordering::Less {
        return Err(X509Error::NotYetValid);
    }
    if cmp_time(&n, &info.not_after) == core::cmp::Ordering::Greater {
        return Err(X509Error::Expired);
    }
    if info.names.iter().any(|p| host_matches(p, hostname)) {
        Ok(info)
    } else {
        Err(X509Error::HostnameMismatch)
    }
}

/// Oʻrnatilgan sinov sertifikati bilan tahlilçini tekşiradi (boot self-test).
/// Sertifikat: CN=zamin.test, SAN=zamin.test va *.zamin.app, 2026..2126.
pub fn self_test() -> Result<(), X509Error> {
    let cm = super::x509_test_cert::CERT_MSG;
    let at = |y: u64| DateTime { year: y, month: 6, day: 1, hour: 12, minute: 0, second: 0 };
    // Toʻgʻri host, amal muddati içida
    validate(cm, "zamin.test", &at(2030))?;
    // Joker SAN moslaşuvi
    validate(cm, "web.zamin.app", &at(2030))?;
    // Boşqa domen — rad etilişi kerak
    if validate(cm, "evil.example", &at(2030)).err() != Some(X509Error::HostnameMismatch) {
        return Err(X509Error::Malformed);
    }
    // Muddati ötgan — rad etilişi kerak
    if validate(cm, "zamin.test", &at(2200)).err() != Some(X509Error::Expired) {
        return Err(X509Error::Malformed);
    }
    // Hali amal qilmaydigan — rad etilişi kerak
    if validate(cm, "zamin.test", &at(2000)).err() != Some(X509Error::NotYetValid) {
        return Err(X509Error::Malformed);
    }

    // Haqiqiy CA ildiz sertifikatlari (RSA va ECDSA) — tahlilçi mustahkamligi.
    // CA ildizlarida SAN yöq, lekin maydonlar tuzilişi standart.
    let isrg = parse(super::x509_test_cert::ISRG_ROOT_X1)?;
    if isrg.not_before != [2015, 6, 4, 11, 4, 38] || isrg.not_after != [2035, 6, 4, 11, 4, 38] {
        return Err(X509Error::Malformed);
    }
    if !isrg.names.iter().any(|n| n.contains("ISRG")) {
        return Err(X509Error::Malformed);
    }
    let ecc = parse(super::x509_test_cert::USERTRUST_ECC)?;
    if ecc.not_before[0] != 2010 || ecc.not_after[0] != 2038 {
        return Err(X509Error::Malformed);
    }
    Ok(())
}
