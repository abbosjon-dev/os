//! tarmoq — tarmoq holatini körsatadi.
#![no_std]
#![no_main]

use zamin::net;
use zamin::{println, tr};

zamin::entry!(main);

fn main() -> i64 {
    let i = net::info();
    let m = i.mac;
    println!("  MAC:      {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", m[0], m[1], m[2], m[3], m[4], m[5]);
    if i.static_mode {
        println!("  {}", tr(["Rejim:    statik (qölda)", "Режим:    статический (вручную)", "Mode:     static (manual)"]));
    } else {
        println!("  {}", tr(["Rejim:    avtomatik (DHCP)", "Режим:    автоматически (DHCP)", "Mode:     automatic (DHCP)"]));
    }
    if i.configured {
        let pfx = u32::from_be_bytes(i.mask).leading_ones();
        println!("  IP:       {}/{}", net::ip_string(i.ip), pfx);
        println!("  {:<10}{}", tr(["Niqob:", "Маска:", "Mask:"]), net::ip_string(i.mask));
        println!("  Gateway:  {}", net::ip_string(i.gateway));
        println!("  DNS:      {}", net::ip_string(i.dns));
        println!("  {}", tr(["Holat: ulangan", "Статус: подключено", "Status: connected"]));
    } else {
        println!("  {}", tr(["Holat: sozlanmagan", "Статус: не настроено", "Status: not configured"]));
    }
    0
}
