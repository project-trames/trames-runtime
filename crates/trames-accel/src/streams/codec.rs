// SPDX-License-Identifier: GPL-3.0-or-later
//! Every C context is initialized by its original native implementation. No
//! guest opaque state, vtable, allocator or address is passed into C.
use super::heap::{self, Budget, Heap, HeapCell};
use std::{
    cell::UnsafeCell,
    ffi::{c_char, c_int, c_void},
    sync::Arc,
};
pub(super) const ROOT: u64 = u64::MAX;
pub(super) const HEAP: u64 = u64::MAX - 1;
pub(super) const ALLOCATOR: u64 = u64::MAX - 2;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Kind {
    Deflate,
    Inflate,
    Xz,
    ZstdC,
    ZstdD,
}
pub(super) fn kind(op: u64) -> Option<Kind> {
    Some(match op {
        1 | 3 | 5 | 8 | 10 => Kind::Deflate,
        2 | 4 | 6 | 7 | 9 | 11 => Kind::Inflate,
        12..=18 => Kind::Xz,
        19 | 21 | 23 | 25 | 27 | 29 | 31..=33 => Kind::ZstdC,
        20 | 22 | 24 | 26 | 28 | 30 => Kind::ZstdD,
        _ => return None,
    })
}
pub(super) fn initializes(op: u64) -> bool {
    matches!(op, 1 | 2 | 12..=16 | 19 | 20)
}
pub(super) fn processes(op: u64) -> bool {
    matches!(op, 3 | 4 | 17 | 21 | 22 | 31..=33)
}
pub(super) fn closes(op: u64) -> bool {
    matches!(op, 8 | 9 | 18 | 27 | 28)
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CustomMem {
    alloc: unsafe extern "C" fn(*mut c_void, usize) -> *mut c_void,
    free: unsafe extern "C" fn(*mut c_void, *mut c_void),
    opaque: *mut c_void,
}
#[repr(C)]
pub(super) struct Buffer {
    pub data: *mut c_void,
    pub size: usize,
    pub pos: usize,
}
unsafe extern "C" {
    #[link_name = "deflateInit2_"]
    fn z_deflateInit2_(
        s: *mut libz_sys::z_stream,
        level: c_int,
        method: c_int,
        bits: c_int,
        mem: c_int,
        strategy: c_int,
        version: *const c_char,
        size: c_int,
    ) -> c_int;
    #[link_name = "inflateInit2_"]
    fn z_inflateInit2_(
        s: *mut libz_sys::z_stream,
        bits: c_int,
        version: *const c_char,
        size: c_int,
    ) -> c_int;
    #[link_name = "deflate"]
    fn z_deflate(s: *mut libz_sys::z_stream, flush: c_int) -> c_int;
    #[link_name = "inflate"]
    fn z_inflate(s: *mut libz_sys::z_stream, flush: c_int) -> c_int;
    #[link_name = "deflateReset"]
    fn z_deflateReset(s: *mut libz_sys::z_stream) -> c_int;
    #[link_name = "inflateReset"]
    fn z_inflateReset(s: *mut libz_sys::z_stream) -> c_int;
    #[link_name = "inflateReset2"]
    fn z_inflateReset2(s: *mut libz_sys::z_stream, bits: c_int) -> c_int;
    #[link_name = "deflateEnd"]
    fn z_deflateEnd(s: *mut libz_sys::z_stream) -> c_int;
    #[link_name = "inflateEnd"]
    fn z_inflateEnd(s: *mut libz_sys::z_stream) -> c_int;
    #[link_name = "deflateSetDictionary"]
    fn z_deflateSetDictionary(s: *mut libz_sys::z_stream, data: *const u8, length: u32) -> c_int;
    #[link_name = "inflateSetDictionary"]
    fn z_inflateSetDictionary(s: *mut libz_sys::z_stream, data: *const u8, length: u32) -> c_int;
    #[link_name = "ZSTD_createCCtx_advanced"]
    fn trames_stream_ZSTD_createCCtx_advanced(m: CustomMem) -> *mut c_void;
    #[link_name = "ZSTD_createDCtx_advanced"]
    fn trames_stream_ZSTD_createDCtx_advanced(m: CustomMem) -> *mut c_void;
    #[link_name = "ZSTD_freeCCtx"]
    fn trames_stream_ZSTD_freeCCtx(c: *mut c_void) -> usize;
    #[link_name = "ZSTD_freeDCtx"]
    fn trames_stream_ZSTD_freeDCtx(c: *mut c_void) -> usize;
    #[link_name = "ZSTD_compressStream2"]
    fn trames_stream_ZSTD_compressStream2(
        c: *mut c_void,
        output: *mut Buffer,
        input: *mut Buffer,
        directive: c_int,
    ) -> usize;
    #[link_name = "ZSTD_compressStream"]
    fn trames_stream_ZSTD_compressStream(
        c: *mut c_void,
        output: *mut Buffer,
        input: *mut Buffer,
    ) -> usize;
    #[link_name = "ZSTD_decompressStream"]
    fn trames_stream_ZSTD_decompressStream(
        c: *mut c_void,
        output: *mut Buffer,
        input: *mut Buffer,
    ) -> usize;
    #[link_name = "ZSTD_flushStream"]
    fn trames_stream_ZSTD_flushStream(c: *mut c_void, output: *mut Buffer) -> usize;
    #[link_name = "ZSTD_endStream"]
    fn trames_stream_ZSTD_endStream(c: *mut c_void, output: *mut Buffer) -> usize;
    #[link_name = "ZSTD_initCStream"]
    fn trames_stream_ZSTD_initCStream(c: *mut c_void, level: c_int) -> usize;
    #[link_name = "ZSTD_initDStream"]
    fn trames_stream_ZSTD_initDStream(c: *mut c_void) -> usize;
    #[link_name = "ZSTD_CCtx_reset"]
    fn trames_stream_ZSTD_CCtx_reset(c: *mut c_void, directive: c_int) -> usize;
    #[link_name = "ZSTD_DCtx_reset"]
    fn trames_stream_ZSTD_DCtx_reset(c: *mut c_void, directive: c_int) -> usize;
    #[link_name = "ZSTD_CCtx_setParameter"]
    fn trames_stream_ZSTD_CCtx_setParameter(
        c: *mut c_void,
        parameter: c_int,
        value: c_int,
    ) -> usize;
    #[link_name = "ZSTD_DCtx_setParameter"]
    fn trames_stream_ZSTD_DCtx_setParameter(
        c: *mut c_void,
        parameter: c_int,
        value: c_int,
    ) -> usize;
}
pub(super) struct Native {
    pub kind: Kind,
    pub encoder: bool,
    pub gzip: bool,
    pub heap: HeapCell,
    z: Option<Box<UnsafeCell<libz_sys::z_stream>>>,
    x: Option<Box<UnsafeCell<liblzma_sys::lzma_stream>>>,
    allocator: Box<liblzma_sys::lzma_allocator>,
    context: *mut c_void,
    pub result: u64,
}
// Native owns all allocation callbacks/context memory. A pool transfers that
// ownership between existing workers; no C operation overlaps another owner.
unsafe impl Send for Native {}
impl Native {
    pub fn image_probes(&self) -> [*const c_void; 4] {
        [
            z_deflate as *const () as *const c_void,
            liblzma_sys::lzma_code as *const () as *const c_void,
            trames_stream_ZSTD_compressStream2 as *const () as *const c_void,
            heap::zalloc as *const () as *const c_void,
        ]
    }
    pub fn new(op: u64, a: [u64; 9], fill: u8, budget: Arc<Budget>) -> Option<Self> {
        let kind = kind(op)?;
        // Private layouts are tied to these bundled implementations. A provider
        // upgrade needs a reviewed projection, not a best-effort reinterpretation.
        let version = unsafe {
            match kind {
                Kind::Deflate | Kind::Inflate => {
                    std::ffi::CStr::from_ptr(libz_sys::zlibVersion()).to_bytes() == b"1.4"
                }
                Kind::Xz => liblzma_sys::lzma_version_number() == 50_080_042,
                _ => zstd_sys::ZSTD_versionNumber() == 10600,
            }
        };
        if !version {
            return None;
        }
        // Decline oversized optional caches before constructing two encoders.
        // The guest still initializes its original stream with its own budget.
        if op == 12
            && unsafe { liblzma_sys::lzma_easy_encoder_memusage(a[1] as u32) }
                > super::heap::LIMIT as u64 / 8
        {
            return None;
        }

        let heap = HeapCell::new(Heap::new(fill, budget));
        let opaque = heap.pointer().cast();
        let allocator = Box::new(liblzma_sys::lzma_allocator {
            alloc: Some(heap::xalloc),
            free: Some(heap::xfree),
            opaque,
        });
        let mut n = Self {
            kind,
            encoder: op == 12,
            gzip: op == 1 && (24..=31).contains(&(a[3] as i32)),
            heap,
            z: None,
            x: None,
            allocator,
            context: std::ptr::null_mut(),
            result: 0,
        };
        match kind {
            Kind::Deflate | Kind::Inflate => {
                n.z = Some(Box::new(UnsafeCell::new(libz_sys::z_stream {
                    next_in: std::ptr::null_mut(),
                    avail_in: 0,
                    total_in: 0,
                    next_out: std::ptr::null_mut(),
                    avail_out: 0,
                    total_out: 0,
                    msg: std::ptr::null_mut(),
                    state: std::ptr::null_mut(),
                    zalloc: heap::zalloc,
                    zfree: heap::zfree,
                    opaque,
                    data_type: 0,
                    adler: 0,
                    reserved: 0,
                })));
                let z = n.z.as_ref()?.get();
                n.result = unsafe {
                    if kind == Kind::Deflate {
                        z_deflateInit2_(
                            z,
                            a[1] as _,
                            a[2] as _,
                            a[3] as _,
                            a[4] as _,
                            a[5] as _,
                            c"1.4".as_ptr(),
                            std::mem::size_of::<libz_sys::z_stream>() as _,
                        )
                    } else {
                        z_inflateInit2_(
                            z,
                            a[1] as _,
                            c"1.4".as_ptr(),
                            std::mem::size_of::<libz_sys::z_stream>() as _,
                        )
                    }
                } as u32 as u64;
            }
            Kind::Xz => {
                // All fields are pointers/integers; the upstream initializer is
                // LZMA_STREAM_INIT (all zero), not an opaque Rust enum value.
                let x = Box::new(UnsafeCell::<liblzma_sys::lzma_stream>::new(unsafe {
                    std::mem::zeroed()
                }));
                unsafe {
                    (*x.get()).allocator = &*n.allocator;
                }
                n.result = unsafe {
                    match op {
                        12 => liblzma_sys::lzma_easy_encoder(x.get(), a[1] as _, a[2] as _),
                        14 => liblzma_sys::lzma_stream_decoder(x.get(), a[1], a[2] as _),
                        15 => liblzma_sys::lzma_auto_decoder(x.get(), a[1], a[2] as _),
                        16 => liblzma_sys::lzma_alone_decoder(x.get(), a[1]),
                        _ => return None,
                    }
                } as u64;
                n.x = Some(x);
            }
            Kind::ZstdC | Kind::ZstdD => {
                let m = CustomMem {
                    alloc: heap::salloc,
                    free: heap::zfree,
                    opaque,
                };
                n.context = unsafe {
                    if kind == Kind::ZstdC {
                        trames_stream_ZSTD_createCCtx_advanced(m)
                    } else {
                        trames_stream_ZSTD_createDCtx_advanced(m)
                    }
                };
                n.result = n.context as usize as u64;
            }
        }
        if matches!(kind, Kind::ZstdC | Kind::ZstdD) {
            if n.context.is_null() {
                return None;
            }
        } else if n.result != 0 {
            return None;
        }
        Some(n)
    }
    pub fn root(&self) -> *mut u8 {
        if let Some(z) = &self.z {
            return unsafe { (*z.get()).state.cast() };
        }
        if let Some(x) = &self.x {
            // `internal` is intentionally private in the public bindings. Its
            // stable LP64 public-stream slot is not private-coder layout.
            return unsafe {
                std::ptr::read_unaligned((x.get().cast::<u8>()).add(56).cast::<*mut u8>())
            };
        }
        self.context.cast()
    }
    pub fn public(&self) -> Option<(*mut u8, usize)> {
        if let Some(z) = &self.z {
            Some((z.get().cast(), std::mem::size_of::<libz_sys::z_stream>()))
        } else {
            self.x.as_ref().map(|x| {
                (
                    x.get().cast(),
                    std::mem::size_of::<liblzma_sys::lzma_stream>(),
                )
            })
        }
    }
    pub fn special(&self) -> [(u64, usize, usize); 3] {
        let (root, size) = self.public().map_or((0, 0), |(p, n)| (p as usize, n));
        [
            (ROOT, root, size),
            (HEAP, self.heap.pointer() as usize, 1),
            (
                ALLOCATOR,
                (&*self.allocator as *const liblzma_sys::lzma_allocator) as usize,
                std::mem::size_of::<liblzma_sys::lzma_allocator>(),
            ),
        ]
    }
    pub fn io(
        &mut self,
        input: *mut u8,
        in_len: usize,
        output: *mut u8,
        out_len: usize,
        [total_in, total_out, adler]: [u64; 3],
    ) -> bool {
        if let Some(cell) = &self.z {
            let z = unsafe { &mut *cell.get() };
            let (Ok(ni), Ok(no)) = (u32::try_from(in_len), u32::try_from(out_len)) else {
                return false;
            };
            z.next_in = input;
            z.avail_in = ni;
            z.next_out = output;
            z.avail_out = no;
            z.total_in = total_in as _;
            z.total_out = total_out as _;
            z.adler = adler as _;
        }
        if let Some(cell) = &self.x {
            let x = unsafe { &mut *cell.get() };
            x.next_in = input;
            x.avail_in = in_len;
            x.next_out = output;
            x.avail_out = out_len;
            x.total_in = total_in;
            x.total_out = total_out;
        }
        true
    }
    /// Default gzip headers describe the Linux guest, including when the
    /// executing provider was compiled for Darwin. Pending header bytes must
    /// also be normalized so original guest code can resume after any byte.
    pub fn linux_gzip_pending(&mut self) -> Option<()> {
        if !self.gzip {
            return Some(());
        }
        let z = self.z.as_ref()?.get();
        if unsafe { (*z).total_out } >= 10 {
            return Some(());
        }
        let state = unsafe { (*z).state.cast::<u8>() };
        let pointer = |at| unsafe { std::ptr::read_unaligned(state.add(at).cast::<*mut u8>()) };
        // Reviewed zlib 1.4 deflate_state prefix: pending_buf 16,
        // pending_out 32, pending 40, gzhead 56 (LP64).
        if !pointer(56).is_null() {
            return None;
        }
        let pending = unsafe { std::ptr::read_unaligned(state.add(40).cast::<u64>()) };
        if pending == 0 {
            return Some(());
        }
        let base = pointer(16);
        let cursor = pointer(32);
        let offset = (cursor as usize).checked_sub(base as usize)?;
        if offset <= 9 && offset.checked_add(usize::try_from(pending).ok()?)? > 9 {
            let block = self
                .heap
                .blocks
                .iter()
                .find(|b| b.pointer == base && b.length >= 10)?;
            unsafe {
                *block.pointer.add(9) = 3;
            }
        }
        Some(())
    }
    pub fn progress(&self) -> Option<(usize, usize, u64, u64, u64, i32, u64)> {
        if let Some(cell) = &self.z {
            let z = unsafe { &*cell.get() };
            Some((
                z.avail_in as _,
                z.avail_out as _,
                z.total_in as _,
                z.total_out as _,
                z.adler as _,
                z.data_type,
                z.msg as usize as u64,
            ))
        } else {
            self.x.as_ref().map(|cell| {
                let x = unsafe { &*cell.get() };
                (x.avail_in, x.avail_out, x.total_in, x.total_out, 0, 0, 0)
            })
        }
    }
    pub fn call(
        &mut self,
        op: u64,
        a: [u64; 9],
        input: &mut Buffer,
        output: &mut Buffer,
        dictionary: *const u8,
    ) -> bool {
        self.result = unsafe {
            match op {
                3 => z_deflate(self.z.as_ref().unwrap().get(), a[1] as _) as u32 as u64,
                4 => z_inflate(self.z.as_ref().unwrap().get(), a[1] as _) as u32 as u64,
                5 => z_deflateReset(self.z.as_ref().unwrap().get()) as u32 as u64,
                6 => z_inflateReset(self.z.as_ref().unwrap().get()) as u32 as u64,
                7 => z_inflateReset2(self.z.as_ref().unwrap().get(), a[1] as _) as u32 as u64,
                10 => z_deflateSetDictionary(self.z.as_ref().unwrap().get(), dictionary, a[2] as _)
                    as u32 as u64,
                11 => z_inflateSetDictionary(self.z.as_ref().unwrap().get(), dictionary, a[2] as _)
                    as u32 as u64,
                17 if a[1] <= 4 => {
                    liblzma_sys::lzma_code(self.x.as_ref().unwrap().get(), a[1] as _) as u64
                }
                21 if a[3] <= 2 => {
                    trames_stream_ZSTD_compressStream2(self.context, output, input, a[3] as _)
                        as u64
                }
                22 => trames_stream_ZSTD_decompressStream(self.context, output, input) as u64,
                23 => trames_stream_ZSTD_initCStream(self.context, a[1] as _) as u64,
                24 => trames_stream_ZSTD_initDStream(self.context) as u64,
                25 if (1..=3).contains(&a[1]) => {
                    trames_stream_ZSTD_CCtx_reset(self.context, a[1] as _) as u64
                }
                26 if (1..=3).contains(&a[1]) => {
                    trames_stream_ZSTD_DCtx_reset(self.context, a[1] as _) as u64
                }
                29 => {
                    trames_stream_ZSTD_CCtx_setParameter(self.context, a[1] as _, a[2] as _) as u64
                }
                30 => {
                    trames_stream_ZSTD_DCtx_setParameter(self.context, a[1] as _, a[2] as _) as u64
                }
                31 => trames_stream_ZSTD_flushStream(self.context, output) as u64,
                32 => trames_stream_ZSTD_endStream(self.context, output) as u64,
                33 => trames_stream_ZSTD_compressStream(self.context, output, input) as u64,
                _ => return false,
            }
        };
        true
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        unsafe {
            if let Some(cell) = &self.z {
                let z = cell.get();
                if !(*z).state.is_null() {
                    if self.kind == Kind::Deflate {
                        z_deflateEnd(z);
                    } else {
                        z_inflateEnd(z);
                    }
                }
            }
            if let Some(x) = &self.x {
                liblzma_sys::lzma_end(x.get());
            }
            if !self.context.is_null() {
                if self.kind == Kind::ZstdC {
                    trames_stream_ZSTD_freeCCtx(self.context);
                } else {
                    trames_stream_ZSTD_freeDCtx(self.context);
                }
            }
        }
    }
}
