//! Birinçi sozlaş (til, ism, login, parol) va keyingi yuklanişlarda kiriş (login).
//!
//! Birinçi işga tuşirişda tanlangan til butun tizimga qöllanadi: xabarlar,
//! buyruqlar yordami va tizim papkalari nomlari (`/dasturlar` → `/programs`).

use crate::i18n::{self, Lang, msg};
use crate::line::read_line;
use crate::fs::zaminfs;
use crate::{accounts, config, fs, print, println, task, tprint, tprintln};
use alloc::string::String;

const MAX_NAME: usize = 64;
const MIN_PASSWORD: usize = 4;

/// Terminal oçilişidan oldin çaqiriladi.
pub fn run(disk: bool) {
    accounts::set_current(None);
    if accounts::all().is_empty() {
        wizard(disk);
    } else {
        login();
    }
}

fn choose_language() -> Lang {
    println!();
    println!("  Zamin OS ga xuş kelibsiz!");
    println!("  Добро пожаловать в Zamin OS!");
    println!("  Welcome to Zamin OS!");
    println!();
    loop {
        println!("  Tilni tanlang · Выберите язык · Choose your language");
        println!("    1) Özbekça");
        println!("    2) Русский");
        println!("    3) English");
        print!("  > ");
        match read_line(false).trim() {
            "1" | "uz" => return Lang::Uz,
            "2" | "ru" => return Lang::Ru,
            "3" | "en" => return Lang::En,
            _ => println!(),
        }
    }
}

fn wizard(disk: bool) {
    let lang = choose_language();
    i18n::set(lang);
    fs::apply_language(lang);
    println!();
    tprintln!(msg::SETUP_TITLE);
    println!();

    let full_name = loop {
        tprint!(msg::SETUP_NAME);
        let n = String::from(read_line(false).trim());
        if !n.is_empty() && n.chars().count() <= MAX_NAME {
            break n;
        }
        tprintln!(msg::SETUP_NAME_BAD);
    };

    let suggested = accounts::suggest_username(&full_name);
    tprintln!(msg::SETUP_USER_HINT);
    let username = loop {
        tprint!(msg::SETUP_USER, suggested);
        let u = read_line(false);
        let u = u.trim();
        let u = if u.is_empty() { suggested.as_str() } else { u };
        if accounts::valid_username(u) {
            break String::from(u);
        }
        tprintln!(msg::SETUP_USER_HINT);
    };

    let password = loop {
        tprint!(msg::SETUP_PASS);
        let p1 = read_line(true);
        if p1.chars().count() < MIN_PASSWORD {
            tprintln!(msg::SETUP_PASS_SHORT);
            continue;
        }
        tprint!(msg::SETUP_PASS2);
        let p2 = read_line(true);
        let same = p2 == p1;
        accounts::forget(p2);
        if same {
            break p1;
        }
        accounts::forget(p1);
        tprintln!(msg::SETUP_PASS_MISMATCH);
    };

    tprintln!(msg::SETUP_CREATING);
    // Birinçi foydalanuvçi — administrator; disk kaliti yangi
    let master = accounts::new_volume_key();
    let account = accounts::create(&username, &full_name, &password, accounts::FIRST_UID, true, &master);
    accounts::forget(password);
    accounts::set_all(alloc::vec![account.clone()]);
    accounts::set_current(Some(account));
    accounts::set_master_key(Some(master));
    let home = fs::setup_home(&username, accounts::FIRST_UID).unwrap_or_default();
    if disk {
        zaminfs::wipe(); // eski (boşqa kalit bilan şifrlangan) nusxalar qolmasin
        zaminfs::format(&master);
        let _ = fs::commit();
    }

    println!();
    tprintln!(msg::SETUP_DONE, username, full_name);
    tprintln!(msg::SETUP_HOME, home);
    if disk && config::save_current() {
        tprintln!(msg::SETUP_SAVED);
        tprintln!(msg::SETUP_ENCRYPTED);
    } else {
        tprintln!(msg::SETUP_NOT_SAVED);
    }
    println!();
}

/// Kiriş. Bitta hisob bölsa — darhol parol; bir neçta bölsa — avval foydalanuvçi nomi.
fn login() {
    let list = accounts::all();
    println!();
    let account = if list.len() == 1 {
        list[0].clone()
    } else {
        let names: alloc::vec::Vec<&str> = list.iter().map(|a| a.username.as_str()).collect();
        tprintln!(msg::LOGIN_USERS, names.join(", "));
        loop {
            tprint!(msg::LOGIN_USER_PROMPT);
            let u = read_line(false);
            if let Some(a) = list.iter().find(|a| a.username == u.trim()) {
                break a.clone();
            }
            tprintln!(msg::LOGIN_NO_USER);
        }
    };
    let account = &account;
    tprintln!(msg::LOGIN_WELCOME, account.full_name);
    let mut failures = 0u32;
    let key = loop {
        tprint!(msg::LOGIN_PROMPT, account.username);
        let p = read_line(true);
        tprintln!(msg::LOGIN_CHECKING);
        let key = accounts::unlock(account, &p);
        accounts::forget(p);
        if let Some(k) = key {
            break k;
        }
        failures += 1;
        tprintln!(msg::LOGIN_FAIL);
        // Har bir xatodan keyin kutiş ikki baravar östiriladi (eng köpi 30 s)
        task::sleep_ms((1000u64 << failures.min(5)).min(30_000));
    };
    accounts::set_master_key(Some(key));
    accounts::set_current(Some(account.clone()));
    let _ = fs::setup_home(&account.username, account.uid);
    match zaminfs::mount(&key) {
        Ok(Some(meta)) => match fs::import_meta(&meta) {
            Ok(n) => tprintln!(msg::ZFS_RESTORED, n),
            Err(e) => {
                tprintln!(msg::ZFS_ERROR, format_args!("{:?}", e));
                zaminfs::block_writes();
            }
        },
        Ok(None) => zaminfs::format(&key),
        Err(e) => tprintln!(msg::ZFS_ERROR, format_args!("{:?}", e)),
    }
    println!();
}
