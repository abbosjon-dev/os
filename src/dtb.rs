//! Flattened Device Tree (DTB) o'quvchisi — noldan yozilgan.
//! QEMU yalang'och (bare-metal) ELF yadro uchun DTB ni RAM boshiga qo'yadi.
//! Biz undan faqat RAM hajmini o'qiymiz.

use core::ffi::CStr;

const FDT_MAGIC: u32 = 0xd00d_feed;
const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_NOP: u32 = 4;
const FDT_END: u32 = 9;

const MAX_DEPTH: usize = 16;

#[derive(Clone, Copy)]
pub struct Fdt {
    base: *const u8,
    total_size: usize,
    struct_off: usize,
    struct_size: usize,
    strings_off: usize,
}

fn be32(p: *const u8) -> u32 {
    u32::from_be_bytes(unsafe { p.cast::<[u8; 4]>().read_unaligned() })
}

fn be_cells(bytes: &[u8], cells: usize) -> u64 {
    bytes[..cells * 4].iter().fold(0u64, |acc, &b| (acc << 8) | b as u64)
}

const fn align4(x: usize) -> usize {
    (x + 3) & !3
}

impl Fdt {
    /// # Safety
    /// `addr` o'qilishi mumkin bo'lgan xotiraga ishora qilishi kerak.
    pub unsafe fn from_addr(addr: usize) -> Option<Self> {
        let base = addr as *const u8;
        let h = |i: usize| be32(unsafe { base.add(i * 4) });
        if h(0) != FDT_MAGIC {
            return None;
        }
        Some(Self {
            base,
            total_size: h(1) as usize,
            struct_off: h(2) as usize,
            strings_off: h(3) as usize,
            struct_size: h(9) as usize,
        })
    }

    pub fn total_size(&self) -> usize {
        self.total_size
    }

    fn string_at(&self, off: usize) -> &'static str {
        unsafe { CStr::from_ptr(self.base.add(self.strings_off + off).cast()) }
            .to_str()
            .unwrap_or("?")
    }

    /// Daraxtdagi har bir xususiyat (property) uchun `f(chuqurlik, tugun_nomi, xususiyat, qiymat)`.
    pub fn for_each_prop(&self, mut f: impl FnMut(usize, &str, &str, &'static [u8])) {
        let mut names: [&str; MAX_DEPTH] = [""; MAX_DEPTH];
        let mut depth = 0usize;
        let mut p = unsafe { self.base.add(self.struct_off) };
        let end = unsafe { p.add(self.struct_size) };

        while p < end {
            let token = be32(p);
            p = unsafe { p.add(4) };
            match token {
                FDT_BEGIN_NODE => {
                    let name = unsafe { CStr::from_ptr(p.cast()) };
                    let len = name.to_bytes().len();
                    if depth < MAX_DEPTH {
                        names[depth] = name.to_str().unwrap_or("?");
                    }
                    depth += 1;
                    p = unsafe { p.add(align4(len + 1)) };
                }
                FDT_END_NODE => depth = depth.saturating_sub(1),
                FDT_PROP => {
                    let len = be32(p) as usize;
                    let nameoff = be32(unsafe { p.add(4) }) as usize;
                    let value = unsafe { core::slice::from_raw_parts(p.add(8), len) };
                    let node = if depth > 0 && depth <= MAX_DEPTH { names[depth - 1] } else { "" };
                    f(depth, node, self.string_at(nameoff), value);
                    p = unsafe { p.add(8 + align4(len)) };
                }
                FDT_NOP => {}
                FDT_END => break,
                _ => break, // buzilgan DTB
            }
        }
    }

    /// Birinchi `memory` tugunidan (boshlanish, hajm).
    pub fn memory(&self) -> Option<(usize, usize)> {
        let (mut addr_cells, mut size_cells) = (2, 1);
        let mut result = None;
        self.for_each_prop(|depth, node, prop, val| {
            if depth == 1 && prop == "#address-cells" {
                addr_cells = be_cells(val, 1) as usize;
            } else if depth == 1 && prop == "#size-cells" {
                size_cells = be_cells(val, 1) as usize;
            } else if depth == 2
                && node.starts_with("memory")
                && prop == "reg"
                && result.is_none()
                && val.len() >= (addr_cells + size_cells) * 4
            {
                let base = be_cells(val, addr_cells) as usize;
                let size = be_cells(&val[addr_cells * 4..], size_cells) as usize;
                result = Some((base, size));
            }
        });
        result
    }
}
