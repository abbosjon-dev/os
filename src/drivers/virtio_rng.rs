//! virtio-rng: emulyator / gipervizor beradigan haqiqiy tasodifiy baytlar manbai.

use super::virtio::{DEVICE_RNG, Transport};
use crate::memory::frame;
use crate::sync::SpinLock;

struct VirtioRng {
    t: Transport,
    buf: usize,
}

static RNG: SpinLock<Option<VirtioRng>> = SpinLock::new(None);

pub fn init() -> bool {
    let Some(t) = Transport::probe(DEVICE_RNG) else { return false };
    let Some(buf) = frame::alloc_zeroed() else { return false };
    *RNG.lock() = Some(VirtioRng { t, buf });
    true
}

/// `out` ni töldiradi. Natija: muvaffaqiyatlimi.
pub fn fill(out: &mut [u8]) -> bool {
    let mut g = RNG.lock();
    let Some(r) = g.as_mut() else { return false };
    let mut done = 0;
    while done < out.len() {
        let want = (out.len() - done).min(64);
        let got = r.t.submit(&[(r.buf, want, true)]).min(want);
        if got == 0 {
            return false;
        }
        unsafe { core::ptr::copy_nonoverlapping(r.buf as *const u8, out[done..].as_mut_ptr(), got) };
        done += got;
    }
    true
}
