//! Uzilişlarni tarqatiş: GIC dan raqamni olib, tegişli drayverga yuboradi.

use crate::{gic, timer, uart};

pub fn init() {
    gic::init();
    timer::init();
    uart::enable_rx_irq();
    gic::irqs_enable();
}

pub fn handle() {
    loop {
        let id = gic::acknowledge();
        if id >= gic::SPURIOUS {
            break;
        }
        match id {
            timer::IRQ => timer::handle_irq(),
            uart::IRQ => uart::handle_irq(),
            _ => crate::tprintln!(crate::i18n::msg::IRQ_UNKNOWN, id),
        }
        gic::end_of_interrupt(id);
    }
}
