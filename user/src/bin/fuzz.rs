//! fuzz — tizim çaqiruvlariga köplab tasodifiy argumentlar yuboradi.
//! Maqsad: yadro heç qaçon qulamasligi va foydalanuvçi manzillari tekşiruvidan ötişi.
//! Dastur özi tugasa (qulamasa) — yadro barqaror. `fuzz [son]`.
#![no_std]
#![no_main]

use core::arch::asm;

zamin::entry!(main);

/// Oddiy, taxmin qilsa böladigan tasodifiy generator (urüğ bilan).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

unsafe fn raw_syscall(num: u64, a: [u64; 6]) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "svc #0",
            inlateout("x0") a[0] as i64 => ret,
            in("x1") a[1], in("x2") a[2], in("x3") a[3], in("x4") a[4], in("x5") a[5], in("x8") num,
            options(nostack),
        );
    }
    ret
}

fn main() -> i64 {
    let n: u64 = zamin::args().trim().parse().unwrap_or(20_000);
    let mut rng = Rng(0x5eed_1234_abcd_0001 ^ zamin::uptime_ms());
    // Maʼlum manzillar: yadro, xaritalanmagan, öz stekimiz, kod
    let probes = [0u64, 0x4020_0000, 0x5000_0000_0000, main as *const () as u64, &n as *const _ as u64, 0xffff_ffff_ffff_fff0];
    for _ in 0..n {
        // Çaqiruv raqami: köpinça haqiqiy (0..36), baʼzan tasodifiy
        let num = if rng.next() & 3 == 0 { rng.next() % 64 } else { rng.next() % 36 };
        use zamin::abi::*;
        // Özini tugatadigan, çeksiz kutadigan yoki köpaytiradigan çaqiruvlarni taşlaymiz:
        // ularda "xato" yöq, lekin dasturni tugatib, sinovni buzadi.
        // Özini tugatadigan yoki uzoq kutadigan (tarmoq, kiriş) çaqiruvlarni taşlaymiz.
        if matches!(num, SYS_EXIT | SYS_THREAD_EXIT | SYS_SPAWN | SYS_THREAD_CREATE | SYS_FUTEX_WAIT | SYS_CHAN_RECV | SYS_READ | SYS_SLEEP | SYS_YIELD | SYS_WAIT | SYS_THREAD_JOIN | SYS_DNS | SYS_PING | SYS_TCP_CONNECT | SYS_TCP_RECV) {
            continue;
        }
        let mut a = [0u64; 6];
        for v in a.iter_mut() {
            *v = if rng.next() & 1 == 0 { probes[(rng.next() % probes.len() as u64) as usize] } else { rng.next() };
        }
        let _ = unsafe { raw_syscall(num, a) };
    }
    zamin::println!("  fuzz: {} {}", n, zamin::tr(["çaqiruv — yadro tirik", "вызовов — ядро живо", "calls — kernel alive"]));
    0
}
