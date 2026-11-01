//! ÇaÇa20-Poly1305 AEAD (RFC 8439): şifrlaydi va maʼlumot buzilmaganini kafolatlaydi.
//! Bitta kalit bilan har bir nonce (12 bayt) faqat bir marta işlatilişi şart.

use super::chacha20;
use super::poly1305::Poly1305;

fn mac(otk: &[u8; 32], aad: &[u8], ct: &[u8]) -> [u8; 16] {
    let mut p = Poly1305::new(otk);
    let zeros = [0u8; 16];
    p.update(aad);
    p.update(&zeros[..(16 - aad.len() % 16) % 16]);
    p.update(ct);
    p.update(&zeros[..(16 - ct.len() % 16) % 16]);
    p.update(&(aad.len() as u64).to_le_bytes());
    p.update(&(ct.len() as u64).to_le_bytes());
    p.finish()
}

fn one_time_key(key: &[u8; 32], nonce: &[u8; 12]) -> [u8; 32] {
    let b = chacha20::block(key, 0, nonce);
    let mut otk = [0u8; 32];
    otk.copy_from_slice(&b[..32]);
    otk
}

/// Joyida şifrlaydi; teg (16 bayt) qaytaradi.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &mut [u8]) -> [u8; 16] {
    chacha20::xor(key, 1, nonce, data);
    let mut otk = one_time_key(key, nonce);
    let tag = mac(&otk, aad, data);
    super::zeroize(&mut otk);
    tag
}

/// Tegni tekşiradi va joyida oçadi. Teg mos kelmasa — `false`, maʼlumotga tegilmaydi.
pub fn open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &mut [u8], tag: &[u8; 16]) -> bool {
    let mut otk = one_time_key(key, nonce);
    let expected = mac(&otk, aad, data);
    super::zeroize(&mut otk);
    if !super::ct_eq(&expected, tag) {
        return false;
    }
    chacha20::xor(key, 1, nonce, data);
    true
}
