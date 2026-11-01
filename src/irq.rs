//! Uzilişlarni tarqatiş: GIC dan raqamni olib, tegişli drayverga yuboradi.

use crate::{gic, timer, uart};

/// "Qayta rejalaştir" — boşqa yadroga yangi vazifa tayyorligini bildiradi.
pub const SGI_RESCHEDULE: u32 = 1;

pub fn init() {
    gic::init();
    gic::enable(SGI_RESCHEDULE);
    timer::init();
    uart::enable_rx_irq();
    gic::irqs_enable();
}

/// Ikkilamçi yadro: faqat öziga xos qismlar (CPU interfeysi, taymer, SGI).
pub fn init_secondary() {
    gic::init_cpu();
    gic::enable(SGI_RESCHEDULE);
    timer::init();
}

pub fn handle() {
    let mut handled = false;
    loop {
        let iar = gic::acknowledge();
        let id = iar & 0x3ff;
        if id >= gic::SPURIOUS {
            break;
        }
        match id {
            timer::IRQ => timer::handle_irq(),
            uart::IRQ => uart::handle_irq(),
            SGI_RESCHEDULE => {} // pastda preempt() — yangi vazifani olamiz
            id if id == crate::drivers::virtio_blk::irq() => crate::drivers::virtio_blk::handle_irq(),
            id if id == crate::drivers::virtio_net::irq() && id != 0 => crate::drivers::virtio_net::handle_irq(),
            _ => crate::tprintln!(crate::i18n::msg::IRQ_UNKNOWN, id),
        }
        gic::end_of_interrupt(iar);
        handled = true;
    }
    // Uziliş tasdiqlangandan keyin (EOI) boşqa vazifaga ötiş mumkin.
    if handled {
        crate::task::preempt();
    }
}
