//! Istisnolarni (exception) qayta ishlash.

use core::arch::asm;

#[repr(C)]
pub struct TrapFrame {
    pub x: [u64; 31],
    pub elr: u64,
    pub spsr: u64,
    _pad: u64,
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

const KIND_NAMES: [&str; 4] = ["Sync", "IRQ", "FIQ", "SError"];

#[unsafe(no_mangle)]
extern "C" fn handle_exception(frame: &mut TrapFrame, kind: u64) {
    let esr: u64;
    let far: u64;
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) esr);
        asm!("mrs {}, far_el1", out(reg) far);
    }
    let ec = (esr >> 26) & 0x3f;

    match (kind % 4, ec) {
        // BRK — dasturiy breakpoint: xabar beramiz va keyingi instruksiyaga o'tamiz.
        (0, 0x3c) => {
            crate::println!(
                "[exc] BRK #{} @ {:#x} — qayta ishlandi, davom etamiz",
                esr & 0xffff,
                frame.elr
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
