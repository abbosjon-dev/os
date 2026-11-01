//! Kriptografik tasodifiy sonlar generatori (CSPRNG).
//!
//! Entropiya manbalari (bor bölganlari):
//!   1. virtio-rng — gipervizor/emulyator beradigan tasodifiy baytlar
//!   2. RNDR — protsessorning apparat generatori (ARMv8.5 FEAT_RNG)
//!   3. taymer "tebranişi" — yuqori aniqlikdagi hisoblagiç va klaviatura bosiş vaqtlari
//! Hammasi SHA-256 orqali 256-bitli kalitga yiğiladi. Çiqiş — ChaCha20 oqimi.
//! Har bir sörövdan keyin kalit almaştiriladi ("fast key erasure"): xotira
//! öğirlansa ham, avvalgi tasodifiy sonlarni tiklab bölmaydi.

use crate::crypto::{chacha20, sha256::Sha256, zeroize};
use crate::sync::SpinLock;
use crate::timer;
use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static KEY: SpinLock<Option<[u8; 32]>> = SpinLock::new(None);
/// Klaviatura uzilişlari vaqtlari şu yerga aralaştiriladi.
static POOL: AtomicU64 = AtomicU64::new(0);
static HAS_VIRTIO: AtomicBool = AtomicBool::new(false);
static HAS_RNDR: AtomicBool = AtomicBool::new(false);

fn rndr_supported() -> bool {
    let isar0: u64;
    unsafe { asm!("mrs {}, id_aa64isar0_el1", out(reg) isar0) };
    (isar0 >> 60) & 0xf >= 1
}

/// RNDR registridan 64 bit. Apparat tayyor bölmasa — `None`.
fn rndr() -> Option<u64> {
    let v: u64;
    let ok: u64;
    unsafe {
        asm!(
            "mrs {v}, s3_3_c2_c4_0", // RNDR
            "cset {ok}, ne",         // muvaffaqiyatsiz bölsa Z=1
            v = out(reg) v,
            ok = out(reg) ok,
        );
    }
    (ok != 0).then_some(v)
}

/// Generatorni boşlangʼiç entropiya bilan işga tuşiradi.
pub fn init() {
    let mut h = Sha256::new();
    h.update(b"zamin-random-v1");

    if crate::drivers::virtio_rng::init() {
        let mut seed = [0u8; 64];
        if crate::drivers::virtio_rng::fill(&mut seed) {
            h.update(&seed);
            HAS_VIRTIO.store(true, Ordering::Relaxed);
        }
        zeroize(&mut seed);
    }
    if rndr_supported() {
        let mut got = 0;
        for _ in 0..16 {
            if let Some(v) = rndr() {
                h.update(&v.to_le_bytes());
                got += 1;
            }
        }
        HAS_RNDR.store(got > 0, Ordering::Relaxed);
    }
    // Taymer tebranişi: har bir öçov orasidagi kiçik farqlar
    for i in 0..512u64 {
        let mut x = timer::counter();
        for _ in 0..(x & 31) {
            x = x.rotate_left(7) ^ i;
        }
        h.update(&x.to_le_bytes());
    }
    *KEY.lock() = Some(h.finish());
}

/// Uzilişlardan (masalan, klaviatura) qöşimça entropiya.
pub fn mix_event() {
    let c = timer::counter();
    let old = POOL.load(Ordering::Relaxed);
    POOL.store(old.rotate_left(13) ^ c, Ordering::Relaxed);
}

/// `out` ni kriptografik tasodifiy baytlar bilan töldiradi.
pub fn fill(out: &mut [u8]) {
    let mut guard = KEY.lock();
    let key = guard.as_mut().expect("random::init çaqirilmagan");
    let mut nonce = [0u8; 12];
    nonce[..8].copy_from_slice(&(timer::counter() ^ POOL.swap(0, Ordering::Relaxed)).to_le_bytes());
    if HAS_RNDR.load(Ordering::Relaxed) {
        if let Some(v) = rndr() {
            nonce[8..].copy_from_slice(&(v as u32).to_le_bytes());
        }
    }
    // 0-blok — yangi kalit, qolganlari — çiqiş
    let mut next = chacha20::block(key, 0, &nonce);
    for b in out.iter_mut() {
        *b = 0;
    }
    chacha20::xor(key, 1, &nonce, out);
    key.copy_from_slice(&next[..32]);
    zeroize(&mut next);
}

pub fn bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    fill(&mut b);
    b
}

pub fn u64() -> u64 {
    u64::from_le_bytes(bytes::<8>())
}

/// (virtio-rng, RNDR) mavjudmi — `xavfsizlik` buyruği uçun.
pub fn sources() -> (bool, bool) {
    (HAS_VIRTIO.load(Ordering::Relaxed), HAS_RNDR.load(Ordering::Relaxed))
}
