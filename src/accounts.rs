//! Foydalanuvçi hisoblari va parollar.
//!
//! Parol heç qaçon oçiq saqlanmaydi. Saqlanadigani: tasodifiy "tuz" (salt) va
//! `SHA-256` ni köp marta takrorlab olingan xeş. Tekşirişda xeşlar doimiy vaqtda
//! soliştiriladi (vaqt orqali parolni taxmin qilib bölmasin).

use crate::crypto::sha256::Sha256;
use crate::sync::SpinLock;
use crate::timer;
use alloc::string::String;

/// Xeşni takrorlaş soni: parolni "kuç bilan" topişni sekinlaştiradi.
const ROUNDS: usize = 4096;

#[derive(Clone)]
pub struct Account {
    pub username: String,
    pub full_name: String,
    pub salt: [u8; 16],
    pub hash: [u8; 32],
}

static CURRENT: SpinLock<Option<Account>> = SpinLock::new(None);

fn derive(salt: &[u8; 16], password: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(password.as_bytes());
    let mut out = h.finish();
    for _ in 0..ROUNDS {
        let mut h = Sha256::new();
        h.update(&out);
        h.update(salt);
        h.update(password.as_bytes());
        out = h.finish();
    }
    out
}

/// Taymer hisoblagiçidan olingan tasodifiy baytlar (foydalanuvçi tugma bosgan
/// paytlar ham aralaşadi). Haqiqiy apparat tasodifiy generatori qöşilgunça yetarli.
fn random_salt() -> [u8; 16] {
    let mut h = Sha256::new();
    for _ in 0..64 {
        h.update(&timer::counter().to_le_bytes());
    }
    let d = h.finish();
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&d[..16]);
    salt
}

pub fn create(username: &str, full_name: &str, password: &str) -> Account {
    let salt = random_salt();
    Account {
        username: String::from(username),
        full_name: String::from(full_name),
        salt,
        hash: derive(&salt, password),
    }
}

pub fn verify(account: &Account, password: &str) -> bool {
    let h = derive(&account.salt, password);
    let mut diff = 0u8;
    for (a, b) in h.iter().zip(account.hash.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

pub fn set_password(account: &mut Account, password: &str) {
    account.salt = random_salt();
    account.hash = derive(&account.salt, password);
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
        c if c.is_ascii_lowercase() || c.is_ascii_digit() => {
            // ASCII harf/raqamning özi (statik jadvaldan)
            const ASCII: &str = "abcdefghijklmnopqrstuvwxyz0123456789";
            let i = ASCII.find(c).unwrap_or(0);
            &ASCII[i..i + 1]
        }
        _ => "",
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
