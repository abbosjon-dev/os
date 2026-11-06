//! Tizim almaşuv buferi: barça ilovalar (muharrir, brauzer manzil qatori,
//! forma maydonlari) uçun umumiy matn. Spin-qulf bilan himoyalangan.

use alloc::string::String;

use crate::sync::SpinLock;

static CLIP: SpinLock<String> = SpinLock::new(String::new());

/// Buferga matn yozadi (oldingisi almaştiriladi).
pub fn set(s: &str) {
    let mut c = CLIP.lock();
    c.clear();
    c.push_str(s);
}

/// Bufer nusxasi (böş bölişi mumkin).
pub fn get() -> String {
    CLIP.lock().clone()
}
