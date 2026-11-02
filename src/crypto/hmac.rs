//! HMAC-SHA256 (RFC 2104) va PBKDF2-HMAC-SHA256 (RFC 8018).

use super::sha256::Sha256;

const BLOCK: usize = 64;

/// Kalit bilan tayyorlangan HMAC holati (PBKDF2 da qayta-qayta işlatiladi).
#[derive(Clone)]
pub struct HmacSha256 {
    inner: Sha256,
    outer: Sha256,
}

impl HmacSha256 {
    pub fn new(key: &[u8]) -> Self {
        let mut k = [0u8; BLOCK];
        if key.len() > BLOCK {
            k[..32].copy_from_slice(&super::sha256::hash(key));
        } else {
            k[..key.len()].copy_from_slice(key);
        }
        let mut ipad = [0x36u8; BLOCK];
        let mut opad = [0x5cu8; BLOCK];
        for i in 0..BLOCK {
            ipad[i] ^= k[i];
            opad[i] ^= k[i];
        }
        let mut inner = Sha256::new();
        inner.update(&ipad);
        let mut outer = Sha256::new();
        outer.update(&opad);
        super::zeroize(&mut k);
        Self { inner, outer }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    pub fn finish(self) -> [u8; 32] {
        let ih = self.inner.finish();
        let mut o = self.outer;
        o.update(&ih);
        o.finish()
    }
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut h = HmacSha256::new(key);
    h.update(data);
    h.finish()
}

/// Paroldan kalit oladi. `iterations` qançalik köp bölsa, parolni taxmin qiliş şunçalik qimmat.
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) {
    let prf = HmacSha256::new(password);
    for (i, chunk) in out.chunks_mut(32).enumerate() {
        let mut h = prf.clone();
        h.update(salt);
        h.update(&(i as u32 + 1).to_be_bytes());
        let mut u = h.finish();
        let mut t = u;
        for _ in 1..iterations {
            let mut h = prf.clone();
            h.update(&u);
            u = h.finish();
            for (a, b) in t.iter_mut().zip(u.iter()) {
                *a ^= b;
            }
        }
        chunk.copy_from_slice(&t[..chunk.len()]);
        super::zeroize(&mut u);
        super::zeroize(&mut t);
    }
}
