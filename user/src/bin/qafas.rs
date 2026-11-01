//! qafas — qafasdan (sandbox) qoçişga urinadi va natijalarni körsatadi.
//! `çeklangan qafas` bilan işlatilsa, joriy papkadan taşqaridagi heç narsaga yetolmasligi kerak.
#![no_std]
#![no_main]

use zamin::{Channel, fs, println, tr};

zamin::entry!(main);

fn report(what: &str, ok: bool) -> u32 {
    println!("  [{}] {}", if ok { tr(["mumkin", "можно", "allowed"]) } else { tr(["TAQIQ", "ЗАПРЕТ", "DENIED"]) }, what);
    ok as u32
}

fn main() -> i64 {
    let mut escapes = 0;
    report(tr(["joriy papkaga yoziş", "запись в текущую папку", "write to current folder"]), fs::write("qafas-sinov.txt", b"ok").is_ok());
    let _ = fs::remove("qafas-sinov.txt");
    escapes += report(tr(["yuqori papka (..)", "папка выше (..)", "parent folder (..)"]), fs::list("..").is_ok());
    escapes += report("~ (uy papkasi)", fs::list("~").is_ok());
    escapes += report("/tizim/nomi", fs::read("/tizim/nomi").is_ok());
    escapes += report("/vaqtinçalik", fs::write("/vaqtinçalik/qafas.txt", b"x").is_ok());
    escapes += report(tr(["dastur işga tuşiriş", "запуск программы", "starting a program"]), zamin::spawn("/dasturlar/salom", "").is_ok());
    escapes += report(tr(["nomli kanal", "именованный канал", "named channel"]), Channel::open("hisob").is_ok());
    if escapes == 0 {
        println!("  {}", tr(["Qafas mustahkam: heç qayerga çiqib bölmadi.", "Песочница надёжна: выйти не удалось.", "Sandbox holds: no way out."]));
        0
    } else {
        println!("  {} {}", escapes, tr(["ta yöl oçiq!", "путей открыто!", "ways out!"]));
        1
    }
}
