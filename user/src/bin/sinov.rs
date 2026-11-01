//! sinov — tizim çaqiruvlarini tekşiradi. Hammasi töğri bölsa, kod 42 bilan çiqadi.
//! Yadro uni yuklaniş paytida öz-özini tekşiriş uçun işga tuşiradi.
#![no_std]
#![no_main]

use zamin::abi::ERR_BAD_ADDRESS;

zamin::entry!(main);

fn main() -> i64 {
    if zamin::pid() == 0 {
        return 1;
    }
    let t0 = zamin::uptime_ms();
    zamin::sleep_ms(20);
    if zamin::uptime_ms() < t0 + 20 {
        return 2;
    }
    zamin::yield_now();
    if zamin::write(b"") != 0 {
        return 3;
    }
    // Yadro xotirasini yozdiriş — rad etilişi kerak
    if zamin::write_raw(0x4020_0000, 16) != ERR_BAD_ADDRESS {
        return 4;
    }
    // Xaritalanmagan foydalanuvçi manzili — rad etilişi kerak
    if zamin::write_raw(0x50_0000_0000, 16) != ERR_BAD_ADDRESS {
        return 5;
    }
    if zamin::lang() > 2 {
        return 6;
    }
    42
}
