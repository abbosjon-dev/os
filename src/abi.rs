// Zamin tizim çaqiruvlari (ABI). Bu fayl yadro va foydalanuvçi kutubxonasi
// tomonidan birga işlatiladi (`include!`), şuning uçun raqamlar hamişa mos.
//
// Çaqiriş: `svc #0`, x8 = çaqiruv raqami, x0..x5 = argumentlar,
// natija x0 da. Manfiy natija — xato.

/// Jarayonni tugatadi. x0 = çiqiş kodi.
pub const SYS_EXIT: u64 = 0;
/// Konsolga yozadi. x0 = manzil, x1 = uzunlik. Natija: yozilgan baytlar.
pub const SYS_WRITE: u64 = 1;
/// Protsessorni boşqa jarayonga böşatib beradi.
pub const SYS_YIELD: u64 = 2;
/// Uxlaydi. x0 = millisekund.
pub const SYS_SLEEP: u64 = 3;
/// Tizim işga tuşganidan beri ötgan vaqt (ms).
pub const SYS_UPTIME: u64 = 4;
/// Joriy jarayon raqami (PID).
pub const SYS_PID: u64 = 5;
/// Joriy til: 0 = özbekça, 1 = русский, 2 = English.
pub const SYS_LANG: u64 = 6;

/// Xato: notöğri manzil (masalan, yadro xotirasiga işora).
pub const ERR_BAD_ADDRESS: i64 = -1;
/// Xato: nomaʼlum tizim çaqiruvi.
pub const ERR_UNKNOWN_SYSCALL: i64 = -2;
