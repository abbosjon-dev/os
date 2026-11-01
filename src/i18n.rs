//! Köp tillilik: özbekça (asosiy), русский, English.
//! Har bir xabar — `[uz, ru, en]` uçligi. Joriy til ixtiyoriy payt almaştirilişi mumkin.
//! Özbek matnlari 2026-yilgi yangi lotin alifbosida: Ş, Ç, Ö, Ğ va tutuq belgisi ʼ.

use core::fmt::{self, Display, Write};
use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Lang {
    Uz = 0,
    Ru = 1,
    En = 2,
}

impl Lang {
    pub fn from_code(code: &str) -> Option<Lang> {
        match code {
            "uz" | "öz" => Some(Lang::Uz),
            "ru" => Some(Lang::Ru),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(Lang::Uz as u8);

pub fn current() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::Ru,
        2 => Lang::En,
        _ => Lang::Uz,
    }
}

pub fn set(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

/// Bitta xabarning uç tildagi matni: `[uz, ru, en]`.
pub type Text = [&'static str; 3];

pub fn pick(t: &Text) -> &'static str {
    t[current() as usize]
}

/// Şablondagi har bir `{}` ni navbatdagi argument bilan almaştiradi.
pub fn write_template(w: &mut impl Write, tpl: &str, args: &[&dyn Display]) -> fmt::Result {
    let mut rest = tpl;
    let mut i = 0;
    while let Some(pos) = rest.find("{}") {
        w.write_str(&rest[..pos])?;
        if let Some(a) = args.get(i) {
            write!(w, "{}", a)?;
        }
        i += 1;
        rest = &rest[pos + 2..];
    }
    w.write_str(rest)
}

#[doc(hidden)]
pub fn _tprint(t: &Text, args: &[&dyn Display], newline: bool) {
    let mut c = crate::uart::console();
    let _ = write_template(&mut c, pick(t), args);
    if newline {
        let _ = c.write_str("\n");
    }
}

/// Joriy tilda çiqaradi: `tprintln!(msg::RAM, a, b)`.
#[macro_export]
macro_rules! tprintln {
    ($t:expr $(, $a:expr)* $(,)?) => {
        $crate::i18n::_tprint(&$t, &[$(&$a as &dyn core::fmt::Display),*], true)
    };
}

#[macro_export]
macro_rules! tprint {
    ($t:expr $(, $a:expr)* $(,)?) => {
        $crate::i18n::_tprint(&$t, &[$(&$a as &dyn core::fmt::Display),*], false)
    };
}

/// Barça xabarlar.
pub mod msg {
    use super::Text;

    pub const LANG_NAME: Text = ["Özbekça", "Русский", "English"];

    // --- Yuklaniş ---
    pub const DEVELOPER: Text = [
        "Dasturçi: Abbosjon Dev | zamin.app | info@zamin.app",
        "Разработчик: Abbosjon Dev | zamin.app | info@zamin.app",
        "Developer: Abbosjon Dev | zamin.app | info@zamin.app",
    ];
    pub const BOOT_LANG: Text = ["[til ] Til: {}", "[язык] Язык: {}", "[lang] Language: {}"];
    pub const BOOT_EL: Text = [
        "[boot] Imtiyoz darajasi: EL{}",
        "[boot] Уровень привилегий: EL{}",
        "[boot] Privilege level: EL{}",
    ];
    pub const DTB_MISSING: Text = [
        "[dtb ] qurilma tavsifi topilmadi, standart RAM hajmi işlatiladi",
        "[dtb ] описание устройства не найдено, используется размер ОЗУ по умолчанию",
        "[dtb ] device tree not found, using default RAM size",
    ];
    pub const DTB_OK: Text = [
        "[dtb ] qurilma tavsifi öqildi ({} KiB)",
        "[dtb ] описание устройства прочитано ({} КиБ)",
        "[dtb ] device tree loaded ({} KiB)",
    ];
    pub const MEM_RAM: Text = [
        "[mem ] RAM: {}..{} ({} MiB)",
        "[mem ] ОЗУ: {}..{} ({} МиБ)",
        "[mem ] RAM: {}..{} ({} MiB)",
    ];
    pub const MEM_KERNEL: Text = [
        "[mem ] yadro: {}..{} ({} KiB)",
        "[mem ] ядро: {}..{} ({} КиБ)",
        "[mem ] kernel: {}..{} ({} KiB)",
    ];
    pub const MMU_ON: Text = [
        "[mmu ] yoqildi: 4 KiB sahifalar, {}-bit VA, keşlar yoqiq",
        "[mmu ] включён: страницы 4 КиБ, {}-битные VA, кэши включены",
        "[mmu ] enabled: 4 KiB pages, {}-bit VA, caches on",
    ];
    pub const MEM_FRAMES: Text = [
        "[mem ] sahifalar: {} böş / {} jami",
        "[mem ] страницы: {} свободно / {} всего",
        "[mem ] pages: {} free / {} total",
    ];
    pub const HEAP_READY: Text = ["[heap] {} MiB tayyor", "[heap] {} МиБ готово", "[heap] {} MiB ready"];
    pub const IRQ_READY: Text = [
        "[irq ] GIC: {} ta uziliş liniyasi, taymer: {} Hz (çastota {} MHz)",
        "[irq ] GIC: линий прерываний — {}, таймер: {} Гц (частота {} МГц)",
        "[irq ] GIC: {} interrupt lines, timer: {} Hz (frequency {} MHz)",
    ];
    pub const IRQ_UNKNOWN: Text = [
        "[irq ] nomaʼlum uziliş: {}",
        "[irq ] неизвестное прерывание: {}",
        "[irq ] unknown interrupt: {}",
    ];
    pub const EXC_BRK: Text = [
        "[exc ] BRK #{} @ {} — qayta işlandi, davom etamiz",
        "[exc ] BRK #{} @ {} — обработано, продолжаем",
        "[exc ] BRK #{} @ {} — handled, continuing",
    ];
    pub const PANIC: Text = ["*** YADRO XATOSI ***", "*** ОШИБКА ЯДРА ***", "*** KERNEL PANIC ***"];

    // --- Öz-özini tekşiriş ---
    pub const TEST_HEAP: Text = [
        "[test] heap: Box/Vec/String va 3200 ajratiş — OK",
        "[test] куча: Box/Vec/String и 3200 выделений — OK",
        "[test] heap: Box/Vec/String and 3200 allocations — OK",
    ];
    pub const TEST_MMU: Text = [
        "[test] mmu: {} -> {} xaritalaş, yoziş va olib taşlaş — OK",
        "[test] mmu: {} -> {} отображение, запись и снятие — OK",
        "[test] mmu: {} -> {} map, write and unmap — OK",
    ];
    pub const TEST_TIMER: Text = [
        "[test] taymer: 50 ms da {} ta uziliş — OK",
        "[test] таймер: {} прерываний за 50 мс — OK",
        "[test] timer: {} interrupts in 50 ms — OK",
    ];

    // --- Zamin terminali ---
    pub const SH_WELCOME: Text = [
        "Zamin terminali. Buyruqlar röyxati uçun: yordam",
        "Терминал Zamin. Список команд: помощь",
        "Zamin terminal. For a list of commands: help",
    ];
    pub const SH_UNKNOWN: Text = [
        "  nomaʼlum buyruq: {} (yordam deb yozing)",
        "  неизвестная команда: {} (введите: помощь)",
        "  unknown command: {} (type: help)",
    ];
    pub const SH_VERSION: Text = ["  Zamin OS v{} (AArch64)", "  Zamin OS v{} (AArch64)", "  Zamin OS v{} (AArch64)"];
    pub const SH_ABOUT: Text = [
        "  Dasturçi:  Abbosjon Dev\n  Veb-sayt:  zamin.app\n  Email:     info@zamin.app",
        "  Разработчик: Abbosjon Dev\n  Веб-сайт:    zamin.app\n  Эл. почта:   info@zamin.app",
        "  Developer: Abbosjon Dev\n  Website:   zamin.app\n  Email:     info@zamin.app",
    ];
    pub const SH_UPTIME: Text = [
        "  {} soat {} daqiqa {} soniya (taymer uzilişlari: {})",
        "  {} ч {} мин {} с (прерываний таймера: {})",
        "  {} h {} min {} s (timer interrupts: {})",
    ];
    pub const SH_MEM: Text = [
        "  RAM:  {} MiB böş / {} MiB jami\n  Heap: {} KiB işlatilgan / {} KiB",
        "  ОЗУ:  {} МиБ свободно / {} МиБ всего\n  Куча: {} КиБ занято / {} КиБ",
        "  RAM:  {} MiB free / {} MiB total\n  Heap: {} KiB used / {} KiB",
    ];
    pub const SH_LANG_SET: Text = ["  Til: {}", "  Язык: {}", "  Language: {}"];
    pub const SH_LANG_USAGE: Text = [
        "  Foydalaniş: til uz | ru | en",
        "  Использование: язык uz | ru | en",
        "  Usage: lang uz | ru | en",
    ];
    pub const SH_REBOOT: Text = ["Qayta yuklanmoqda...", "Перезагрузка...", "Rebooting..."];
    pub const SH_SHUTDOWN: Text = [
        "Tizim öçirilmoqda. Xayr!",
        "Выключение. До свидания!",
        "Shutting down. Goodbye!",
    ];
}
