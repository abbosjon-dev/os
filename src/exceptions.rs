//! Istisnolarni (exception) qayta işlaş.

use core::arch::asm;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapFrame {
    pub x: [u64; 31],
    pub elr: u64,
    pub spsr: u64,
    /// Foydalanuvçi steki (SP_EL0)
    pub sp_el0: u64,
}

unsafe extern "C" {
    static exception_vectors: u8;
}

pub fn init() {
    unsafe {
        let addr = &raw const exception_vectors as u64;
        asm!("msr vbar_el1, {}", "isb", in(reg) addr);
    }
}

const EC_SVC64: u64 = 0x15;
const EC_INSN_ABORT_LOWER: u64 = 0x20;
const EC_DATA_ABORT_LOWER: u64 = 0x24;

/// arch/vectors.s dagi `stack_overflow` tomonidan beriladi.
const KIND_STACK_OVERFLOW: u64 = 16;

/// Yadro steki qörğon sahifaga tegdi. Biz favqulodda stekdamiz.
fn stack_overflow(frame: &TrapFrame) -> ! {
    let far: u64;
    unsafe { asm!("mrs {}, far_el1", out(reg) far) };
    // Uzilişlar öçiq bölgan bölsa, oqim qulf uşlab turgan bölişi mumkin — uni
    // töxtatsak, qulf abadiy band qoladi. Bunday holatda tizimni töxtatiş xavfsizroq.
    let irqs_were_masked = frame.spsr & (1 << 7) != 0;
    if irqs_were_masked {
        panic!("yadro steki töldi (uzilişlar öçiq), ELR={:#x}, FAR={:#x}", frame.elr, far);
    }
    let pid = crate::task::current_pid();
    if pid == 0 {
        panic!("yuklaniş steki töldi, ELR={:#x}, FAR={:#x}", frame.elr, far);
    }
    crate::tprintln!(crate::i18n::msg::STACK_OVERFLOW, pid);
    crate::task::exit_current(-11)
}

const KIND_NAMES: [&str; 4] = ["Sync", "IRQ", "FIQ", "SError"];

#[unsafe(no_mangle)]
extern "C" fn handle_exception(frame: &mut TrapFrame, kind: u64) {
    if kind == KIND_STACK_OVERFLOW {
        stack_overflow(frame);
    }
    if kind % 4 == 1 {
        crate::irq::handle();
        if kind == 9 {
            crate::task::check_killed(); // uziliş foydalanuvçi rejimida kelgan — xavfsiz nuqta
        }
        return;
    }

    let esr: u64;
    let far: u64;
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) esr);
        asm!("mrs {}, far_el1", out(reg) far);
    }
    let ec = (esr >> 26) & 0x3f;

    // Foydalanuvçi rejimidan (EL0) kelgan sinxron istisno
    if kind == 8 {
        if ec == EC_SVC64 {
            crate::task::syscall::dispatch(frame);
            crate::task::check_killed(); // xavfsiz nuqta: foydalanuvçi rejimiga qaytişdan oldin
            return;
        }
        // Sahifa xatosi (translation fault) lazy heap maydonida bölsa — sahifani xaritalab, davom etamiz.
        if (ec == EC_DATA_ABORT_LOWER || ec == EC_INSN_ABORT_LOWER) && (esr & 0x3c) == 0x04 {
            if crate::task::handle_page_fault(far as usize) {
                return;
            }
        }
        // Boşqa har qanday xato (taqiqlangan xotira, nomaʼlum buyruq...) — jarayon tugatiladi.
        let addr = if ec == EC_DATA_ABORT_LOWER || ec == EC_INSN_ABORT_LOWER { far } else { frame.elr };
        crate::tprintln!(
            crate::i18n::msg::PROC_FAULT,
            crate::task::current_pid(),
            format_args!("{:#x}", addr),
            format_args!("{:#x}", ec)
        );
        crate::task::exit_current(-1);
    }

    match (kind % 4, ec) {
        // BRK — dasturiy breakpoint: xabar beramiz va keyingi instruksiyaga ötamiz.
        (0, 0x3c) => {
            crate::tprintln!(
                crate::i18n::msg::EXC_BRK,
                esr & 0xffff,
                format_args!("{:#x}", frame.elr)
            );
            frame.elr += 4;
        }
        _ => {
            panic!(
                "kutilmagan istisno: {} (vektor {}), EC={:#x}, ESR={:#x}, ELR={:#x}, FAR={:#x}",
                KIND_NAMES[(kind % 4) as usize],
                kind,
                ec,
                esr,
                frame.elr,
                far
            );
        }
    }
}
