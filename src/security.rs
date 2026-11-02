//! `xavfsizlik` buyruği: tizim himoyasining joriy holati.

use crate::i18n::{self, msg};
use crate::{accounts, drivers, fs, print, println, random, tprintln};

fn line(label: &i18n::Text, on: bool) {
    print!("    [{}] {} — ", if on { "+" } else { "-" }, i18n::pick(label));
    println!("{}", i18n::pick(if on { &msg::SEC_ON } else { &msg::SEC_OFF }));
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
