// SPDX-License-Identifier: GPL-3.0-or-later
//! Callbacks allocate host-owned bytes only. Guest callbacks are never invoked.
use std::{
    alloc::{alloc, dealloc, Layout},
    ffi::c_void,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
pub(super) const LIMIT: usize = 256 * 1024 * 1024;
#[derive(Default, Debug)]
pub(super) struct Budget(AtomicUsize);
impl Budget {
    pub fn acquire(&self, bytes: usize) -> bool {
        self.0
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes).filter(|n| *n <= LIMIT)
            })
            .is_ok()
    }
    pub fn release(&self, bytes: usize) {
        self.0.fetch_sub(bytes, Ordering::Relaxed);
    }
}
pub(super) struct Block {
    pub id: u64,
    pub pointer: *mut u8,
    pub length: usize,
    allocation: *mut u8,
    layout: Layout,
    budget: Arc<Budget>,
}
// The block has one owner. It moves between existing workers only while no C
// invocation or borrowed slice is live, exactly like the external-sync C API.
unsafe impl Send for Block {}
impl Block {
    pub fn new(id: u64, length: usize, fill: u8, budget: Arc<Budget>) -> Option<Self> {
        Self::with_alignment(id, length, fill, budget, None)
    }
    pub fn with_alignment(
        id: u64,
        length: usize,
        fill: u8,
        budget: Arc<Budget>,
        hint: Option<usize>,
    ) -> Option<Self> {
        if hint.is_some_and(|n| n >= 64 || n % 16 != 0) {
            return None;
        }
        let extra = if hint.is_some() { 64 } else { 0 };
        let layout = Layout::from_size_align(
            length.max(1).checked_add(extra)?,
            if hint.is_some() { 64 } else { 16 },
        )
        .ok()?;
        if !budget.acquire(layout.size()) {
            return None;
        }
        let allocation = unsafe { alloc(layout) };
        let pointer = if allocation.is_null() {
            allocation
        } else {
            unsafe { allocation.add(hint.unwrap_or(0)) }
        };
        if pointer.is_null() {
            budget.release(layout.size());
            return None;
        }
        unsafe {
            allocation.write_bytes(fill, layout.size());
        }
        Some(Self {
            id,
            pointer,
            allocation,
            length,
            layout,
            budget,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.pointer, self.length) }
    }
}
impl Drop for Block {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.allocation, self.layout);
        }
        self.budget.release(self.layout.size());
    }
}
pub(super) struct Heap {
    pub blocks: Vec<Block>,
    pub fill: u8,
    next: u64,
    pub budget: Arc<Budget>,
    pub events: u64,
    pub alignment: Option<usize>,
}
unsafe impl Send for Heap {}
impl Heap {
    pub fn new(fill: u8, budget: Arc<Budget>) -> Self {
        Self {
            blocks: Vec::new(),
            fill,
            next: 1,
            budget,
            events: 0,
            alignment: None,
        }
    }
    pub fn allocate(&mut self, length: usize) -> *mut c_void {
        self.events = self.events.wrapping_add(1);
        if self.blocks.try_reserve(1).is_err() {
            return std::ptr::null_mut();
        }
        let Some(block) = Block::with_alignment(
            self.next,
            length,
            self.fill,
            self.budget.clone(),
            self.alignment,
        ) else {
            return std::ptr::null_mut();
        };
        self.next += 1;
        let pointer = block.pointer;
        self.blocks.push(block);
        pointer.cast()
    }
    pub fn free(&mut self, pointer: *mut c_void) {
        self.events = self.events.wrapping_add(1);
        if let Some(at) = self
            .blocks
            .iter()
            .position(|b| b.pointer.cast::<c_void>() == pointer)
        {
            self.blocks.swap_remove(at);
        }
    }
    pub fn block(&self, id: u64) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }
}
pub(super) unsafe extern "C" fn zalloc(opaque: *mut c_void, count: u32, size: u32) -> *mut c_void {
    let Some(length) = (count as usize).checked_mul(size as usize) else {
        return std::ptr::null_mut();
    };
    unsafe { (&mut *opaque.cast::<Heap>()).allocate(length) }
}
pub(super) unsafe extern "C" fn zfree(opaque: *mut c_void, pointer: *mut c_void) {
    unsafe {
        (&mut *opaque.cast::<Heap>()).free(pointer);
    }
}
pub(super) extern "C" fn xalloc(opaque: *mut c_void, count: usize, size: usize) -> *mut c_void {
    let Some(length) = count.checked_mul(size) else {
        return std::ptr::null_mut();
    };
    unsafe { (&mut *opaque.cast::<Heap>()).allocate(length) }
}
pub(super) extern "C" fn xfree(opaque: *mut c_void, pointer: *mut c_void) {
    unsafe {
        (&mut *opaque.cast::<Heap>()).free(pointer);
    }
}
pub(super) unsafe extern "C" fn salloc(opaque: *mut c_void, size: usize) -> *mut c_void {
    unsafe { (&mut *opaque.cast::<Heap>()).allocate(size) }
}

pub(super) struct Reservation {
    budget: Arc<Budget>,
    bytes: usize,
}
impl Reservation {
    pub fn new(budget: Arc<Budget>) -> Self {
        Self { budget, bytes: 0 }
    }
    pub fn resize(&mut self, bytes: usize) -> bool {
        if bytes > self.bytes && !self.budget.acquire(bytes - self.bytes) {
            return false;
        }
        if bytes < self.bytes {
            self.budget.release(self.bytes - bytes);
        }
        self.bytes = bytes;
        true
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.release(self.bytes);
    }
}

/// C retains the allocator opaque pointer. Keep its pointee behind UnsafeCell,
/// so moving/borrowing the Rust owner never promises immutable callback data.
pub(super) struct HeapCell(Box<std::cell::UnsafeCell<Heap>>);
impl HeapCell {
    pub fn new(heap: Heap) -> Self {
        Self(Box::new(std::cell::UnsafeCell::new(heap)))
    }
    pub fn pointer(&self) -> *mut Heap {
        self.0.get()
    }
}
impl std::ops::Deref for HeapCell {
    type Target = Heap;
    fn deref(&self) -> &Heap {
        unsafe { &*self.pointer() }
    }
}
impl std::ops::DerefMut for HeapCell {
    fn deref_mut(&mut self) -> &mut Heap {
        unsafe { &mut *self.pointer() }
    }
}
