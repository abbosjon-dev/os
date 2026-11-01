//! Foydalanuvçi hisoblari, parollar va disk kaliti.
//!
//! Sxema:
//!   - har bir hisobga tasodifiy 256-bitli **asosiy kalit** yaratiladi — disk (ZaminFS) şu kalit bilan şifrlanadi;
//!   - paroldan PBKDF2-HMAC-SHA256 (300 000 takror, tasodifiy tuz) orqali **örov kaliti** olinadi;
//!   - asosiy kalit örov kaliti bilan ChaCha20-Poly1305 da şifrlanib ("örab") diskka yoziladi.
//!
//! Natijada: parol heç qayerda saqlanmaydi; parolsiz disk maʼlumotlarini öqib bölmaydi;
//! parol özgarganda faqat kalit qayta öraladi (maʼlumotlarni qayta şifrlaş şart emas);
//! sozlamalarni öçiriş (`tiklaş`) — kalit yöqoladi, maʼlumotlar abadiy öqib bölmas bölib qoladi.

use crate::crypto::{aead, hmac::pbkdf2_sha256, zeroize};
use crate::random;
use crate::sync::SpinLock;
use alloc::string::String;
use alloc::vec::Vec;

/// PBKDF2 takrorlari: parolni "kuç bilan" topişni qimmatlaştiradi.
pub const ITERATIONS: u32 = 300_000;
/// Örangan kalit: nonce (12) + şifrlangan kalit (32) + teg (16)
pub const WRAPPED_LEN: usize = 60;

#[derive(Clone)]
pub struct Account {
    pub username: String,
    pub full_name: String,
    pub salt: [u8; 16],
    pub iterations: u32,
    pub wrapped_key: [u8; WRAPPED_LEN],
}

static CURRENT: SpinLock<Option<Account>> = SpinLock::new(None);
/// Kirişdan keyin oçilgan asosiy kalit (faqat xotirada).
static MASTER_KEY: SpinLock<Option<[u8; 32]>> = SpinLock::new(None);

fn aad(username: &str) -> Vec<u8> {
    let mut v = Vec::from(&b"zamin-asosiy-kalit:"[..]);
    v.extend_from_slice(username.as_bytes());
    v
}

fn wrap(username: &str, password: &str, master: &[u8; 32]) -> ([u8; 16], [u8; WRAPPED_LEN]) {
    let salt = random::bytes::<16>();
    let mut kek = [0u8; 32];
    pbkdf2_sha256(password.as_bytes(), &salt, ITERATIONS, &mut kek);
    let nonce = random::bytes::<12>();
    let mut ct = *master;
    let tag = aead::seal(&kek, &nonce, &aad(username), &mut ct);
    zeroize(&mut kek);
    let mut w = [0u8; WRAPPED_LEN];
    w[..12].copy_from_slice(&nonce);
    w[12..44].copy_from_slice(&ct);
    w[44..].copy_from_slice(&tag);
    (salt, w)
}

/// Yangi hisob va uning asosiy kaliti.
pub fn create(username: &str, full_name: &str, password: &str) -> (Account, [u8; 32]) {
    let master = random::bytes::<32>();
    let (salt, wrapped_key) = wrap(username, password, &master);
    let account = Account {
        username: String::from(username),
        full_name: String::from(full_name),
        salt,
        iterations: ITERATIONS,
        wrapped_key,
    };
    (account, master)
}

/// Parol töğri bölsa — asosiy kalitni oçadi.
pub fn unlock(account: &Account, password: &str) -> Option<[u8; 32]> {
    let mut kek = [0u8; 32];
    pbkdf2_sha256(password.as_bytes(), &account.salt, account.iterations, &mut kek);
    let nonce: [u8; 12] = account.wrapped_key[..12].try_into().ok()?;
    let tag: [u8; 16] = account.wrapped_key[44..].try_into().ok()?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&account.wrapped_key[12..44]);
    let ok = aead::open(&kek, &nonce, &aad(&account.username), &mut key, &tag);
    zeroize(&mut kek);
    if ok {
        Some(key)
    } else {
        zeroize(&mut key);
        None
    }
}

pub fn verify(account: &Account, password: &str) -> bool {
    match unlock(account, password) {
        Some(mut k) => {
            zeroize(&mut k);
            true
        }
        None => false,
    }
}

/// Parolni almaştiradi: asosiy kalit özgarmaydi, faqat qayta öraladi.
pub fn set_password(account: &mut Account, master: &[u8; 32], password: &str) {
    let (salt, w) = wrap(&account.username, password, master);
    account.salt = salt;
    account.iterations = ITERATIONS;
    account.wrapped_key = w;
}

/// Parol satrini xotiradan öçiradi.
pub fn forget(mut s: String) {
    unsafe { zeroize(s.as_bytes_mut()) };
}

/// Login: kiçik lotin harflari, raqamlar, `-` va `_`; 2–32 belgi; harf bilan boşlanadi.
pub fn valid_username(name: &str) -> bool {
    (2..=32).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Bitta harfning login uçun lotinça (ASCII) körinişi.
/// Yangi alifbo harflari soddalaştiriladi (ş → s), kirill harflari transliteratsiya qilinadi.
fn ascii_fold(c: char) -> &'static str {
    const ASCII: &str = "abcdefghijklmnopqrstuvwxyz0123456789";
    match c.to_lowercase().next().unwrap_or(c) {
        'ö' | 'о' | 'ў' => "o",
        'ğ' | 'г' | 'ғ' => "g",
        'ş' | 'ш' | 'щ' | 'с' | 'ц' => "s",
        'ç' | 'ч' => "c",
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'д' => "d",
        'е' | 'э' => "e",
        'ё' => "yo",
        'ж' => "j",
        'з' => "z",
        'и' | 'ы' => "i",
        'й' => "y",
        'к' => "k",
        'қ' => "q",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'п' => "p",
        'р' => "r",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "x",
        'ҳ' => "h",
        'ю' => "yu",
        'я' => "ya",
        c => match ASCII.find(c) {
            Some(i) => &ASCII[i..i + 1],
            None => "",
        },
    }
}

/// Töliq ismdan login taklif qiladi: "Abbosjon Dev" → "abbosjon", "Аббосжон" → "abbosjon".
pub fn suggest_username(full_name: &str) -> String {
    let first = full_name.split_whitespace().next().unwrap_or("");
    let mut s: String = first.chars().map(ascii_fold).collect();
    s.truncate(32);
    if valid_username(&s) { s } else { String::from("user") }
}

pub fn set_current(a: Option<Account>) {
    *CURRENT.lock() = a;
}

pub fn current() -> Option<Account> {
    CURRENT.lock().clone()
}

pub fn set_master_key(key: Option<[u8; 32]>) {
    let mut g = MASTER_KEY.lock();
    if let Some(old) = g.as_mut() {
        zeroize(old);
    }
    *g = key;
}

pub fn master_key() -> Option<[u8; 32]> {
    *MASTER_KEY.lock()
}
