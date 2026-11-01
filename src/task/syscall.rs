//! Tizim çaqiruvlarini bajariş (`svc #0` → bu yerga keladi).

use crate::abi::*;
use crate::exceptions::TrapFrame;
use crate::{i18n, task, timer, uart};

pub fn dispatch(frame: &mut TrapFrame) {
    let num = frame.x[8];
    let (a0, a1) = (frame.x[0], frame.x[1]);
    let ret: i64 = match num {
        SYS_EXIT => task::exit_current(a0 as i64),
        SYS_WRITE => {
            let (addr, len) = (a0 as usize, a1 as usize);
            if len == 0 {
                0 // böş yozuv — manzil tekşirilmaydi
            } else if task::user_range_ok(addr, len) {
                // Manzil tekşirildi: joriy jarayonning öz sahifalari.
                let bytes = unsafe { core::slice::from_raw_parts(addr as *const u8, len) };
                uart::write_bytes(bytes);
                len as i64
            } else {
                ERR_BAD_ADDRESS
            }
        }
        SYS_YIELD => {
            task::yield_now();
            0
        }
        SYS_SLEEP => {
            task::sleep_ms(a0);
            0
        }
        SYS_UPTIME => timer::uptime_ms() as i64,
        SYS_PID => task::current_pid() as i64,
        SYS_LANG => i18n::current() as i64,
        _ => {
            crate::tprintln!(i18n::msg::PROC_BAD_SYSCALL, task::current_pid(), num);
            ERR_UNKNOWN_SYSCALL
        }
    };
    frame.x[0] = ret as u64;
}
