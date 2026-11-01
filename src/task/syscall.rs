//! Tizim çaqiruvlarini bajariş (`svc #0` → bu yerga keladi).
//!
//! Foydalanuvçi xotirasi faqat `uaccess` orqali: tekşiriladi va nusxalanadi.
//! Notöğri manzil — `ERR_BAD_ADDRESS`, yadro unga tegmaydi.

use crate::abi::*;
use crate::exceptions::TrapFrame;
use crate::task::uaccess::{check_writable, copy_from_user, copy_to_user, user_string};
use crate::task::{self, Handle, SpawnError};
use crate::{fs, i18n, ipc, timer, uart};
use alloc::string::String;

const MAX_PATH: usize = 1024;

type SysResult = Result<i64, i64>;

fn user_str(addr: u64, len: u64) -> Result<String, i64> {
    user_string(addr, len, MAX_PATH)
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
            let bytes = copy_from_user(a[0], a[1])?;
            uart::write_bytes(&bytes);
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
            let args = user_string(a[2], a[3], task::MAX_ARGS)?;
            let image = fs::read_all(&path).map_err(fs_err)?;
            let name = path.rsplit('/').next().unwrap_or(&path);
            let pid = task::spawn_user(name, &image, &args, false, task::current_pid()).map_err(|e| match e {
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
            let node = fs::open(&path, a[2]).map_err(fs_err)?;
            let h = Handle::File { node, offset: 0, append: a[2] & OPEN_APPEND != 0 };
            task::add_handle(h).map(|i| i as i64).ok_or(ERR_FULL)
        }
        SYS_READ => {
            check_writable(a[1], a[2])?;
            let mut buf = alloc::vec![0u8; (a[2] as usize).min(task::uaccess::MAX_COPY)];
            match task::get_handle(a[0] as usize) {
                Some(Handle::File { node, offset, append }) => {
                    let n = fs::read(node, offset, &mut buf).map_err(fs_err)?;
                    copy_to_user(a[1], &buf[..n])?;
                    task::set_handle(a[0] as usize, Handle::File { node, offset: offset + n, append });
                    Ok(n as i64)
                }
                _ => Err(ERR_BAD_HANDLE),
            }
        }
        SYS_FWRITE => {
            let data = copy_from_user(a[1], a[2])?;
            match task::get_handle(a[0] as usize) {
                Some(Handle::File { node, offset, append }) => {
                    let n = fs::write(node, if append { None } else { Some(offset) }, &data).map_err(fs_err)?;
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
            let mut out = String::new();
            for e in fs::list(&path).map_err(fs_err)? {
                out += &e.name;
                out += if e.is_dir { "/\n" } else { "\n" };
            }
            if out.len() as u64 > a[3] {
                return Err(ERR_TOO_BIG);
            }
            copy_to_user(a[2], out.as_bytes())?;
            Ok(out.len() as i64)
        }
        SYS_MKDIR => {
            fs::mkdir(&user_str(a[0], a[1])?).map_err(fs_err)?;
            Ok(0)
        }
        SYS_REMOVE => {
            fs::remove(&user_str(a[0], a[1])?).map_err(fs_err)?;
            Ok(0)
        }

        // --- Zamin kanallari ---
        SYS_CHAN_OPEN => {
            let name = user_str(a[0], a[1])?;
            let id = ipc::open(&name).map_err(|e| e.code())?;
            task::add_handle(Handle::Channel(id)).map(|i| i as i64).ok_or(ERR_FULL)
        }
        SYS_CHAN_SEND => {
            let data = copy_from_user(a[1], a[2])?;
            match task::get_handle(a[0] as usize) {
                Some(Handle::Channel(id)) => ipc::send(id, &data).map(|_| 0).map_err(|e| e.code()),
                _ => Err(ERR_BAD_HANDLE),
            }
        }
        SYS_CHAN_RECV => {
            check_writable(a[1], a[2])?;
            match task::get_handle(a[0] as usize) {
                Some(Handle::Channel(id)) => {
                    let msg = ipc::recv(id, a[2] as usize).map_err(|e| e.code())?;
                    copy_to_user(a[1], &msg)?;
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
