//! Uzilişlarni tarqatiş: GIC dan raqamni olib, tegişli drayverga yuboradi.

use crate::{gic, timer, uart};

/// "Qayta rejalaştir" — boşqa yadroga yangi vazifa tayyorligini bildiradi.
pub const SGI_RESCHEDULE: u32 = 1;

/// SGI qabul qilgan yadrolar (bit = yadro raqami) — self-test uçun.
static SGI_SEEN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// SGI belgilarini tozalaydi va hozirgiça qabul qilganlar niqobini qaytaradi.
pub fn take_sgi_seen() -> u64 {
    SGI_SEEN.swap(0, core::sync::atomic::Ordering::AcqRel)
}

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
            id if id == timer::irq() => timer::handle_irq(),
            id if id == uart::irq() => uart::handle_irq(),
            SGI_RESCHEDULE => {
                // pastda preempt() — yangi vazifani olamiz
                let bit = 1u64 << (crate::task::cpu_id() % 64);
                SGI_SEEN.fetch_or(bit, core::sync::atomic::Ordering::Relaxed);
            }
            // virtio-pci INTx liniyalari umumiy bölişi mumkin — ikkala drayver ham tekşiriladi
            id if id == crate::drivers::virtio_blk::irq() || (id == crate::drivers::virtio_net::irq() && id != 0) => {
                if id == crate::drivers::virtio_blk::irq() {
                    crate::drivers::virtio_blk::handle_irq();
                }
                if id == crate::drivers::virtio_net::irq() {
                    crate::drivers::virtio_net::handle_irq();
                }
            }
            id if id == crate::drivers::pwrbtn::irq() && id != 0 => crate::drivers::pwrbtn::handle_irq(),
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
