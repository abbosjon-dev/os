//! Zamin kanallari — jarayonlar orasida xabar almaşiş (IPC).
//!
//! Kanal — nomli xabarlar navbati. Uni birinçi oçgan jarayon yaratadi,
//! qolganlari şu nom orqali ulanadi. Xabarlar navbatda saqlanadi, şuning uçun
//! yuboruvçi qabul qiluvçidan oldin işga tuşsa ham heç narsa yöqolmaydi.
//! Qabul qiluvçi xabar kelgunça uxlaydi (protsessor vaqtini sarflamaydi).

use crate::abi::{ERR_FULL, ERR_INTERRUPTED, ERR_NOT_FOUND, ERR_TOO_BIG, MAX_MESSAGE};
use crate::sync::SpinLock;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;

pub type ChanId = usize;

const MAX_QUEUE: usize = 64;
const MAX_CHANNELS: usize = 256;
const MAX_NAME: usize = 64;

struct Channel {
    /// Böş — nomsiz kanal (faqat dastak orqali beriladi)
    name: String,
    /// Yaratgan foydalanuvçi: boşqalar nom orqali oça olmaydi ("ommaviy/" bundan mustasno)
    owner: u32,
    queue: VecDeque<Vec<u8>>,
    sent: u64,
}

static CHANNELS: SpinLock<Vec<Channel>> = SpinLock::new(Vec::new());

#[derive(Debug)]
pub enum IpcError {
    Full,
    TooBig,
    NotFound,
    Interrupted,
    Denied,
}

impl IpcError {
    pub fn code(&self) -> i64 {
        match self {
            IpcError::Full => ERR_FULL,
            IpcError::TooBig => ERR_TOO_BIG,
            IpcError::NotFound => ERR_NOT_FOUND,
            IpcError::Interrupted => ERR_INTERRUPTED,
            IpcError::Denied => crate::abi::ERR_DENIED,
        }
    }
}

/// Hamma foydalanuvçilar oça oladigan kanallar şu söz bilan boşlanadi.
pub const PUBLIC_PREFIX: &str = "ommaviy/";

/// Nomli kanalni topadi yoki yaratadi. Boşqa foydalanuvçining kanalini oçib bölmaydi.
pub fn open(name: &str, uid: u32) -> Result<ChanId, IpcError> {
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(IpcError::TooBig);
    }
    let mut ch = CHANNELS.lock();
    if let Some(i) = ch.iter().position(|c| c.name == name) {
        let c = &ch[i];
        if uid != 0 && c.owner != uid && !name.starts_with(PUBLIC_PREFIX) {
            return Err(IpcError::Denied);
        }
        return Ok(i);
    }
    if ch.len() >= MAX_CHANNELS {
        return Err(IpcError::Full);
    }
    ch.push(Channel { name: String::from(name), owner: uid, queue: VecDeque::new(), sent: 0 });
    Ok(ch.len() - 1)
}

/// Nomsiz kanal: uni faqat dastagi bor jarayon (masalan, bolaga berilgan) işlata oladi.
pub fn create_anonymous(uid: u32) -> Result<ChanId, IpcError> {
    let mut ch = CHANNELS.lock();
    if ch.len() >= MAX_CHANNELS {
        return Err(IpcError::Full);
    }
    ch.push(Channel { name: String::new(), owner: uid, queue: VecDeque::new(), sent: 0 });
    Ok(ch.len() - 1)
}

/// Xabarni navbatga qöyadi va kutayotganlarni uyğotadi.
pub fn send(id: ChanId, data: &[u8]) -> Result<(), IpcError> {
    if data.len() > MAX_MESSAGE {
        return Err(IpcError::TooBig);
    }
    {
        let mut ch = CHANNELS.lock();
        let c = ch.get_mut(id).ok_or(IpcError::NotFound)?;
        if c.queue.len() >= MAX_QUEUE {
            return Err(IpcError::Full);
        }
        c.queue.push_back(data.to_vec());
        c.sent += 1;
    }
    crate::task::wake_channel(id);
    Ok(())
}

/// Navbatdagi xabarni oladi (bölmasa — `None`). Xabar `cap` dan katta bölsa,
/// u navbatda qoladi va `TooBig` qaytariladi.
pub fn try_recv(id: ChanId, cap: usize) -> Result<Option<Vec<u8>>, IpcError> {
    let mut ch = CHANNELS.lock();
    let c = ch.get_mut(id).ok_or(IpcError::NotFound)?;
    match c.queue.front() {
        None => Ok(None),
        Some(m) if m.len() > cap => Err(IpcError::TooBig),
        Some(_) => Ok(c.queue.pop_front()),
    }
}

/// Xabar kelgunça kutadi.
pub fn recv(id: ChanId, cap: usize) -> Result<Vec<u8>, IpcError> {
    loop {
        if let Some(m) = try_recv(id, cap)? {
            return Ok(m);
        }
        if crate::task::current_killed() {
            return Err(IpcError::Interrupted);
        }
        crate::task::wait_channel(id, || CHANNELS.lock().get(id).is_some_and(|c| !c.queue.is_empty()));
    }
}

/// `kanallar` buyruği uçun: (nom, navbatdagi xabarlar, jami yuborilgan)
pub fn list() -> Vec<(String, usize, u64)> {
    CHANNELS
        .lock()
        .iter()
        .map(|c| (if c.name.is_empty() { String::from("(nomsiz)") } else { c.name.clone() }, c.queue.len(), c.sent))
        .collect()
}
