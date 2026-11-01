//! izla — qatorlar içidan qidiradi: `izla söz fayl...` yoki `... | izla söz`
#![no_std]
#![no_main]

use zamin::{String, fs, println, tr};

zamin::entry!(main);

fn main() -> i64 {
    let args = zamin::args().trim();
    let mut it = args.split_whitespace();
    let Some(pattern) = it.next() else {
        println!("{}", tr(["Foydalaniş: izla söz [fayl...]", "Использование: izla слово [файл...]", "Usage: izla word [file...]"]));
        return 2;
    };
    let files: zamin::Vec<&str> = it.collect();
    let mut found = 0;
    let mut check = |line: &str| {
        if line.contains(pattern) {
            println!("{}", line);
            found += 1;
        }
    };
    if files.is_empty() {
        while let Some(line) = zamin::read_line() {
            check(&line);
        }
    } else {
        for f in files {
            match fs::read(f) {
                Ok(d) => String::from_utf8_lossy(&d).lines().for_each(&mut check),
                Err(e) => zamin::eprintln!("izla: {}: {}", f, e),
            }
        }
    }
    if found > 0 { 0 } else { 1 }
}
