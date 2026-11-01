//! Foydalanuvçi xotirasi bilan xavfsiz işlaş.
//!
//! Yadro foydalanuvçi bergan manzilni heç qaçon töğridan-töğri işlatmaydi:
//!   1. manzil joriy jarayon sahifa jadvali böyiça tekşiriladi (yoziş uçun — yoziş ruxsati ham);
//!   2. maʼlumot yadro buferiga nusxalanadi (yoki yadro buferidan nusxalanadi);
//!   3. nusxalaş PAN vaqtinça öçirilgan qisqa oraliqda bajariladi.
//! Şunda tekşiruvdan keyin maʼlumot özgartirilişi (TOCTOU) ham, yadroning foydalanuvçi
//! xotirasiga tasodifan tegişi ham mumkin emas.

use crate::abi::{ERR_BAD_ADDRESS, ERR_INVALID, ERR_TOO_BIG};
use crate::memory::mmu::with_user_access;
use alloc::string::String;
use alloc::vec::Vec;

/// Bitta çaqiruvda nusxalanadigan eng katta hajm.
pub const MAX_COPY: usize = 16 << 20;

pub fn copy_from_user(addr: u64, len: u64) -> Result<Vec<u8>, i64> {
    let len = len as usize;
    if len == 0 {
        return Ok(Vec::new());
    }
    if len > MAX_COPY {
        return Err(ERR_TOO_BIG);
    }
    if !super::user_range_ok(addr as usize, len, false) {
        return Err(ERR_BAD_ADDRESS);
    }
    let mut buf = alloc::vec![0u8; len];
    with_user_access(|| unsafe { core::ptr::copy_nonoverlapping(addr as *const u8, buf.as_mut_ptr(), len) });
    Ok(buf)
}

pub fn copy_to_user(addr: u64, data: &[u8]) -> Result<(), i64> {
    if data.is_empty() {
        return Ok(());
    }
    if !super::user_range_ok(addr as usize, data.len(), true) {
        return Err(ERR_BAD_ADDRESS);
    }
    with_user_access(|| unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), addr as *mut u8, data.len()) });
    Ok(())
}

/// Foydalanuvçi buferi yozilişi mumkinligini oldindan tekşiradi (masalan, kutişdan oldin).
pub fn check_writable(addr: u64, len: u64) -> Result<(), i64> {
    if len == 0 || super::user_range_ok(addr as usize, len as usize, true) {
        Ok(())
    } else {
        Err(ERR_BAD_ADDRESS)
    }
}

pub fn user_string(addr: u64, len: u64, max: usize) -> Result<String, i64> {
    if len as usize > max {
        return Err(ERR_TOO_BIG);
    }
    String::from_utf8(copy_from_user(addr, len)?).map_err(|_| ERR_INVALID)
}
