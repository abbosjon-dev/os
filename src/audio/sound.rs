//! Övoz çiqariş — virtio-sound drayveriga yönaltiradi.
//! Qurilma bölmasa jim (vizual ijro baribir işlaydi).

use crate::drivers::virtio_snd;

/// Namunalarni `from_ms` dan boşlab çaladi (qurilma bölsa).
pub fn play(samples: &[i16], rate: u32, channels: u16, from_ms: u32) {
    virtio_snd::play(samples, rate, channels, from_ms);
}

/// Ijroni töxtatadi.
pub fn stop() {
    virtio_snd::stop();
}

/// Övoz qurilmasi mavjudmi?
pub fn present() -> bool {
    virtio_snd::present()
}

/// Hozir çalinmoqdami?
pub fn playing() -> bool {
    virtio_snd::playing()
}
