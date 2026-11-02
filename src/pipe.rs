//! Quvurlar (pipe): bir jarayonning çiqişini boşqasining kirişiga ulaydi.
//! `ekran fayl.txt | izla salom | sana`
//!
//! Öquvçi maʼlumot kelgunça uxlaydi; barça yozuvçilar yopilsa — "fayl oxiri" (0 bayt).
//! Bufer töla bölsa yozuvçi kutadi; öquvçilar qolmasa — `ERR_BROKEN_PIPE`.

use crate::abi::ERR_BROKEN_PIPE;
use crate::sync::SpinLock;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

pub type PipeId = usize;
const CAPACITY: usize = 64 * 1024;
const EVENT_BASE: usize = 0x7000_0000;

struct Pipe {
    buf: VecDeque<u8>,
    readers: usize,
    writers: usize,
}

static PIPES: SpinLock<Vec<Option<Pipe>>> = SpinLock::new(Vec::new());

fn event(id: PipeId) -> usize {
    EVENT_BASE + id
}

/// Yangi quvur: bitta öquvçi va bitta yozuvçi dastagi bilan.
pub fn create() -> PipeId {
    let mut p = PIPES.lock();
    let pipe = Pipe { buf: VecDeque::new(), readers: 1, writers: 1 };
    if let Some(i) = p.iter().position(|x| x.is_none()) {
        p[i] = Some(pipe);
        i
    } else {
        p.push(Some(pipe));
        p.len() - 1
    }
}

/// Dastak nusxalandi (masalan, bola jarayonga berildi).
pub fn add_ref(id: PipeId, writer: bool) {
    if let Some(Some(p)) = PIPES.lock().get_mut(id) {
        if writer {
            p.writers += 1;
        } else {
            p.readers += 1;
        }
    }
}

/// Dastak yopildi. Ikkala tomon ham yopilsa — quvur öçiriladi.
pub fn release(id: PipeId, writer: bool) {
    {
        let mut ps = PIPES.lock();
        let Some(slot) = ps.get_mut(id) else { return };
        let Some(p) = slot.as_mut() else { return };
        if writer {
            p.writers = p.writers.saturating_sub(1);
        } else {
            p.readers = p.readers.saturating_sub(1);
        }
        if p.readers == 0 && p.writers == 0 {
            *slot = None;
        }
    }
    crate::task::wake_event(event(id)); // kutayotganlar EOF / uzilganini körsin
}

/// Yozadi (joy bölmasa kutadi). Natija: yozilgan baytlar yoki xato.
pub fn write(id: PipeId, data: &[u8]) -> Result<usize, i64> {
    let mut done = 0;
    while done < data.len() {
        {
            let mut ps = PIPES.lock();
            let p = ps.get_mut(id).and_then(|x| x.as_mut()).ok_or(ERR_BROKEN_PIPE)?;
            if p.readers == 0 {
                return Err(ERR_BROKEN_PIPE);
            }
            let room = CAPACITY.saturating_sub(p.buf.len());
            let n = room.min(data.len() - done);
            p.buf.extend(&data[done..done + n]);
            done += n;
        }
        crate::task::wake_event(event(id));
        if done < data.len() {
            if crate::task::current_killed() {
                return Err(ERR_BROKEN_PIPE);
            }
            crate::task::wait_event(event(id), || {
                PIPES.lock().get(id).and_then(|x| x.as_ref()).is_some_and(|p| p.buf.len() >= CAPACITY && p.readers > 0)
            });
        }
    }
    Ok(done)
}

/// Öqiydi (maʼlumot bölmasa kutadi). Böş natija — fayl oxiri (yozuvçilar qolmadi).
pub fn read(id: PipeId, max: usize) -> Vec<u8> {
    loop {
        {
            let mut ps = PIPES.lock();
            let Some(p) = ps.get_mut(id).and_then(|x| x.as_mut()) else { return Vec::new() };
            if !p.buf.is_empty() {
                let n = max.min(p.buf.len());
                let out: Vec<u8> = p.buf.drain(..n).collect();
                drop(ps);
                crate::task::wake_event(event(id)); // yozuvçi uçun joy böşadi
                return out;
            }
            if p.writers == 0 {
                return Vec::new();
            }
        }
        if crate::task::current_killed() {
            return Vec::new();
        }
        crate::task::wait_event(event(id), || {
            PIPES.lock().get(id).and_then(|x| x.as_ref()).is_some_and(|p| p.buf.is_empty() && p.writers > 0)
        });
    }
}
