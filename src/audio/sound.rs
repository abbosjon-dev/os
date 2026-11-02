//! Övoz çiqariş drayveri interfeysi. Haqiqiy qurilma (virtio-sound) ulanmagan
//! bölsa — jim (vizual ijro öz-özicha işlayveradi).

use crate::sync::SpinLock;

static STATE: SpinLock<bool> = SpinLock::new(false); // ijro ketyaptimi

/// Namunalarni `from_ms` dan boşlab çaladi (qurilma bölsa).
pub fn play(_samples: &[i16], _rate: u32, _channels: u16, _from_ms: u32) {
    *STATE.lock() = true;
    // TODO: virtio-sound qurilmasiga namunalarni uzatiş.
}

/// Ijroni töxtatadi.
pub fn stop() {
    *STATE.lock() = false;
}

/// Qurilma mavjudmi?
pub fn present() -> bool {
    false
}
