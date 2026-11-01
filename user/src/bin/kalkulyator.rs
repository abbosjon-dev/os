//! kalkulyator — Zamin kanallari orqali işlaydigan server.
//!
//! "hisob" kanalini tinglaydi. Har bir söröv: "<javob kanali> <a> <amal> <b>",
//! masalan "javob-7 12 * 30". Natijani sörövçining javob kanaliga yuboradi.
//! "tugat" xabari kelsa — işni tugatadi.
#![no_std]
#![no_main]

use zamin::{Channel, format, println, tr};

zamin::entry!(main);

fn calc(a: i64, op: &str, b: i64) -> Option<i64> {
    match op {
        "+" => a.checked_add(b),
        "-" => a.checked_sub(b),
        "*" | "x" => a.checked_mul(b),
        "/" => a.checked_div(b),
        "%" => a.checked_rem(b),
        _ => None,
    }
}

fn main() -> i64 {
    let Ok(inbox) = Channel::open("hisob") else { return 1 };
    println!("  [kalkulyator {}] {}", zamin::pid(), tr(["\"hisob\" kanalini tinglayapman", "слушаю канал \"hisob\"", "listening on channel \"hisob\""]));
    let mut served = 0;
    loop {
        let Ok(req) = inbox.recv_string() else { continue };
        if req == "tugat" {
            println!("  [kalkulyator] {} {}", served, tr(["ta söröv bajarildi, xayr", "запросов выполнено, пока", "requests served, bye"]));
            return 0;
        }
        let mut it = req.split_whitespace();
        let (Some(reply_to), Some(a), Some(op), Some(b)) = (it.next(), it.next(), it.next(), it.next()) else {
            continue;
        };
        let answer = match (a.parse::<i64>(), b.parse::<i64>()) {
            (Ok(a), Ok(b)) => calc(a, op, b).map_or_else(|| format!("xato"), |v| format!("{}", v)),
            _ => format!("xato"),
        };
        if let Ok(out) = Channel::open(reply_to) {
            let _ = out.send(answer.as_bytes());
        }
        served += 1;
    }
}
