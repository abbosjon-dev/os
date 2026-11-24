//! `xavfsizlik` buyruği: tizim himoyasining joriy holati.

use crate::i18n::{self, msg};
use crate::{accounts, drivers, fs, print, println, random, tprintln};

fn line(label: &i18n::Text, on: bool) {
    print!("    [{}] {} — ", if on { "+" } else { "-" }, i18n::pick(label));
    println!("{}", i18n::pick(if on { &msg::SEC_ON } else { &msg::SEC_OFF }));
}

/// Ruxsatlarni kim özgartira oladi: faylning egasi yoki administrator (uid 0 — yadro ham).
/// Tizim fayllari (faqat öqiş uçun) — heç kim.
pub fn may_chmod(owner: u32, uid: u32, readonly: bool) -> bool {
    !readonly && (owner == uid || accounts::is_admin_uid(uid))
}

/// Ruxsat qatori: 0o644 → "rw-r--r--", 0o755 → "rwxr-xr-x"; sticky (0o1000) — oxirida "t".
pub fn mode_string(mode: u16) -> alloc::string::String {
    let mut s = alloc::string::String::with_capacity(9);
    for k in (0..3).rev() {
        let b = (mode >> (k * 3)) & 7;
        s.push(if b & 4 != 0 { 'r' } else { '-' });
        s.push(if b & 2 != 0 { 'w' } else { '-' });
        let x = b & 1 != 0;
        s.push(match (k == 0 && mode & fs::PERM_STICKY != 0, x) {
            (true, true) => 't',
            (true, false) => 'T',
            (false, true) => 'x',
            (false, false) => '-',
        });
    }
    s
}

pub fn report() {
    tprintln!(msg::SEC_TITLE);
    line(&msg::SEC_WX, true);
    line(&msg::SEC_WXN, true);
    line(&msg::SEC_PAN, crate::memory::mmu::pan_enabled());
    line(&msg::SEC_GUARD, true);
    line(&msg::SEC_ZERO, true);
    line(&msg::SEC_ISOLATION, true);
    line(&msg::SEC_ASLR, true);
    line(&msg::SEC_UCOPY, true);
    line(&msg::SEC_PASSWORD, true);
    let (virtio, rndr) = random::sources();
    print!("    [+] {} — ChaCha20", i18n::pick(&msg::SEC_RNG));
    println!(
        " ({}{}{})",
        if virtio { "virtio-rng, " } else { "" },
        if rndr { "RNDR, " } else { "" },
        "timer"
    );
    let disk = drivers::virtio_blk::present() && accounts::master_key().is_some();
    print!("    [{}] {} — ", if disk { "+" } else { "-" }, i18n::pick(&msg::SEC_DISK));
    if disk {
        let iters = accounts::current().map_or(accounts::ITERATIONS, |a| a.iterations);
        crate::tprintln!(msg::SEC_DISK_ON, iters);
        let (generation, slot, used, total) = fs::zaminfs::state();
        print!("        ");
        crate::tprintln!(msg::SEC_DISK_STATE, generation, if slot == 0 { "A" } else { "B" }, used, total);
    } else {
        println!("{}", i18n::pick(&msg::SEC_OFF));
    }
}
