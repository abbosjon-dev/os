//! Kriptografiya — hammasi noldan yozilgan, taşqi kutubxonalarsiz.
//!
//! - `sha256`  — SHA-256 xeş
//! - `hmac`    — HMAC-SHA256 va PBKDF2-HMAC-SHA256 (paroldan kalit oliş)
//! - `chacha20`, `poly1305`, `aead` — ChaCha20-Poly1305 (RFC 8439): şifrlaş + yaxlitlik
//!
//! Har bir algoritm yuklanişda standart test vektorlari bilan tekşiriladi (`self_test`).

pub mod aead;
pub mod chacha20;
pub mod hmac;
pub mod poly1305;
pub mod sha256;

/// Ikki baytlar ketma-ketligini doimiy vaqtda soliştiradi.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    unsafe { core::ptr::read_volatile(&diff) == 0 }
}

/// Maxfiy maʼlumotni xotiradan öçiradi (kompilyator "keraksiz" deb taşlab yubormasin).
pub fn zeroize(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        unsafe { core::ptr::write_volatile(b, 0) };
    }
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

fn hex(s: &str) -> alloc::vec::Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// Barça algoritmlarni standart test vektorlari bilan tekşiradi.
pub fn self_test() -> Result<(), &'static str> {
    if !sha256::self_test() {
        return Err("SHA-256");
    }
    // HMAC-SHA256 (Vikipediyadagi klassik misol)
    let mac = hmac::hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog");
    if mac[..] != hex("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8")[..] {
        return Err("HMAC-SHA256");
    }
    // PBKDF2-HMAC-SHA256 (RFC 7914 / klassik vektor)
    let mut dk = [0u8; 32];
    hmac::pbkdf2_sha256(b"password", b"salt", 4096, &mut dk);
    if dk[..] != hex("c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a")[..] {
        return Err("PBKDF2");
    }
    // ChaCha20-Poly1305 AEAD (RFC 8439, 2.8.2)
    let key: [u8; 32] = core::array::from_fn(|i| 0x80 + i as u8);
    let nonce: [u8; 12] = hex("070000004041424344454647").try_into().unwrap();
    let aad = hex("50515253c0c1c2c3c4c5c6c7");
    let mut msg = alloc::vec::Vec::from(
        &b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it."[..],
    );
    let original = msg.clone();
    let tag = aead::seal(&key, &nonce, &aad, &mut msg);
    if msg[..16] != hex("d31a8d34648e60db7b86afbc53ef7ec2")[..] || tag[..] != hex("1ae10b594f09e26a7e902ecbd0600691")[..] {
        return Err("ChaCha20-Poly1305");
    }
    // Oçiş va buzilganini aniqlaş
    let mut tampered = msg.clone();
    tampered[5] ^= 1;
    if aead::open(&key, &nonce, &aad, &mut tampered, &tag) {
        return Err("AEAD: buzilgan maʼlumot qabul qilindi");
    }
    if !aead::open(&key, &nonce, &aad, &mut msg, &tag) || msg != original {
        return Err("AEAD: oçiş");
    }
    Ok(())
}
