//! HKDF-SHA256 (RFC 5869) va TLS 1.3 "HKDF-Expand-Label" (RFC 8446).

use super::hmac::hmac_sha256;
use alloc::vec::Vec;

/// HKDF-Extract: tuz va kirişdan PRK (256 bit).
pub fn extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    hmac_sha256(salt, ikm)
}

/// HKDF-Expand: PRK va info dan `len` baytli kalit.
pub fn expand(prk: &[u8; 32], info: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut t: Vec<u8> = Vec::new();
    let mut counter = 1u8;
    while out.len() < len {
        let mut h = super::hmac::HmacSha256::new(prk);
        h.update(&t);
        h.update(info);
        h.update(&[counter]);
        t = h.finish().to_vec();
        out.extend_from_slice(&t);
        counter += 1;
    }
    out.truncate(len);
    out
}

/// TLS 1.3 HKDF-Expand-Label.
pub fn expand_label(secret: &[u8; 32], label: &str, context: &[u8], len: usize) -> Vec<u8> {
    let full = alloc::format!("tls13 {}", label);
    let mut info = Vec::new();
    info.extend_from_slice(&(len as u16).to_be_bytes());
    info.push(full.len() as u8);
    info.extend_from_slice(full.as_bytes());
    info.push(context.len() as u8);
    info.extend_from_slice(context);
    expand(secret, &info, len)
}

/// Derive-Secret (TLS 1.3): expand_label(secret, label, Hash(messages)).
pub fn derive_secret(secret: &[u8; 32], label: &str, transcript_hash: &[u8; 32]) -> [u8; 32] {
    let v = expand_label(secret, label, transcript_hash, 32);
    let mut out = [0u8; 32];
    out.copy_from_slice(&v);
    out
}
