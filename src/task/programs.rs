//! Yadro bilan birga keladigan foydalanuvçi dasturlari (build.rs yaratadi).

include!(concat!(env!("OUT_DIR"), "/programs.rs"));

pub fn find(name: &str) -> Option<&'static [u8]> {
    PROGRAMS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}
