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
    let t = (unix as i64 + tz_minutes() as i64 * 60).max(0) as u64;
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
