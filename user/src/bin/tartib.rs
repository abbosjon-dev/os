//! tartib — qatorlarni alifbo böyiça tartiblaydi: `tartib fayl` yoki `... | tartib`
//! `tartib -t` — teskari tartib.
#![no_std]
#![no_main]

use zamin::{String, Vec, fs, println};

zamin::entry!(main);

fn main() -> i64 {
    let args = zamin::args().trim();
    let (reverse, file) = match args.strip_prefix("-t") {
        Some(rest) => (true, rest.trim()),
        None => (false, args),
    };
    let data = if file.is_empty() {
        zamin::read_all_stdin()
    } else {
        match fs::read(file) {
            Ok(d) => d,
            Err(e) => {
                zamin::eprintln!("tartib: {}: {}", file, e);
                return 1;
            }
        }
    };
    let text = String::from_utf8_lossy(&data);
    let mut lines: Vec<&str> = text.lines().collect();
    lines.sort_unstable();
    if reverse {
        lines.reverse();
    }
    for l in lines {
        println!("{}", l);
    }
    0
}
