//! sana — qatorlar, sözlar va baytlarni sanaydi: `sana fayl` yoki `... | sana`
#![no_std]
#![no_main]

use zamin::{fs, println, tr};

zamin::entry!(main);

fn main() -> i64 {
    let args = zamin::args().trim();
    let data = if args.is_empty() {
        zamin::read_all_stdin()
    } else {
        match fs::read(args) {
            Ok(d) => d,
            Err(e) => {
                zamin::eprintln!("sana: {}: {}", args, e);
                return 1;
            }
        }
    };
    let text = zamin::String::from_utf8_lossy(&data);
    println!(
        "  {} {}, {} {}, {} {}",
        text.lines().count(),
        tr(["qator", "строк", "lines"]),
        text.split_whitespace().count(),
        tr(["söz", "слов", "words"]),
        data.len(),
        tr(["bayt", "байт", "bytes"])
    );
    0
}
