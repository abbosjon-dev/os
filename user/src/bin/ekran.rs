//! ekran — fayllarni (yoki standart kirişni) ekranga çiqaradi: `ekran a.txt b.txt`, `... | ekran`
#![no_std]
#![no_main]

use zamin::fs;

zamin::entry!(main);

fn main() -> i64 {
    let args = zamin::args().trim();
    if args.is_empty() {
        zamin::write(&zamin::read_all_stdin());
        return 0;
    }
    let mut code = 0;
    for f in args.split_whitespace() {
        match fs::read(f) {
            Ok(d) => {
                zamin::write(&d);
            }
            Err(e) => {
                zamin::eprintln!("ekran: {}: {}", f, e);
                code = 1;
            }
        }
    }
    code
}
