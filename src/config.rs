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
use alloc::vec::Vec;

const MAGIC: &[u8; 8] = b"ZAMINDSK";
const VERSION: u32 = 2;
const CONFIG_SECTOR: u64 = 1;
const CONFIG_SECTORS: usize = 16;

pub struct Settings {
    pub lang: Lang,
    pub accounts: Vec<Account>,
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
    let mut accounts = Vec::new();
    // Har bir "[hisob]" bölimini yiğamiz; eski (bitta hisobli) format ham öqiladi.
    let mut cur: Vec<(String, String)> = Vec::new();
    let flush = |cur: &mut Vec<(String, String)>, accounts: &mut Vec<Account>, legacy: bool| {
        let get = |k: &str| cur.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
        let (Some(username), Some(full_name), Some(salt), Some(iterations), Some(wrapped_key)) = (
            get("login"),
            get("ism"),
            get("tuz").and_then(|v| unhex::<16>(&v)),
            get("takror").and_then(|v| v.parse::<u32>().ok()),
            get("kalit").and_then(|v| unhex::<{ crate::accounts::WRAPPED_LEN }>(&v)),
        ) else {
            cur.clear();
            return;
        };
        let uid = get("uid").and_then(|v| v.parse().ok()).unwrap_or(crate::accounts::FIRST_UID);
        let admin = get("admin").map_or(legacy, |v| v == "ha");
        if iterations > 0 {
            accounts.push(Account { username, full_name, uid, admin, salt, iterations, wrapped_key });
        }
        cur.clear();
    };
    let mut in_section = false;
    for line in text.lines() {
        if line.trim() == "[hisob]" {
            flush(&mut cur, &mut accounts, !in_section);
            in_section = true;
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if k == "til" && !in_section {
            lang = Lang::from_code(v).unwrap_or(Lang::Uz);
        } else if k == "mintaqa" && !in_section {
            if let Ok(m) = v.parse::<i32>() {
                crate::time::set_tz_minutes(m);
            }
        } else {
            cur.push((String::from(k), String::from(v)));
        }
    }
    flush(&mut cur, &mut accounts, !in_section);
    Some(Settings { lang, accounts })
}

/// Diskka yozadi. Natija: saqlandimi.
pub fn save(s: &Settings) -> bool {
    if !disk::present() {
        return false;
    }
    let mut text = format!(
        "# Zamin OS sozlamalari\ntil={}\nmintaqa={}\n",
        lang_code(s.lang),
        crate::time::tz_minutes()
    );
    for a in &s.accounts {
        text += &format!(
            "[hisob]\nlogin={}\nism={}\nuid={}\nadmin={}\nkdf=pbkdf2-sha256\ntakror={}\ntuz={}\nkalit={}\n",
            a.username,
            a.full_name,
            a.uid,
            if a.admin { "ha" } else { "yöq" },
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
    save(&Settings { lang: crate::i18n::current(), accounts: crate::accounts::all() })
}
