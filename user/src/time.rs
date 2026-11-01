//! Sana va vaqt (tizim mintaqasida).

use crate::abi::{SYS_TIME, SYS_TZ};
use crate::{String, format, syscall};

pub struct DateTime {
    pub year: u64,
    pub month: u64,
    pub day: u64,
    pub hour: u64,
    pub minute: u64,
    pub second: u64,
}

/// Hozirgi vaqt: Unix soniyalar (UTC).
pub fn unix() -> u64 {
    unsafe { syscall(SYS_TIME, 0, 0, 0, 0) as u64 }
}

/// Tizim mintaqasi: UTC dan farq (daqiqalarda).
pub fn tz_minutes() -> i64 {
    unsafe { syscall(SYS_TZ, 0, 0, 0, 0) }
}

fn leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Unix soniyalar → mahalliy sana-vaqt.
pub fn local(unix: u64) -> DateTime {
    let t = (unix as i64 + tz_minutes() * 60).max(0) as u64;
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
    local(unix())
}

/// "2026-11-01 22:30"
pub fn short(d: &DateTime) -> String {
    format!("{:04}-{:02}-{:02} {:02}:{:02}", d.year, d.month, d.day, d.hour, d.minute)
}
