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

/// Fayl yöli: nisbiy bölsa — jarayonning joriy papkasiga nisbatan.
/// Qafasdagi jarayon uçun yöl qafas papkasi içida bölişi şart.
fn user_path(addr: u64, len: u64) -> Result<String, i64> {
    let p = user_str(addr, len)?;
    let full = if p.starts_with('/') || p.starts_with('~') {
        p
    } else {
        let cwd = task::cwd();
        alloc::format!("{}/{}", cwd.trim_end_matches('/'), p)
    };
    if let Some(jail) = task::jail() {
        if !fs::path_inside(&full, jail) {
            return Err(ERR_DENIED);
        }
    }
    Ok(full)
}

fn fs_err(e: fs::FsError) -> i64 {
    e.code()
}

pub fn dispatch(frame: &mut TrapFrame) {
    let num = frame.x[8];
    let a = [frame.x[0], frame.x[1], frame.x[2], frame.x[3], frame.x[4], frame.x[5]];
    // Tizim çaqiruvi davomida uzilişlar yoqiq: uzoq amallar (disk, katta fayllar)
    // paytida ham taymer işlaydi va jarayonni almaştiriş mumkin.
    unsafe { core::arch::asm!("msr daifclr, #2", options(nomem, nostack)) };
    let ret = match handle(num, a) {
        Ok(v) | Err(v) => v,
    };
    unsafe { core::arch::asm!("msr daifset, #2", options(nomem, nostack)) };
    frame.x[0] = ret as u64;
}

/// Dastakka yozadi: konsol, quvur yoki fayl.
fn write_handle(h: usize, data: &[u8]) -> SysResult {
    match task::get_handle(h) {
        Some(Handle::Console) => {
            uart::write_bytes(data);
            Ok(data.len() as i64)
        }
        Some(Handle::PipeWrite(id)) => crate::pipe::write(id, data).map(|n| n as i64),
        Some(Handle::File { node, offset, append, writable }) => {
            if !writable {
                return Err(ERR_DENIED); // faqat öqiş uçun oçilgan
            }
            let n = fs::write(node, if append { None } else { Some(offset) }, data).map_err(fs_err)?;
            task::set_handle(h, Handle::File { node, offset: offset + n, append, writable });
            Ok(n as i64)
        }
        _ => Err(ERR_BAD_HANDLE),
    }
}

/// Dastakdan öqiydi: konsol (bitta qator), quvur yoki fayl.
fn read_handle(h: usize, max: usize) -> Result<alloc::vec::Vec<u8>, i64> {
    match task::get_handle(h) {
        Some(Handle::Console) => Ok(match crate::line::read_line_eof(false) {
            Some(mut line) => {
                line.push('\n');
                let mut v = line.into_bytes();
                v.truncate(max);
                v
            }
            None => alloc::vec::Vec::new(), // Ctrl+D — kiriş oxiri
        }),
        Some(Handle::PipeRead(id)) => Ok(crate::pipe::read(id, max)),
        Some(Handle::File { node, offset, append, writable }) => {
            let mut buf = alloc::vec![0u8; max];
            let n = fs::read(node, offset, &mut buf).map_err(fs_err)?;
            buf.truncate(n);
            task::set_handle(h, Handle::File { node, offset: offset + n, append, writable });
            Ok(buf)
        }
        _ => Err(ERR_BAD_HANDLE),
    }
}

fn handle(num: u64, a: [u64; 6]) -> SysResult {
    match num {
        SYS_EXIT => task::exit_current(a[0] as i64),
        SYS_WRITE => {
            let bytes = copy_from_user(a[0], a[1])?;
            write_handle(STDOUT as usize, &bytes)
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
            if task::jail().is_some() {
                return Err(ERR_DENIED); // qafasdagi jarayon dastur işga tuşira olmaydi
            }
            let path = user_path(a[0], a[1])?;
            let args = user_string(a[2], a[3], task::MAX_ARGS)?;
            let image = fs::read_program(&path).map_err(fs_err)?;
            let name = path.rsplit('/').next().unwrap_or(&path);
            // Standart dastaklar: x4 dagi raqamlar (INHERIT — otaning şu raqamli dastagi)
            let mut stdio = [Handle::Console; 3];
            for (k, slot) in stdio.iter_mut().enumerate() {
                let idx = (a[4] >> (16 * k)) & 0xffff;
                let idx = if idx == INHERIT { k as u64 } else { idx };
                *slot = task::get_handle(idx as usize).ok_or(ERR_BAD_HANDLE)?;
            }
            let opts = task::SpawnOptions {
                detached: false,
                parent: task::current_pid(),
                stdio,
                env: task::current_env(),
                trusted: false, // işonç bolaga ötmaydi
                cwd: task::cwd(),
                uid: task::current_uid(), // bola ota bilan bir xil foydalanuvçi
                // Qafas: bola faqat otaning joriy papkasi içida işlaydi
                jail: if a[5] & SPAWN_SANDBOX != 0 {
                    Some(fs::node_of(&task::cwd()).map_err(fs_err)?)
                } else {
                    None
                },
                extra: match a[5] & 0xffff {
                    INHERIT => None,
                    h => Some(task::get_handle(h as usize).ok_or(ERR_BAD_HANDLE)?),
                },
            };
            let pid = task::spawn_user(name, &image, &args, opts).map_err(|e| match e {
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
            if a[1] == 1 {
                return match task::try_wait(pid) {
                    Some(Some(code)) => Ok(code),
                    Some(None) => Err(ERR_RUNNING),
                    None => Err(ERR_NOT_FOUND),
                };
            }
            task::wait(pid).ok_or(ERR_NOT_FOUND)
        }

        // --- Fayllar ---
        SYS_OPEN => {
            let path = user_path(a[0], a[1])?;
            let node = fs::open(&path, a[2]).map_err(fs_err)?;
            let writable = a[2] & (OPEN_CREATE | OPEN_TRUNCATE | OPEN_APPEND | OPEN_WRITE) != 0;
            let h = Handle::File { node, offset: 0, append: a[2] & OPEN_APPEND != 0, writable };
            task::add_handle(h).map(|i| i as i64).ok_or(ERR_FULL)
        }
        SYS_READ => {
            check_writable(a[1], a[2])?;
            let data = read_handle(a[0] as usize, (a[2] as usize).min(task::uaccess::MAX_COPY))?;
            copy_to_user(a[1], &data)?;
            Ok(data.len() as i64)
        }
        SYS_FWRITE => {
            let data = copy_from_user(a[1], a[2])?;
            write_handle(a[0] as usize, &data)
        }
        SYS_CLOSE => {
            if task::close_handle(a[0] as usize) {
                Ok(0)
            } else {
                Err(ERR_BAD_HANDLE)
            }
        }
        SYS_READDIR => {
            let path = user_path(a[0], a[1])?;
            let mut out = String::new();
            for e in fs::list(&path).map_err(fs_err)? {
                out += &e.name;
                out += if e.is_dir { "/" } else { "" };
                // nom \t hajm \t ruxsat (sakkizlik) \t egasi \t özgartirilgan vaqt
                out += &alloc::format!(
                    "\t{}\t{:o}\t{}\t{}\n",
                    e.size,
                    e.mode,
                    crate::accounts::username_of(e.owner),
                    e.mtime
                );
            }
            if out.len() as u64 > a[3] {
                return Err(ERR_TOO_BIG);
            }
            copy_to_user(a[2], out.as_bytes())?;
            Ok(out.len() as i64)
        }
        SYS_MKDIR => {
            fs::mkdir(&user_path(a[0], a[1])?).map_err(fs_err)?;
            Ok(0)
        }
        SYS_REMOVE => {
            fs::remove(&user_path(a[0], a[1])?).map_err(fs_err)?;
            Ok(0)
        }

        // --- Zamin kanallari ---
        SYS_CHAN_OPEN => {
            if task::jail().is_some() {
                return Err(ERR_DENIED); // qafasda faqat berilgan (nomsiz) kanallar
            }
            let name = user_str(a[0], a[1])?;
            let id = ipc::open(&name, task::current_uid()).map_err(|e| e.code())?;
            task::add_handle(Handle::Channel(id)).map(|i| i as i64).ok_or(ERR_FULL)
        }
        SYS_CHAN_CREATE => {
            let id = ipc::create_anonymous(task::current_uid()).map_err(|e| e.code())?;
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

        // --- Kiriş/çiqiş, muhit, terminal ---
        SYS_PIPE => {
            check_writable(a[0], 16)?;
            let id = crate::pipe::create();
            let r = task::add_handle(Handle::PipeRead(id));
            let w = task::add_handle(Handle::PipeWrite(id));
            let (Some(r), Some(w)) = (r, w) else {
                crate::pipe::release(id, false);
                crate::pipe::release(id, true);
                return Err(ERR_FULL);
            };
            let mut out = [0u8; 16];
            out[..8].copy_from_slice(&(r as u64).to_le_bytes());
            out[8..].copy_from_slice(&(w as u64).to_le_bytes());
            copy_to_user(a[0], &out)?;
            Ok(0)
        }
        SYS_SETENV => {
            let name = user_str(a[0], a[1])?;
            let value = user_str(a[2], a[3])?;
            if name.is_empty() || name.contains('=') {
                return Err(ERR_INVALID);
            }
            task::set_env(&name, &value);
            Ok(0)
        }
        SYS_GETENV => {
            let name = user_str(a[0], a[1])?;
            let value = task::get_env(&name).ok_or(ERR_NOT_FOUND)?;
            if value.len() as u64 > a[3] {
                return Err(ERR_TOO_BIG);
            }
            copy_to_user(a[2], value.as_bytes())?;
            Ok(value.len() as i64)
        }
        SYS_FOREGROUND => {
            if !task::is_trusted() {
                return Err(ERR_DENIED);
            }
            task::set_foreground(a[0] as usize);
            Ok(0)
        }
        // --- Oqimlar ---
        SYS_THREAD_CREATE => {
            let (entry, stack) = (a[0] as usize, a[1] as usize);
            // Kiriş va stek joriy jarayonning foydalanuvçi xotirasida bölişi kerak
            if !task::user_range_ok(entry, 4, false) || !task::user_range_ok(stack - 16, 16, true) {
                return Err(ERR_BAD_ADDRESS);
            }
            task::spawn_thread(entry, stack, a[2]).map(|t| t as i64).map_err(|_| ERR_NO_MEMORY)
        }
        SYS_THREAD_EXIT => task::exit_thread(a[0] as i64),
        SYS_THREAD_JOIN => {
            let tid = a[0] as usize;
            if task::parent_of(tid) != Some(task::current_pid()) || tid == task::current_tid() {
                return Err(ERR_NOT_CHILD);
            }
            task::wait(tid).ok_or(ERR_NOT_FOUND)
        }
        SYS_FUTEX_WAIT => {
            let (addr, expected) = (a[0], a[1] as u32);
            if addr % 4 != 0 {
                return Err(ERR_INVALID);
            }
            check_writable(addr, 4)?;
            let key = task::futex_key(addr as usize);
            task::wait_event(key, || task::uaccess::read_u32_checked_before(addr) == expected);
            Ok(0)
        }
        SYS_FUTEX_WAKE => {
            task::wake_event(task::futex_key(a[0] as usize));
            Ok(0)
        }
        SYS_TID => Ok(task::current_tid() as i64),
        SYS_TIME => Ok(crate::timer::unix_time() as i64),
        SYS_TZ => Ok(crate::time::tz_minutes() as i64),
        SYS_CHMOD => {
            let path = user_path(a[0], a[1])?;
            fs::chmod(&path, a[2] as u16).map_err(fs_err)?;
            Ok(0)
        }
        SYS_CHDIR => {
            let path = user_path(a[0], a[1])?;
            fs::list(&path).map_err(fs_err)?; // mavjud papka bölişi kerak
            task::set_cwd(&path);
            Ok(0)
        }
        SYS_ADMIN => {
            // Tizim buyruqlari (parol, tiklaş, öçir...) — faqat işonçli terminal uçun
            if !task::is_trusted() {
                return Err(ERR_DENIED);
            }
            let line = user_str(a[0], a[1])?;
            crate::shell::admin(&line);
            Ok(0)
        }

        _ => Err(ERR_UNKNOWN_SYSCALL), // nomaʼlum çaqiruv — dastur xatoni özi qayta işlaydi
    }
}
