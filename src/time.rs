//! Haqiqiy vaqt: RTC (PL031) soniyalari + vaqt mintaqasi (standart — Toşkent, UTC+5).

use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicI32, Ordering};

/// Mintaqa: UTC dan farq (daqiqalarda)
static TZ_MINUTES: AtomicI32 = AtomicI32::new(5 * 60);

pub fn tz_minutes() -> i32 {
    TZ_MINUTES.load(Ordering::Relaxed)
}

pub fn set_tz_minutes(m: i32) {
    TZ_MINUTES.store(m.clamp(-12 * 60, 14 * 60), Ordering::Relaxed);
}

/// Sana-vaqt qismlari
pub struct DateTime {
    pub year: u64,
    pub month: u64,
    pub day: u64,
    pub hour: u64,
    pub minute: u64,
    pub second: u64,
}

fn leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Unix soniyalar (UTC) → mahalliy sana-vaqt.
pub fn local(unix: u64) -> DateTime {
    utc((unix as i64 + tz_minutes() as i64 * 60).max(0) as u64)
}

/// Unix soniyalar → sana-vaqt (UTC, mintaqasiz).
pub fn utc(t: u64) -> DateTime {
    let mut days = t / 86_400;
    let rest = t % 86_400;
    let mut year = 1970;
    loop {
        let len = if leap(year) { 366 } else { 365 };
        if days < len {
            break;
        }
        days -= len;
        year += 1;
    }
    let lens = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;
    for l in lens {
        if days < l {
            break;
        }
        days -= l;
        month += 1;
    }
    DateTime { year, month, day: days + 1, hour: rest / 3600, minute: rest % 3600 / 60, second: rest % 60 }
}

pub fn now() -> DateTime {
    local(crate::timer::unix_time())
}

/// "2026-11-01 22:30:05"
pub fn format_datetime(d: &DateTime) -> String {
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", d.year, d.month, d.day, d.hour, d.minute, d.second)
}

/// "UTC+5", "UTC+5:30", "UTC-3"
pub fn tz_name() -> String {
    let m = tz_minutes();
    let sign = if m < 0 { '-' } else { '+' };
    let a = m.unsigned_abs();
    if a % 60 == 0 { format!("UTC{}{}", sign, a / 60) } else { format!("UTC{}{}:{:02}", sign, a / 60, a % 60) }
}

/// Sanadan (UTC) Unix soniyalarga — UEFI `EFI_TIME` uçun (sof funksiya: EFI yuklovçida ham,
/// yadroda ham işlaydi, xotira ajratmaydi). Noto'g'ri maydonlar (oy 13, 31-fevral, yil < 1970…)
/// bölsa — `None`.
pub fn civil_to_unix(year: u64, month: u64, day: u64, hour: u64, minute: u64, second: u64) -> Option<u64> {
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let lens = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if day < 1 || day > lens[month as usize - 1] {
        return None;
    }
    // 1970-yildan beri kunlar: tölik yillar (kabisa — 4/100/400 qoidasi) + oylar + kunlar
    let y = year - 1;
    let leaps = |y: u64| y / 4 - y / 100 + y / 400;
    let mut days = (year - 1970) * 365 + leaps(y) - leaps(1969);
    days += lens[..month as usize - 1].iter().sum::<u64>() + day - 1;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second.min(59))
}

/// EFI_TIME → Unix aylantiriş sinovi: kabisa yil, 2038 dan keyin, oy/yil oxiri, xato sanalar.
pub fn self_test() -> Result<usize, &'static str> {
    let ok = [
        ((2024, 2, 29, 12, 0, 0), 1_709_208_000u64, "kabisa yil (2024-02-29)"),
        ((2040, 1, 1, 0, 0, 0), 2_208_988_800, "2038 dan keyin (2040-01-01)"),
        ((2106, 2, 7, 6, 28, 16), 1 << 32, "32-bit chegarasidan keyin (2106)"),
        ((2025, 12, 31, 23, 59, 59), 1_767_225_599, "oy/yil oxiri (2025-12-31)"),
        ((2100, 2, 28, 23, 59, 59), 4_107_542_399, "kabisa emas (2100-02-28)"),
        ((1970, 1, 1, 0, 0, 0), 0, "davr boşi"),
    ];
    for ((y, mo, d, h, mi, s), want, what) in ok {
        if civil_to_unix(y, mo, d, h, mi, s) != Some(want) {
            return Err(what);
        }
        let t = utc(want);
        if (t.year, t.month, t.day, t.hour, t.minute, t.second) != (y, mo, d, h, mi, s) {
            return Err(what);
        }
    }
    let bad = [(2025, 2, 29), (2100, 2, 29), (2026, 13, 1), (2026, 4, 31), (1969, 12, 31), (2026, 1, 0)];
    for (y, mo, d) in bad {
        if civil_to_unix(y, mo, d, 0, 0, 0).is_some() {
            return Err("noto'g'ri sana qabul qilindi");
        }
    }
    Ok(ok.len() * 2 + bad.len())
}

