//! Tizim çaqiruvlarini bajariş (`svc #0` → bu yerga keladi).
//!
//! Foydalanuvçi bergan har bir manzil işlatilişidan oldin tekşiriladi:
//! u joriy jarayonning öz sahifalarida bölişi (yozilsa — yoziş ruxsati bölişi) şart.
//! Aks holda yadro unga tegmaydi va `ERR_BAD_ADDRESS` qaytaradi.

use crate::abi::*;
use crate::exceptions::TrapFrame;
use crate::task::{self, Handle, SpawnError};
use crate::{fs, i18n, ipc, timer, uart};
use alloc::string::String;

const MAX_PATH: usize = 1024;

type SysResult = Result<i64, i64>;

fn user_bytes<'a>(addr: u64, len: u64) -> Result<&'a [u8], i64> {
    if len == 0 {
        return Ok(&[]);
    }
    if !task::user_range_ok(addr as usize, len as usize, false) {
        return Err(ERR_BAD_ADDRESS);
    }
    // Tekşirildi: joriy jarayonning öz sahifalari (TTBR0 şu jarayonniki).
    Ok(unsafe { core::slice::from_raw_parts(addr as *const u8, len as usize) })
}

fn user_bytes_mut<'a>(addr: u64, len: u64) -> Result<&'a mut [u8], i64> {
    if len == 0 {
        return Ok(&mut []);
    }
    if !task::user_range_ok(addr as usize, len as usize, true) {
        return Err(ERR_BAD_ADDRESS);
    }
    Ok(unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len as usize) })
}

fn user_str<'a>(addr: u64, len: u64) -> Result<&'a str, i64> {
    if len as usize > MAX_PATH {
        return Err(ERR_TOO_BIG);
    }
    core::str::from_utf8(user_bytes(addr, len)?).map_err(|_| ERR_INVALID)
}

fn fs_err(e: fs::FsError) -> i64 {
    e.code()
}

pub fn dispatch(frame: &mut TrapFrame) {
    let num = frame.x[8];
    let a = [frame.x[0], frame.x[1], frame.x[2], frame.x[3]];
    let ret = match handle(num, a) {
        Ok(v) | Err(v) => v,
    };
    frame.x[0] = ret as u64;
}

fn handle(num: u64, a: [u64; 4]) -> SysResult {
    match num {
        SYS_EXIT => task::exit_current(a[0] as i64),
        SYS_WRITE => {
            let bytes = user_bytes(a[0], a[1])?;
            uart::write_bytes(bytes);
            Ok(bytes.len() as i64)
        }
        SYS_YIELD => {
            task::yield_now();
            Ok(0)
        }
        SYS_SLEEP => {
            task::sleep_ms(a[0]);
            Ok(0)
        }
        SYS_UPTIME => Ok(timer::uptime_ms() as i64),
        SYS_PID => Ok(task::current_pid() as i64),
        SYS_LANG => Ok(i18n::current() as i64),
        SYS_MAP => task::map_memory(a[0] as usize).map(|v| v as i64).ok_or(ERR_NO_MEMORY),
        SYS_SPAWN => {
            let path = user_str(a[0], a[1])?;
            let args = user_str(a[2], a[3])?;
            let image = fs::read_all(path).map_err(fs_err)?;
            let name = path.rsplit('/').next().unwrap_or(path);
            let pid = task::spawn_user(name, &image, args, false, task::current_pid()).map_err(|e| match e {
                SpawnError::OutOfMemory => ERR_NO_MEMORY,
                SpawnError::TooManyTasks => ERR_FULL,
                SpawnError::ArgsTooLong => ERR_TOO_BIG,
                SpawnError::BadImage(_) => ERR_INVALID,
            })?;
            Ok(pid as i64)
        }
        SYS_WAIT => {
            let pid = a[0] as usize;
            if task::parent_of(pid) != Some(task::current_pid()) {
                return Err(ERR_NOT_CHILD);
            }
            task::wait(pid).ok_or(ERR_NOT_FOUND)
        }

        // --- Fayllar ---
        SYS_OPEN => {
            let path = user_str(a[0], a[1])?;
            let node = fs::open(path, a[2]).map_err(fs_err)?;
            let h = Handle::File { node, offset: 0, append: a[2] & OPEN_APPEND != 0 };
            task::add_handle(h).map(|i| i as i64).ok_or(ERR_FULL)
        }
        SYS_READ => {
            let buf = user_bytes_mut(a[1], a[2])?;
            match task::get_handle(a[0] as usize) {
                Some(Handle::File { node, offset, append }) => {
                    let n = fs::read(node, offset, buf).map_err(fs_err)?;
                    task::set_handle(a[0] as usize, Handle::File { node, offset: offset + n, append });
                    Ok(n as i64)
                }
                _ => Err(ERR_BAD_HANDLE),
            }
        }
        SYS_FWRITE => {
            let data = user_bytes(a[1], a[2])?;
            match task::get_handle(a[0] as usize) {
                Some(Handle::File { node, offset, append }) => {
                    let n = fs::write(node, if append { None } else { Some(offset) }, data).map_err(fs_err)?;
                    task::set_handle(a[0] as usize, Handle::File { node, offset: offset + n, append });
                    Ok(n as i64)
                }
                _ => Err(ERR_BAD_HANDLE),
            }
        }
        SYS_CLOSE => {
            if task::close_handle(a[0] as usize) {
                Ok(0)
            } else {
                Err(ERR_BAD_HANDLE)
            }
        }
        SYS_READDIR => {
            let path = user_str(a[0], a[1])?;
            let buf = user_bytes_mut(a[2], a[3])?;
            let mut out = String::new();
            for e in fs::list(path).map_err(fs_err)? {
                out += &e.name;
                out += if e.is_dir { "/\n" } else { "\n" };
            }
            if out.len() > buf.len() {
                return Err(ERR_TOO_BIG);
            }
            buf[..out.len()].copy_from_slice(out.as_bytes());
            Ok(out.len() as i64)
        }
        SYS_MKDIR => {
            fs::mkdir(user_str(a[0], a[1])?).map_err(fs_err)?;
            Ok(0)
        }
        SYS_REMOVE => {
            fs::remove(user_str(a[0], a[1])?).map_err(fs_err)?;
            Ok(0)
        }

        // --- Zamin kanallari ---
        SYS_CHAN_OPEN => {
            let name = user_str(a[0], a[1])?;
            let id = ipc::open(name).map_err(|e| e.code())?;
            task::add_handle(Handle::Channel(id)).map(|i| i as i64).ok_or(ERR_FULL)
        }
        SYS_CHAN_SEND => {
            let data = user_bytes(a[1], a[2])?;
            match task::get_handle(a[0] as usize) {
                Some(Handle::Channel(id)) => ipc::send(id, data).map(|_| 0).map_err(|e| e.code()),
                _ => Err(ERR_BAD_HANDLE),
            }
        }
        SYS_CHAN_RECV => {
            let buf = user_bytes_mut(a[1], a[2])?;
            match task::get_handle(a[0] as usize) {
                Some(Handle::Channel(id)) => {
                    let msg = ipc::recv(id, buf.len()).map_err(|e| e.code())?;
                    buf[..msg.len()].copy_from_slice(&msg);
                    Ok(msg.len() as i64)
                }
                _ => Err(ERR_BAD_HANDLE),
            }
        }

        _ => {
            crate::tprintln!(i18n::msg::PROC_BAD_SYSCALL, task::current_pid(), num);
            Err(ERR_UNKNOWN_SYSCALL)
        }
    }
}
