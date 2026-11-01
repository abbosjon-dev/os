//! mijoz — "kalkulyator" serverini işga tuşiradi va unga Zamin kanallari orqali
//! sörövlar yuboradi. Jarayon yaratiş, kanallar va kutiş birga işlaşini körsatadi.
//! Öz misollaringiz: `işlat mijoz 7 * 6`.
#![no_std]
#![no_main]

use zamin::{Channel, format, println, tr};

zamin::entry!(main);

fn main() -> i64 {
    let pid = zamin::pid();
    let server = match zamin::spawn("/dasturlar/kalkulyator", "") {
        Ok(p) => p,
        Err(e) => {
            println!("  [mijoz] {}: {}", tr(["server işga tuşmadi", "сервер не запустился", "server failed to start"]), e);
            return 1;
        }
    };
    let (Ok(server_ch), Ok(inbox)) = (Channel::open("hisob"), Channel::open(&format!("javob-{}", pid))) else {
        return 2;
    };

    let custom = zamin::args().trim();
    let default = ["12 + 30", "6 * 7", "100 / 3"];
    let questions: zamin::Vec<&str> = if custom.is_empty() { default.to_vec() } else { zamin::vec![custom] };

    for q in questions {
        let _ = server_ch.send(format!("javob-{} {}", pid, q).as_bytes());
        match inbox.recv_string() {
            Ok(ans) => println!("  [mijoz {}] {} = {}", pid, q, ans),
            Err(e) => println!("  [mijoz {}] {}: {}", pid, q, e),
        }
    }
    let _ = server_ch.send(b"tugat");
    match zamin::wait(server) {
        Ok(code) => println!("  [mijoz {}] {} {}", pid, tr(["server tugadi, kod", "сервер завершился, код", "server exited, code"]), code),
        Err(e) => println!("  [mijoz {}] {}", pid, e),
    }
    0
}
