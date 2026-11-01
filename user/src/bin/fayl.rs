//! fayl — fayl tizimi bilan işlaşni körsatadi: yaratiş, yoziş, öqiş, röyxat.
#![no_std]
#![no_main]

use zamin::{format, fs, println, tr};

zamin::entry!(main);

fn main() -> i64 {
    let path = "~/Hujjatlar/eslatma.txt"; // ~ — uy papkasi; "Hujjatlar" har qanday tilda topiladi
    let text = format!(
        "{}\n{}: {} ms\n",
        tr(["Bu faylni Zamin OS dasturi yozdi.", "Этот файл написала программа Zamin OS.", "This file was written by a Zamin OS program."]),
        tr(["Vaqt", "Время", "Time"]),
        zamin::uptime_ms()
    );
    if let Err(e) = fs::write(path, text.as_bytes()) {
        println!("  {}: {}", path, e);
        return 1;
    }
    let shown = tr(["~/Hujjatlar/eslatma.txt", "~/Документы/eslatma.txt", "~/Documents/eslatma.txt"]);
    println!("  {} {}", tr(["Yozildi:", "Записано:", "Written:"]), shown);

    match fs::read_to_string(path) {
        Ok(t) => {
            println!("  {}", tr(["Fayl içida:", "Содержимое:", "Contents:"]));
            for line in t.lines() {
                println!("    | {}", line);
            }
        }
        Err(e) => println!("  {}", e),
    }

    println!("  {}", tr(["~/Hujjatlar papkasi:", "Папка ~/Документы:", "Directory ~/Documents:"]));
    for name in fs::list("~/Hujjatlar").unwrap_or_default() {
        println!("    {}", name);
    }
    0
}
