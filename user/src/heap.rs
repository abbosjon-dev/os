//! Foydalanuvçi xotira ajratuvçisi: böş bloklar röyxati (free list).
//! Xotira yetmasa, yadrodan yangi sahifalar sörar (SYS_MAP). Jarayon bitta oqimli,
//! şuning uçun qulf kerak emas.

use crate::abi::SYS_MAP;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::null_mut;

const MIN_BLOCK: usize = 16;
const GROW: usize = 64 * 1024;

struct Node {
    size: usize,
    next: *mut Node,
}

struct Heap {
    head: UnsafeCell<*mut Node>,
}

unsafe impl Sync for Heap {}

#[global_allocator]
static HEAP: Heap = Heap { head: UnsafeCell::new(null_mut()) };

const fn align_up(x: usize, a: usize) -> usize {
    (x + a - 1) & !(a - 1)
}

fn block_size(layout: &Layout) -> usize {
    align_up(layout.size().max(MIN_BLOCK), MIN_BLOCK)
}

impl Heap {
    unsafe fn insert(&self, addr: usize, size: usize) {
        unsafe {
            let head = &mut *self.head.get();
            let mut prev: *mut Node = null_mut();
            let mut cur = *head;
            while !cur.is_null() && (cur as usize) < addr {
                prev = cur;
                cur = (*cur).next;
            }
            let node = addr as *mut Node;
            node.write(Node { size, next: cur });
            if !cur.is_null() && addr + size == cur as usize {
                (*node).size += (*cur).size;
                (*node).next = (*cur).next;
            }
            if !prev.is_null() && prev as usize + (*prev).size == addr {
                (*prev).size += (*node).size;
                (*prev).next = (*node).next;
            } else if prev.is_null() {
                *head = node;
            } else {
                (*prev).next = node;
            }
        }
    }

    unsafe fn try_alloc(&self, size: usize, align: usize) -> *mut u8 {
        unsafe {
            let head = &mut *self.head.get();
            let mut prev: *mut Node = null_mut();
            let mut cur = *head;
            while !cur.is_null() {
                let start = cur as usize;
                let end = start + (*cur).size;
                let aligned = align_up(start, align);
                if aligned + size <= end {
                    let next = (*cur).next;
                    if prev.is_null() {
                        *head = next;
                    } else {
                        (*prev).next = next;
                    }
                    if aligned > start {
                        self.insert(start, aligned - start);
                    }
                    if aligned + size < end {
                        self.insert(aligned + size, end - aligned - size);
                    }
                    return aligned as *mut u8;
                }
                prev = cur;
                cur = (*cur).next;
            }
        }
        null_mut()
    }
}

unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = block_size(&layout);
        let align = layout.align().max(MIN_BLOCK);
        unsafe {
            let p = self.try_alloc(size, align);
            if !p.is_null() {
                return p;
            }
            let grow = align_up(size + align, GROW);
            let addr = crate::syscall(SYS_MAP, grow as u64, 0, 0, 0);
            if addr < 0 {
                return null_mut();
            }
            self.insert(addr as usize, grow);
            self.try_alloc(size, align)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.insert(ptr as usize, block_size(&layout)) };
    }
}
