//! Diskdagi tizim sozlamalari: til va foydalanuvçi hisobi.
//!
//! Disk tuzilişi:
//!   sektor 0      — sarlavha: "ZAMINDSK", versiya, sozlamalar uzunligi
//!   sektor 1..16  — sozlamalar matni ("kalit=qiymat" qatorlari): til, login, ism,
//!                   PBKDF2 tuzi va takrorlar soni, örangan asosiy kalit
//!   sektor 64..   — ZaminFS (şifrlangan fayl tizimi), ikki nusxa (A/B)
//!
//! Disk bölmasa, sozlamalar faqat xotirada turadi (keyingi yuklanişda yöqoladi).

use crate::accounts::Account;
use crate::drivers::virtio_blk::{self as disk, SECTOR};
use crate::i18n::Lang;
use alloc::format;
use alloc::string::String;
use alloc::vec;

const MAGIC: &[u8; 8] = b"ZAMINDSK";
const VERSION: u32 = 2;
const CONFIG_SECTOR: u64 = 1;
const CONFIG_SECTORS: usize = 16;

pub struct Settings {
    pub lang: Lang,
    pub account: Option<Account>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

fn lang_code(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "uz",
        Lang::Ru => "ru",
        Lang::En => "en",
    }
}

/// Diskdan öqiydi. `None` — disk yöq yoki hali sozlanmagan (birinçi işga tuşiriş).
pub fn load() -> Option<Settings> {
    let mut header = [0u8; SECTOR];
    disk::read(0, &mut header).ok()?;
    if &header[..8] != MAGIC || u32::from_le_bytes(header[8..12].try_into().ok()?) != VERSION {
        return None;
    }
    let len = (u32::from_le_bytes(header[12..16].try_into().ok()?) as usize).min(CONFIG_SECTORS * SECTOR);
    let mut buf = vec![0u8; CONFIG_SECTORS * SECTOR];
    disk::read(CONFIG_SECTOR, &mut buf).ok()?;
    let text = core::str::from_utf8(&buf[..len]).ok()?;

    let mut lang = Lang::Uz;
    let (mut user, mut name, mut salt, mut iters, mut key) = (None, None, None, None, None);
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k {
            "til" => lang = Lang::from_code(v).unwrap_or(Lang::Uz),
            "login" => user = Some(String::from(v)),
            "ism" => name = Some(String::from(v)),
            "tuz" => salt = unhex::<16>(v),
            "takror" => iters = v.parse::<u32>().ok(),
            "kalit" => key = unhex::<{ crate::accounts::WRAPPED_LEN }>(v),
            _ => {}
        }
    }
    let account = match (user, name, salt, iters, key) {
        (Some(username), Some(full_name), Some(salt), Some(iterations), Some(wrapped_key)) if iterations > 0 => {
            Some(Account { username, full_name, salt, iterations, wrapped_key })
        }
        _ => None,
    };
    Some(Settings { lang, account })
}

/// Diskka yozadi. Natija: saqlandimi.
pub fn save(s: &Settings) -> bool {
    if !disk::present() {
        return false;
    }
    let mut text = format!("# Zamin OS sozlamalari\ntil={}\n", lang_code(s.lang));
    if let Some(a) = &s.account {
        text += &format!(
            "login={}\nism={}\nkdf=pbkdf2-sha256\ntakror={}\ntuz={}\nkalit={}\n",
            a.username,
            a.full_name,
            a.iterations,
            hex(&a.salt),
            hex(&a.wrapped_key)
        );
    }
    if text.len() > CONFIG_SECTORS * SECTOR {
        return false;
    }
    let mut buf = vec![0u8; CONFIG_SECTORS * SECTOR];
    buf[..text.len()].copy_from_slice(text.as_bytes());
    let mut header = [0u8; SECTOR];
    header[..8].copy_from_slice(MAGIC);
    header[8..12].copy_from_slice(&VERSION.to_le_bytes());
    header[12..16].copy_from_slice(&(text.len() as u32).to_le_bytes());
    // Avval maʼlumot, keyin sarlavha (sarlavhasiz maʼlumot "sozlanmagan" deb hisoblanadi).
    disk::write(CONFIG_SECTOR, &buf).is_ok() && disk::write(0, &header).is_ok()
}

/// Sozlamalarni öçiradi: keyingi yuklanişda birinçi sozlaş qayta boşlanadi.
/// Örangan kalit ham öçadi — ZaminFS dagi maʼlumotlar endi heç qaçon oçilmaydi ("kripto-öçiriş").
pub fn wipe() -> bool {
    if !disk::present() {
        return false;
    }
    let zeros = vec![0u8; CONFIG_SECTORS * SECTOR];
    disk::write(CONFIG_SECTOR, &zeros).is_ok() && disk::write(0, &[0u8; SECTOR]).is_ok() && crate::fs::zaminfs::wipe()
}

/// Joriy sozlamalar (til + hisob) ni saqlaydi.
pub fn save_current() -> bool {
    save(&Settings { lang: crate::i18n::current(), account: crate::accounts::current() })
}
