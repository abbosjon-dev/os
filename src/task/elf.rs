//! ELF64 yuklovçi (faqat statik, AArch64 bajariladigan fayllar).
//! Har bir PT_LOAD segmenti uçun yangi sahifalar ajratiladi, fayldagi baytlar
//! nusxalanadi, qolgani (.bss) nol bilan töldiriladi.

use crate::memory::mmu::{AddressSpace, Kind, USER_BASE, USER_END};
use crate::memory::{PAGE_SIZE, align_down, align_up, frame};

const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const EM_AARCH64: u16 = 183;
const ET_EXEC: u16 = 2;

#[derive(Debug)]
pub enum ElfError {
    NotElf,
    WrongArch,
    BadSegment,
    OutOfMemory,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?) as usize)
}

/// Faylni manzil maydoniga yuklaydi va kiriş nuqtasini qaytaradi.
pub fn load(aspace: &mut AddressSpace, data: &[u8]) -> Result<usize, ElfError> {
    if data.get(0..4) != Some(b"\x7fELF") || data.get(4) != Some(&2) || data.get(5) != Some(&1) {
        return Err(ElfError::NotElf);
    }
    if u16_at(data, 16) != Some(ET_EXEC) || u16_at(data, 18) != Some(EM_AARCH64) {
        return Err(ElfError::WrongArch);
    }
    let entry = u64_at(data, 24).ok_or(ElfError::NotElf)?;
    let phoff = u64_at(data, 32).ok_or(ElfError::NotElf)?;
    let phentsize = u16_at(data, 54).ok_or(ElfError::NotElf)? as usize;
    let phnum = u16_at(data, 56).ok_or(ElfError::NotElf)? as usize;

    for i in 0..phnum {
        let ph = phoff + i * phentsize;
        if u32_at(data, ph) != Some(PT_LOAD) {
            continue;
        }
        let flags = u32_at(data, ph + 4).ok_or(ElfError::BadSegment)?;
        let offset = u64_at(data, ph + 8).ok_or(ElfError::BadSegment)?;
        let vaddr = u64_at(data, ph + 16).ok_or(ElfError::BadSegment)?;
        let filesz = u64_at(data, ph + 32).ok_or(ElfError::BadSegment)?;
        let memsz = u64_at(data, ph + 40).ok_or(ElfError::BadSegment)?;
        let end = vaddr.checked_add(memsz).ok_or(ElfError::BadSegment)?;
        if vaddr < USER_BASE || end > USER_END || filesz > memsz {
            return Err(ElfError::BadSegment);
        }
        let file = data.get(offset..offset + filesz).ok_or(ElfError::BadSegment)?;
        let kind = if flags & PF_X != 0 { Kind::UserCode } else { Kind::UserData };

        let mut page = align_down(vaddr, PAGE_SIZE);
        while page < align_up(end, PAGE_SIZE) {
            let pa = match aspace.translate(page) {
                Some(pa) => pa,
                None => {
                    let pa = frame::alloc_zeroed().ok_or(ElfError::OutOfMemory)?;
                    if aspace.map(page, pa, kind).is_err() {
                        frame::free(pa);
                        return Err(ElfError::OutOfMemory);
                    }
                    pa
                }
            };
            // Şu sahifaga tuşadigan fayl baytlari
            let lo = page.max(vaddr);
            let hi = (page + PAGE_SIZE).min(vaddr + filesz);
            if lo < hi {
                let src = &file[lo - vaddr..hi - vaddr];
                // Sahifa identity xaritalangan RAMʼda: PA orqali yozamiz.
                unsafe {
                    core::ptr::copy_nonoverlapping(src.as_ptr(), (pa + (lo - page)) as *mut u8, src.len());
                }
            }
            page += PAGE_SIZE;
        }
    }
    if !(USER_BASE..USER_END).contains(&entry) {
        return Err(ElfError::BadSegment);
    }
    Ok(entry)
}
