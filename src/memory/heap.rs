//! Kernel heap: bo'sh bloklar ro'yxati (free list) allokatori.
//! Bloklar manzil bo'yicha tartiblangan, bo'shatilganda qo'shnilar bilan birlashtiriladi.
//! Bu `alloc` crate'ini (Box, Vec, String...) ishlatish imkonini beradi —
//! `alloc` Rust'ning o'z standart qismi, tashqi kutubxona emas.

use super::{PAGE_SIZE, align_up, frame};
use crate::sync::SpinLock;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{self, null_mut};

const MIN_BLOCK: usize = 16; // = size_of::<Node>()
const HEAP_PAGES: usize = 2048; // 8 MiB

struct Node {
    size: usize,
    next: *mut Node,
}

struct FreeList {
    head: *mut Node,
    total: usize,
    used: usize,
}

unsafe impl Send for FreeList {}

pub struct KernelHeap(SpinLock<FreeList>);

#[global_allocator]
static HEAP: KernelHeap =
    KernelHeap(SpinLock::new(FreeList { head: null_mut(), total: 0, used: 0 }));

fn block_size(layout: &Layout) -> usize {
    align_up(layout.size().max(MIN_BLOCK), MIN_BLOCK)
}

impl FreeList {
    /// [addr, addr+size) ni ro'yxatga qo'shadi va qo'shnilar bilan birlashtiradi.
    unsafe fn insert(&mut self, addr: usize, size: usize) {
        unsafe {
            let mut prev: *mut Node = null_mut();
            let mut cur = self.head;
            while !cur.is_null() && (cur as usize) < addr {
                prev = cur;
                cur = (*cur).next;
            }

            let node = addr as *mut Node;
            node.write(Node { size, next: cur });

            // Keyingisi bilan birlashtirish
            if !cur.is_null() && addr + size == cur as usize {
                (*node).size += (*cur).size;
                (*node).next = (*cur).next;
            }
            // Oldingisi bilan birlashtirish
            if !prev.is_null() && prev as usize + (*prev).size == addr {
                (*prev).size += (*node).size;
                (*prev).next = (*node).next;
            } else if prev.is_null() {
                self.head = node;
            } else {
                (*prev).next = node;
            }
        }
    }

    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = block_size(&layout);
        let align = layout.align().max(MIN_BLOCK);
        unsafe {
            let mut prev: *mut Node = null_mut();
            let mut cur = self.head;
            while !cur.is_null() {
                let start = cur as usize;
                let end = start + (*cur).size;
                let aligned = align_up(start, align);
                if aligned + size <= end {
                    let next = (*cur).next;
                    // Blokni ro'yxatdan olib tashlaymiz
                    if prev.is_null() {
                        self.head = next;
                    } else {
                        (*prev).next = next;
                    }
                    // Old va orqa qoldiqlarni qaytaramiz (ular 16 ga karrali)
                    if aligned > start {
                        self.insert(start, aligned - start);
                    }
                    if aligned + size < end {
                        self.insert(aligned + size, end - aligned - size);
                    }
                    self.used += size;
                    return aligned as *mut u8;
                }
                prev = cur;
                cur = (*cur).next;
            }
        }
        null_mut()
    }
}

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { self.0.lock().alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = block_size(&layout);
        let mut h = self.0.lock();
        h.used -= size;
        unsafe { h.insert(ptr as usize, size) };
    }
}

pub fn init() {
    let base = frame::alloc_contiguous(HEAP_PAGES).expect("heap uchun xotira yetmadi");
    let size = HEAP_PAGES * PAGE_SIZE;
    let mut h = HEAP.0.lock();
    h.total = size;
    unsafe {
        ptr::write_bytes(base as *mut u8, 0, MIN_BLOCK);
        h.insert(base, size);
    }
}

/// (ishlatilgan, jami) baytlarda
pub fn stats() -> (usize, usize) {
    let h = HEAP.0.lock();
    (h.used, h.total)
}
