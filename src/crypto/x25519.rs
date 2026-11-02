//! X25519 (Curve25519) kalit almaşinuvi — noldan (tweetnacl uslubida, u oçiq manba).
//! Maydon: 2^255 - 19. Montgomery "narvoni" (ladder). Doimiy vaqtli.

type Gf = [i64; 16];

const _121665: Gf = [0xDB41, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn car25519(o: &mut Gf) {
    for i in 0..16 {
        o[i] += 1 << 16;
        let c = o[i] >> 16;
        o[(i + 1) * ((i < 15) as usize)] += c - 1 + 37 * (c - 1) * ((i == 15) as i64);
        o[i] -= c << 16;
    }
}

fn sel25519(p: &mut Gf, q: &mut Gf, b: i64) {
    let c = !(b - 1);
    for i in 0..16 {
        let t = c & (p[i] ^ q[i]);
        p[i] ^= t;
        q[i] ^= t;
    }
}

fn pack25519(o: &mut [u8; 32], n: &Gf) {
    let mut m: Gf = [0; 16];
    let mut t = *n;
    car25519(&mut t);
    car25519(&mut t);
    car25519(&mut t);
    for _ in 0..2 {
        m[0] = t[0] - 0xffed;
        for i in 1..15 {
            m[i] = t[i] - 0xffff - ((m[i - 1] >> 16) & 1);
            m[i - 1] &= 0xffff;
        }
        m[15] = t[15] - 0x7fff - ((m[14] >> 16) & 1);
        let b = (m[15] >> 16) & 1;
        m[14] &= 0xffff;
        sel25519(&mut t, &mut m, 1 - b);
    }
    for i in 0..16 {
        o[2 * i] = (t[i] & 0xff) as u8;
        o[2 * i + 1] = (t[i] >> 8) as u8;
    }
}

fn unpack25519(o: &mut Gf, n: &[u8; 32]) {
    for i in 0..16 {
        o[i] = n[2 * i] as i64 + ((n[2 * i + 1] as i64) << 8);
    }
    o[15] &= 0x7fff;
}

fn add(o: &mut Gf, a: &Gf, b: &Gf) {
    for i in 0..16 {
        o[i] = a[i] + b[i];
    }
}
fn sub(o: &mut Gf, a: &Gf, b: &Gf) {
    for i in 0..16 {
        o[i] = a[i] - b[i];
    }
}
fn mul(o: &mut Gf, a: &Gf, b: &Gf) {
    let mut t = [0i64; 31];
    for i in 0..16 {
        for j in 0..16 {
            t[i + j] += a[i] * b[j];
        }
    }
    for i in 0..15 {
        t[i] += 38 * t[i + 16];
    }
    let mut r: Gf = [0; 16];
    r.copy_from_slice(&t[..16]);
    car25519(&mut r);
    car25519(&mut r);
    *o = r;
}
fn sqr(o: &mut Gf, a: &Gf) {
    let c = *a;
    mul(o, &c, &c);
}

fn inv25519(o: &mut Gf, i: &Gf) {
    let mut c = *i;
    for a in (0..=253).rev() {
        let t = c;
        sqr(&mut c, &t);
        if a != 2 && a != 4 {
            let t2 = c;
            mul(&mut c, &t2, i);
        }
    }
    *o = c;
}

/// Montgomery narvoni: natija = scalar * nuqta.
pub fn scalarmult(out: &mut [u8; 32], scalar: &[u8; 32], point: &[u8; 32]) {
    let mut z = *scalar;
    z[0] &= 248;
    z[31] &= 127;
    z[31] |= 64;
    let mut x: Gf = [0; 16];
    unpack25519(&mut x, point);
    let mut a: Gf = [0; 16];
    let mut b = x;
    let mut c: Gf = [0; 16];
    let mut d: Gf = [0; 16];
    let (mut e, mut f): (Gf, Gf) = ([0; 16], [0; 16]);
    a[0] = 1;
    d[0] = 1;
    for i in (0..=254).rev() {
        let bit = ((z[i >> 3] >> (i & 7)) & 1) as i64;
        sel25519(&mut a, &mut b, bit);
        sel25519(&mut c, &mut d, bit);
        add(&mut e, &a, &c);
        let t = a;
        sub(&mut a, &t, &c);
        add(&mut c, &b, &d);
        let t = b;
        sub(&mut b, &t, &d);
        sqr(&mut d, &e);
        sqr(&mut f, &a);
        let t = a;
        mul(&mut a, &c, &t);
        mul(&mut c, &b, &e);
        add(&mut e, &a, &c);
        let t = a;
        sub(&mut a, &t, &c);
        sqr(&mut b, &a);
        sub(&mut c, &d, &f);
        mul(&mut a, &c, &_121665);
        let t = a;
        add(&mut a, &t, &d);
        let t = c;
        mul(&mut c, &t, &a);
        mul(&mut a, &d, &f);
        mul(&mut d, &b, &x);
        sqr(&mut b, &e);
        sel25519(&mut a, &mut b, bit);
        sel25519(&mut c, &mut d, bit);
    }
    let mut cc = c;
    let t = cc;
    inv25519(&mut cc, &t);
    let t = a;
    mul(&mut a, &t, &cc);
    pack25519(out, &a);
}

/// Berilgan maxfiy kalitdan oçiq kalit (9 asos nuqtasi bilan).
pub fn public_key(out: &mut [u8; 32], secret: &[u8; 32]) {
    let mut base = [0u8; 32];
    base[0] = 9;
    scalarmult(out, secret, &base);
}

/// Standart test vektori (RFC 7748) bilan tekşiriş.
pub fn self_test() -> bool {
    // a = 9 ni maxfiy kalit a_priv ga köpaytirsak, RFC 7748 dagi birinçi iteratsiya.
    let mut k = [0u8; 32];
    k[0] = 9;
    let u = k;
    let mut out = [0u8; 32];
    scalarmult(&mut out, &k, &u);
    // RFC 7748: natija 422c8e7a...
    out[0] == 0x42 && out[1] == 0x2c && out[31] == 0x79
}
