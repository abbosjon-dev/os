//! Flattened Device Tree (DTB) öquvçisi — noldan yozilgan.
//! QEMU yalangʼoç (bare-metal) ELF yadro uçun DTB ni RAM boşiga qöyadi.
//! Biz undan faqat RAM hajmini öqiymiz.

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
    /// `addr` öqilişi mumkin bölgan xotiraga işora qilişi kerak.
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

    /// Daraxtdagi har bir xususiyat (property) uçun `f(çuqurlik, tugun_nomi, xususiyat, qiymat)`.
    pub fn for_each_prop(&self, mut f: impl FnMut(usize, &'static str, &str, &'static [u8])) {
        let mut names: [&'static str; MAX_DEPTH] = [""; MAX_DEPTH];
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

    /// Birinçi `memory` tugunidan (boşlaniş, hajm).
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

    /// `/cpus/cpu@…` tugunlaridagi MPIDR lar (`reg`, `/cpus` ning #address-cells böyiça).
    /// Natija: (MPIDR lar, soni) — 8 tadan köpi tasdiqlanmaydi.
    pub fn cpus(&self) -> ([u64; 8], usize) {
        let (mut out, mut n, mut ac) = ([0u64; 8], 0usize, 1usize);
        self.for_each_prop(|depth, node, prop, val| {
            if depth == 2 && node == "cpus" && prop == "#address-cells" && val.len() >= 4 {
                ac = (be_cells(val, 1) as usize).clamp(1, 2);
            } else if depth == 3 && node.starts_with("cpu@") && prop == "reg" && val.len() >= ac * 4 && n < 8 {
                out[n] = be_cells(val, ac);
                n += 1;
            }
        });
        (out, n)
    }

    /// Yuqori darajadagi (`/nom...`) tugunning xususiyati.
    pub fn top_level_prop(&self, node_prefix: &str, prop: &str) -> Option<&'static [u8]> {
        let mut found = None;
        self.for_each_prop(|depth, node, p, val| {
            if found.is_none() && depth == 2 && node.starts_with(node_prefix) && p == prop {
                found = Some(val);
            }
        });
        found
    }
}

impl Fdt {
    /// `/gpio-keys` (compatible "gpio-keys") içidagi quvvat tugmasi (`linux,code` = KEY_POWER,
    /// 116): `gpios = <&gpio PIN bayroq>` dagi PIN. QEMU virt: PL061 ning 3-oyoği.
    pub fn gpio_key_power(&self) -> Option<u32> {
        const KEY_POWER: u64 = 116;
        let (mut in_keys, mut pin, mut code, mut found) = (false, None, None, None);
        let mut cur: &'static str = "";
        self.for_each_prop(|depth, node, prop, val| {
            if depth == 2 {
                in_keys = prop == "compatible" && val.split(|&b| b == 0).any(|s| s == b"gpio-keys")
                    || (in_keys && core::ptr::eq(node, cur));
                cur = node;
            } else if depth == 3 && in_keys {
                if !core::ptr::eq(node, cur) {
                    (cur, pin, code) = (node, None, None);
                }
                match prop {
                    "gpios" if val.len() >= 8 => pin = Some(be_cells(&val[4..], 1) as u32),
                    "linux,code" if val.len() >= 4 => code = Some(be_cells(val, 1)),
                    _ => {}
                }
                if found.is_none() && code == Some(KEY_POWER) {
                    found = pin;
                }
            }
        });
        found
    }
}

/// `compatible` böyiça topilgan qurilma tuguni.
#[derive(Clone, Copy, Default)]
pub struct Node {
    /// `reg` juftliklari (manzil, hajm) — ota tugunning #address-cells/#size-cells böyiça.
    pub reg: [(u64, u64); 4],
    pub nreg: usize,
    /// `interrupts` ning birinçi uçligi (tur, raqam, bayroqlar) — bölsa.
    pub irq: Option<(u32, u32)>,
    /// `ranges` (PCI host uçun xom baytlar).
    pub ranges: &'static [u8],
    /// `interrupt-map` (PCI host uçun xom baytlar).
    pub irq_map: &'static [u8],
    /// `interrupts` — barça uçliklar (xom baytlar; masalan taymerning 4 ta PPI si).
    pub irqs: &'static [u8],
}

impl Node {
    /// `interrupts` dagi `i`-uçlik (tur, raqam, bayroqlar).
    pub fn irq_at(&self, i: usize) -> Option<(u32, u32, u32)> {
        let c = self.irqs.get(i * 12..i * 12 + 12)?;
        Some((be_cells(c, 1) as u32, be_cells(&c[4..], 1) as u32, be_cells(&c[8..], 1) as u32))
    }
}

impl Fdt {
    /// `compatible` ro'yxatida `compat` bölgan, `status` "okay" (yoki yöq) bölgan birinçi tugun.
    /// Faqat yuqori darajadagi (ildiz bolalari) tugunlar va ildiz #address/#size-cells.
    pub fn find_compatible(&self, compat: &str) -> Option<Node> {
        let (mut ac, mut sc) = (2usize, 1usize);
        let mut found: Option<Node> = None;
        let mut cur_node: &'static str = "";
        let mut cur = Node::default();
        let (mut cur_match, mut cur_disabled, mut cur_reg): (bool, bool, &'static [u8]) = (false, false, &[]);
        let finish = |name: &str, m: bool, dis: bool, reg: &'static [u8], n: &mut Node, found: &mut Option<Node>, ac: usize, sc: usize| {
            if !m || dis || found.is_some() || name.is_empty() {
                return;
            }
            let step = (ac + sc) * 4;
            let mut k = 0;
            while k < 4 && (k + 1) * step <= reg.len() {
                let e = &reg[k * step..];
                n.reg[k] = (be_cells(e, ac), be_cells(&e[ac * 4..], sc));
                k += 1;
            }
            n.nreg = k;
            *found = Some(*n);
        };
        self.for_each_prop(|depth, node, prop, val| {
            if depth == 1 {
                if prop == "#address-cells" {
                    ac = be_cells(val, 1) as usize;
                } else if prop == "#size-cells" {
                    sc = be_cells(val, 1) as usize;
                }
                return;
            }
            if depth != 2 {
                return;
            }
            if !core::ptr::eq(node, cur_node) {
                finish(cur_node, cur_match, cur_disabled, cur_reg, &mut cur, &mut found, ac, sc);
                cur_node = node;
                cur = Node::default();
                (cur_match, cur_disabled, cur_reg) = (false, false, &[]);
            }
            match prop {
                "compatible" => cur_match = val.split(|&b| b == 0).any(|s| s == compat.as_bytes()),
                "status" => cur_disabled = !(val.starts_with(b"okay") || val.starts_with(b"ok")),
                "reg" => cur_reg = val,
                "ranges" => cur.ranges = val,
                "interrupt-map" => cur.irq_map = val,
                "interrupts" if val.len() >= 12 => {
                    cur.irq = Some((be_cells(val, 1) as u32, be_cells(&val[4..], 1) as u32));
                    cur.irqs = val;
                }
                _ => {}
            }
        });
        finish(cur_node, cur_match, cur_disabled, cur_reg, &mut cur, &mut found, ac, sc);
        found
    }
}
