// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional, evictable acceleration of original guest-owned compression state.
mod codec;
mod heap;
mod image;
mod projection;
mod schema;
use crate::native::{Error, Metrics, Return};
use codec::{Kind, Native};
use heap::{Block, Budget};
use projection::{External, Pair};
use std::sync::{Arc, Mutex};
use trames_memory::NativeMemory;
pub const PROFILE: u64 = 0x0001_000c;
pub const AUXV_KEY: u64 = 0x5452_000d;
pub const FUNCTION: u64 = 1;
pub const ENABLED: bool = cfg!(all(target_arch = "aarch64", unix));
pub const NAMES: &[(&[u8], u32)] = &[
    (b"deflateInit2_", 1),
    (b"inflateInit2_", 2),
    (b"deflate", 3),
    (b"inflate", 4),
    (b"deflateReset", 5),
    (b"inflateReset", 6),
    (b"inflateReset2", 7),
    (b"deflateEnd", 8),
    (b"inflateEnd", 9),
    (b"deflateSetDictionary", 10),
    (b"inflateSetDictionary", 11),
    (b"lzma_easy_encoder", 12),
    (b"lzma_stream_decoder", 14),
    (b"lzma_auto_decoder", 15),
    (b"lzma_alone_decoder", 16),
    (b"lzma_code", 17),
    (b"lzma_end", 18),
    (b"ZSTD_createCCtx", 19),
    (b"ZSTD_createCStream", 19),
    (b"ZSTD_createDCtx", 20),
    (b"ZSTD_createDStream", 20),
    (b"ZSTD_compressStream2", 21),
    (b"ZSTD_decompressStream", 22),
    (b"ZSTD_initCStream", 23),
    (b"ZSTD_initDStream", 24),
    (b"ZSTD_CCtx_reset", 25),
    (b"ZSTD_DCtx_reset", 26),
    (b"ZSTD_freeCCtx", 27),
    (b"ZSTD_freeCStream", 27),
    (b"ZSTD_freeDCtx", 28),
    (b"ZSTD_freeDStream", 28),
    (b"ZSTD_CCtx_setParameter", 29),
    (b"ZSTD_DCtx_setParameter", 30),
    (b"ZSTD_flushStream", 31),
    (b"ZSTD_endStream", 32),
    (b"ZSTD_compressStream", 33),
];
struct Ready {
    kind: Kind,
    root: u64,
    pair: Pair,
}
struct Pending {
    frame: u64,
    op: u64,
    args: [u64; 9],
    pair: Pair,
    io: Option<Io>,
}
struct Cache {
    ready: Vec<Ready>,
    pending: Vec<Pending>,
}
struct Owner {
    state: Mutex<Cache>,
    budget: Arc<Budget>,
}
#[derive(Clone)]
pub struct Shared(Arc<Owner>);
impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Stream acceleration cache")
    }
}
impl Default for Shared {
    fn default() -> Self {
        Self(Arc::new(Owner {
            state: Mutex::new(Cache {
                ready: Vec::new(),
                pending: Vec::new(),
            }),
            budget: Arc::new(Budget::default()),
        }))
    }
}
impl Shared {
    fn take(&self, kind: Kind, root: u64) -> Option<Pair> {
        let mut state = self.0.state.lock().unwrap();
        let at = state
            .ready
            .iter()
            .position(|e| e.kind == kind && e.root == root)?;
        Some(state.ready.swap_remove(at).pair)
    }
    fn invalidate(&self, kind: Kind, root: u64) {
        drop(self.take(kind, root));
    }
    fn ready(&self, mut pair: Pair) {
        pair.prune_external();
        let mut retired = Vec::new();
        {
            let mut state = self.0.state.lock().unwrap();
            if let Some(at) = state
                .ready
                .iter()
                .position(|e| e.kind == pair.a.kind && e.root == pair.guest)
            {
                retired.push(state.ready.swap_remove(at));
            }
            if state.ready.len() >= 32 {
                retired.push(state.ready.remove(0));
            }
            state.ready.push(Ready {
                kind: pair.a.kind,
                root: pair.guest,
                pair,
            });
        }
        // End/free callbacks run after the resource list lock is released.
        drop(retired);
    }
    fn pending(&self, p: Pending) {
        let retired = {
            let mut state = self.0.state.lock().unwrap();
            let retired = if state.pending.len() >= 32 {
                Some(state.pending.remove(0))
            } else {
                None
            };
            state.pending.push(p);
            retired
        };
        drop(retired);
    }
    fn complete(&self, frame: u64, op: u64) -> Option<Pending> {
        let mut state = self.0.state.lock().unwrap();
        let at = state
            .pending
            .iter()
            .rposition(|p| p.frame == frame && p.op == op)?;
        Some(state.pending.swap_remove(at))
    }
    fn begin(&self, op: u64, args: [u64; 9], abi: u64) -> Option<Pair> {
        let a = Native::new(op, args, 0xa5, self.0.budget.clone())?;
        let b = Native::new(op, args, 0x5a, self.0.budget.clone())?;
        Pair::new(a, b, abi)
    }
}
fn fallback() -> Return {
    Return {
        value: 0,
        fallback: true,
    }
}
fn done(value: u64) -> Return {
    Return {
        value,
        fallback: false,
    }
}
fn read(m: &impl NativeMemory, at: u64, len: usize) -> Option<Vec<u8>> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).ok()?;
    v.resize(len, 0);
    m.native_read(at, &mut v).ok()?;
    Some(v)
}
fn u64at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}
#[derive(Clone, Copy)]
struct Io {
    input: u64,
    in_len: usize,
    in_pos: usize,
    output: u64,
    out_len: usize,
    out_pos: usize,
    input_record: u64,
    output_record: u64,
    totals: [u64; 3],
    input_id: u64,
    output_id: u64,
    in_after: usize,
    out_after: usize,
}

/// This resource is only a cache. An unsupported operation, changed opaque
/// state, failed memory admission, or missing entry runs original guest code.
/// No stream/race error is invented, and no host operation runs under its lock.
pub(crate) fn execute(
    shared: &Shared,
    m: &mut impl NativeMemory,
    args: [u64; 3],
    metrics: &mut Metrics,
) -> Result<Return, Error> {
    let Some(frame) = read(m, args[0], 72) else {
        return Ok(if args[1] == 1 {
            done(args[2])
        } else {
            fallback()
        });
    };
    let a: [u64; 9] = std::array::from_fn(|i| u64at(&frame, i * 8).unwrap());
    let op = a[8] & 65535;
    let abi = a[8] >> 16;
    let Some(kind) = codec::kind(op) else {
        return Ok(if args[1] == 1 {
            done(args[2])
        } else {
            fallback()
        });
    };
    if !ENABLED {
        return Ok(if args[1] == 1 {
            done(args[2])
        } else {
            fallback()
        });
    }
    if args[1] == 1 {
        let Some(mut p) = shared.complete(args[0], op) else {
            return Ok(done(args[2]));
        };
        let creates = matches!(op, 19 | 20);
        let same = if creates {
            args[2] != 0
        } else if op <= 18 {
            (args[2] as u32 as u64) == p.pair.a.result && p.pair.a.result == p.pair.b.result
        } else {
            args[2] == p.pair.a.result && p.pair.a.result == p.pair.b.result
        };
        if same {
            let root = if creates { args[2] } else { p.args[0] };
            let mut learned = p.pair.learn(m, root).is_some();
            if !learned
                && p.io.is_some()
                && p.pair.align_workspace(m).is_some()
                && advance(shared, m, &mut p.pair, op, p.args, &mut p.io) == Some(args[2])
            {
                learned = p.pair.learn(m, root).is_some();
            }
            if !learned && codec::initializes(op) {
                if let Some(guard) = guards(m, kind, root) {
                    p.pair.guest = root;
                    p.pair.guard = guard;
                    shared.ready(p.pair);
                }
                return Ok(done(args[2]));
            }
            if learned && codec::processes(op) {
                p.pair.started = true;
            }
            if learned {
                // Public callbacks/state handles are part of eligibility; I/O
                // cursors and documented public counters remain caller-owned.
                p.pair
                    .guard
                    .extend(guards(m, kind, root).unwrap_or_default());
                if !p.pair.guard.is_empty() || matches!(kind, Kind::ZstdC | Kind::ZstdD) {
                    if let Some(io) = p.io {
                        if !validate_output(m, &mut p.pair, io) {
                            return Ok(done(args[2]));
                        }
                    }
                    shared.ready(p.pair);
                    metrics.stream_learns += 1;
                    metrics.stream_learn_operations[kind as usize] += 1;
                }
            }
        }
        return Ok(done(args[2]));
    }
    if args[1] != 0 {
        return Ok(fallback());
    }
    if codec::closes(op) {
        shared.invalidate(kind, a[0]);
        return Ok(fallback());
    }
    if codec::initializes(op) {
        let eligible = match kind {
            Kind::Deflate | Kind::Inflate => read(m, a[0].checked_add(64).unwrap_or(0), 16)
                .is_some_and(|b| b.iter().all(|v| *v == 0)),
            Kind::Xz => read(m, a[0].checked_add(48).unwrap_or(0), 8)
                .is_some_and(|b| b.iter().all(|v| *v == 0)),
            _ => true,
        };
        if eligible {
            if let Some(pair) = shared.begin(op, a, abi) {
                shared.pending(Pending {
                    frame: args[0],
                    op,
                    args: a,
                    pair,
                    io: None,
                });
            }
        }
        return Ok(fallback());
    }
    let Some(mut pair) = shared.take(kind, a[0]) else {
        metrics.stream_cache_misses += 1;
        return Ok(fallback());
    };
    if pair.abi != abi {
        return Ok(fallback());
    }
    if !pair.matches(m) {
        metrics.stream_cache_misses += 1;
        return Ok(fallback());
    }
    let mut io = if codec::processes(op) {
        let Some(io) = prepare_io(shared, m, &mut pair, op, a) else {
            return Ok(fallback());
        };
        Some(io)
    } else {
        None
    };
    let events = (pair.a.heap.events, pair.b.heap.events);
    let Some(result) = advance(shared, m, &mut pair, op, a, &mut io) else {
        return Ok(fallback());
    };
    if kind == Kind::ZstdC && !codec::processes(op) && !pair.started {
        if pair.controls.len() >= 16 {
            return Ok(fallback());
        }
        pair.controls.push((op, a));
    }
    // Every actual allocator/free invocation must still happen in the guest.
    // Native calls which allocate (including temporary allocations), free or
    // fail an allocation remain shadows until the original call completes.
    let publishable = match kind {
        Kind::Deflate | Kind::Inflate => {
            matches!(result as u32 as i32, 0 | 1 | -5)
                && !(kind == Kind::Deflate
                    && result == 1
                    && io.is_some_and(|i| i.out_after == i.out_len))
        }
        Kind::Xz => matches!(result, 0..=4 | 10),
        _ => unsafe { zstd_sys::ZSTD_isError(result as usize) == 0 },
    };
    if codec::processes(op) && publishable && events == (pair.a.heap.events, pair.b.heap.events) {
        let changed = pair.changed(m);
        if let Some(mut writes) = changed {
            if let Some(extra) = public_output(m, &mut pair, io.unwrap()) {
                writes.extend(extra);
                if writes
                    .iter()
                    .all(|(at, b)| m.native_output_range(*at, b.len()).unwrap_or(false))
                {
                    for (at, bytes) in writes {
                        m.fill_with(at, bytes.len(), {
                            let mut at = 0;
                            move |out| {
                                out.copy_from_slice(&bytes[at..at + out.len()]);
                                at += out.len();
                            }
                        })?;
                    }
                    if let Some(guard) = guards(m, kind, pair.guest) {
                        pair.guard = guard;
                        shared.ready(pair);
                    }
                    metrics.stream_native_steps += 1;
                    metrics.stream_native_operations[kind as usize] += 1;
                    return Ok(done(result));
                }
            }
        }
    }
    shared.pending(Pending {
        frame: args[0],
        op,
        args: a,
        pair,
        io,
    });
    Ok(fallback())
}
fn guards(m: &impl NativeMemory, kind: Kind, root: u64) -> Option<Vec<(u64, Vec<u8>)>> {
    Some(match kind {
        Kind::Deflate | Kind::Inflate => vec![
            (root + 56, read(m, root + 56, 32)?),
            (root + 16, read(m, root + 16, 8)?),
            (root + 40, read(m, root + 40, 16)?),
            (root + 88, read(m, root + 88, 4)?),
            (root + 96, read(m, root + 96, 8)?),
        ],
        Kind::Xz => vec![
            (root + 48, read(m, root + 48, 16)?),
            (root + 16, read(m, root + 16, 8)?),
            (root + 40, read(m, root + 40, 8)?),
        ],
        _ => Vec::new(),
    })
}
// I/O helpers are kept separate from the allocation-graph projection.
fn external(shared: &Shared, pair: &mut Pair, guest: u64, len: usize) -> Option<u64> {
    if guest == 0 {
        return (len == 0).then_some(0);
    }
    let end = guest.checked_add(len as u64)?;
    for e in &pair.external {
        let old_end = e.guest.checked_add(e.a.length as u64)?;
        if guest >= e.guest && end <= old_end {
            return Some(e.id);
        }
        if guest < old_end && end > e.guest {
            return None;
        }
    }
    pair.external.try_reserve(1).ok()?;
    let id = pair.next_io;
    pair.next_io = pair.next_io.checked_add(1)?;
    let a = Block::new(id, len, 0, shared.0.budget.clone())?;
    let b = Block::new(id, len, 0, shared.0.budget.clone())?;
    pair.external.push(External { id, guest, a, b });
    Some(id)
}
fn pointers(pair: &Pair, id: u64, guest: u64) -> Option<(*mut u8, *mut u8)> {
    if id == 0 {
        return Some((std::ptr::null_mut(), std::ptr::null_mut()));
    }
    let e = pair.external.iter().find(|e| e.id == id)?;
    let offset = usize::try_from(guest.checked_sub(e.guest)?).ok()?;
    if offset > e.a.length {
        return None;
    }
    Some(unsafe { (e.a.pointer.add(offset), e.b.pointer.add(offset)) })
}
fn sync(m: &impl NativeMemory, pair: &mut Pair, id: u64, guest: u64, len: usize) -> Option<()> {
    if id == 0 {
        return (len == 0).then_some(());
    }
    let e = pair.external.iter_mut().find(|e| e.id == id)?;
    let offset = usize::try_from(guest.checked_sub(e.guest)?).ok()?;
    if offset.checked_add(len)? > e.a.length {
        return None;
    }
    let mut at = 0;
    while at < len {
        let n = (len - at).min(4096 - ((guest + at as u64) & 4095) as usize);
        m.with_read_chunk(guest + at as u64, n, |bytes| unsafe {
            if bytes.len() != n {
                return false;
            }
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                e.a.pointer.add(offset + at),
                bytes.len(),
            );
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                e.b.pointer.add(offset + at),
                bytes.len(),
            );
            true
        })
        .ok()
        .filter(|r| r.0)?;
        at += n;
    }
    Some(())
}
fn prepare_io(
    shared: &Shared,
    m: &impl NativeMemory,
    pair: &mut Pair,
    op: u64,
    a: [u64; 9],
) -> Option<Io> {
    let mut io = Io {
        input: 0,
        in_len: 0,
        in_pos: 0,
        output: 0,
        out_len: 0,
        out_pos: 0,
        input_record: 0,
        output_record: 0,
        totals: [0; 3],
        input_id: 0,
        output_id: 0,
        in_after: 0,
        out_after: 0,
    };
    match pair.a.kind {
        Kind::Deflate | Kind::Inflate | Kind::Xz => {
            let bytes = read(m, a[0], if pair.a.kind == Kind::Xz { 64 } else { 104 })?;
            io.input = u64at(&bytes, 0)?;
            io.output = u64at(&bytes, 24)?;
            if pair.a.kind == Kind::Xz {
                io.in_len = usize::try_from(u64at(&bytes, 8)?).ok()?;
                io.out_len = usize::try_from(u64at(&bytes, 32)?).ok()?;
            } else {
                io.in_len = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
                io.out_len = u32::from_le_bytes(bytes[32..36].try_into().ok()?) as usize;
                io.totals[2] = u64at(&bytes, 96)?;
            }
            io.totals[0] = u64at(&bytes, 16)?;
            io.totals[1] = u64at(&bytes, 40)?;
        }
        _ => {
            io.output_record = a[1];
            let out = read(m, a[1], 24)?;
            io.output = u64at(&out, 0)?;
            io.out_len = usize::try_from(u64at(&out, 8)?).ok()?;
            io.out_pos = usize::try_from(u64at(&out, 16)?).ok()?;
            if !matches!(op, 31 | 32) {
                io.input_record = a[2];
                let input = read(m, a[2], 24)?;
                io.input = u64at(&input, 0)?;
                io.in_len = usize::try_from(u64at(&input, 8)?).ok()?;
                io.in_pos = usize::try_from(u64at(&input, 16)?).ok()?;
            }
        }
    }
    // A cold Zstd decoder owns a multi-megabyte history allocation. Small
    // compressed fragments are faster in the original interpreter than the
    // full-state cache admission/projection. Retiring the cache here preserves
    // the original state and avoids paying that cost on every tiny fragment.
    if pair.a.kind == Kind::ZstdD && io.in_len.saturating_sub(io.in_pos) < 64 * 1024 {
        return None;
    }
    if io.in_pos > io.in_len || io.out_pos > io.out_len {
        return None;
    }
    let ie = io.input.checked_add(io.in_len as u64)?;
    let oe = io.output.checked_add(io.out_len as u64)?;
    if io.in_len != 0 && io.out_len != 0 && io.input < oe && io.output < ie {
        return None;
    }
    if pair.overlaps_state(io.input, io.in_len) || pair.overlaps_state(io.output, io.out_len) {
        return None;
    }
    for record in [io.input_record, io.output_record] {
        if record != 0
            && (pair.overlaps_state(record, 24)
                || (record < oe && io.output < record.checked_add(24)?))
        {
            return None;
        }
    }
    if io.input_record != 0
        && io.output_record != 0
        && io.input_record < io.output_record.checked_add(24)?
        && io.output_record < io.input_record.checked_add(24)?
    {
        return None;
    }
    for record in [io.input_record, io.output_record] {
        if record != 0 && io.in_len != 0 && record < ie && io.input < record.checked_add(24)? {
            return None;
        }
    }
    // Caller buffers and retained byte-history buffers have ordinary guest
    // ownership. Synchronize their bytes, never an opaque C allocation.
    let retained: Vec<_> = pair
        .external
        .iter()
        .map(|e| (e.id, e.guest, e.a.length))
        .collect();
    for (id, g, n) in retained {
        sync(m, pair, id, g, n)?;
    }
    io.input_id = external(shared, pair, io.input, io.in_len)?;
    io.output_id = external(shared, pair, io.output, io.out_len)?;
    sync(m, pair, io.input_id, io.input, io.in_len)?;
    io.in_after = io.in_pos;
    io.out_after = io.out_pos;
    Some(io)
}
fn advance(
    shared: &Shared,
    m: &impl NativeMemory,
    pair: &mut Pair,
    op: u64,
    a: [u64; 9],
    io: &mut Option<Io>,
) -> Option<u64> {
    let mut ai = codec::Buffer {
        data: std::ptr::null_mut(),
        size: 0,
        pos: 0,
    };
    let mut bi = codec::Buffer {
        data: std::ptr::null_mut(),
        size: 0,
        pos: 0,
    };
    let mut ao = codec::Buffer {
        data: std::ptr::null_mut(),
        size: 0,
        pos: 0,
    };
    let mut bo = codec::Buffer {
        data: std::ptr::null_mut(),
        size: 0,
        pos: 0,
    };
    if let Some(i) = io.as_mut() {
        let (ap, bp) = pointers(pair, i.input_id, i.input)?;
        let (aq, bq) = pointers(pair, i.output_id, i.output)?;
        ai = codec::Buffer {
            data: ap.cast(),
            size: i.in_len,
            pos: i.in_pos,
        };
        bi = codec::Buffer {
            data: bp.cast(),
            size: i.in_len,
            pos: i.in_pos,
        };
        ao = codec::Buffer {
            data: aq.cast(),
            size: i.out_len,
            pos: i.out_pos,
        };
        bo = codec::Buffer {
            data: bq.cast(),
            size: i.out_len,
            pos: i.out_pos,
        };
        if !pair.a.io(ap, i.in_len, aq, i.out_len, i.totals)
            || !pair.b.io(bp, i.in_len, bq, i.out_len, i.totals)
        {
            return None;
        }
    }
    let (mut ad, mut bd) = (std::ptr::null_mut(), std::ptr::null_mut());
    if matches!(op, 10 | 11) {
        let len = a[2] as u32 as usize;
        let id = external(shared, pair, a[1], len)?;
        sync(m, pair, id, a[1], len)?;
        (ad, bd) = pointers(pair, id, a[1])?;
    }
    // Stay within the ordinary default profile. Advanced strategies and
    // multithread/dictionary state continue through original guest code.
    if op == 23 && !(0..=9).contains(&(a[1] as i32)) {
        return None;
    }
    if op == 29 && !matches!(a[1], 100 | 101 | 200..=202) {
        return None;
    }
    if op == 29 && a[1] == 100 && !(0..=9).contains(&(a[2] as i32)) {
        return None;
    }
    if op == 30 && a[1] != 100 {
        return None;
    }
    if !pair.a.call(op, a, &mut ai, &mut ao, ad) || !pair.b.call(op, a, &mut bi, &mut bo, bd) {
        return None;
    }
    if pair.a.result != pair.b.result {
        return None;
    }
    if let Some(i) = io.as_mut() {
        if let Some(p) = pair.a.progress() {
            if Some(p) != pair.b.progress() || p.0 > i.in_len || p.1 > i.out_len {
                return None;
            }
            i.in_after = i.in_len - p.0;
            i.out_after = i.out_len - p.1;
        } else {
            if ai.pos != bi.pos || ao.pos != bo.pos {
                return None;
            }
            i.in_after = ai.pos;
            i.out_after = ao.pos;
        }
        if i.in_after < i.in_pos
            || i.in_after > i.in_len
            || i.out_after < i.out_pos
            || i.out_after > i.out_len
        {
            return None;
        }
    }
    if pair.a.gzip {
        pair.a.linux_gzip_pending()?;
        pair.b.linux_gzip_pending()?;
        if let Some(i) = io.as_ref() {
            let produced = i.out_after.checked_sub(i.out_pos)?;
            if i.totals[1] <= 9 && i.totals[1].checked_add(produced as u64)? > 9 {
                let e = pair.external.iter().find(|e| e.id == i.output_id)?;
                let at = usize::try_from(i.output.checked_sub(e.guest)?)
                    .ok()?
                    .checked_add(i.out_pos)?
                    .checked_add((9 - i.totals[1]) as usize)?;
                if at >= e.a.length {
                    return None;
                }
                unsafe {
                    *e.a.pointer.add(at) = 3;
                    *e.b.pointer.add(at) = 3;
                }
            }
        }
    }
    Some(pair.a.result)
}
fn public_output(m: &impl NativeMemory, pair: &mut Pair, io: Io) -> Option<Vec<(u64, Vec<u8>)>> {
    let mut out = Vec::new();
    if io.output_id != 0 {
        let e = pair.external.iter().find(|e| e.id == io.output_id)?;
        let offset = usize::try_from(io.output.checked_sub(e.guest)?).ok()?;
        let start = offset.checked_add(io.out_pos)?;
        let end = offset.checked_add(io.out_after)?;
        let a = e.a.bytes().get(start..end)?;
        let b = e.b.bytes().get(start..end)?;
        if a != b {
            return None;
        }
        if !a.is_empty() {
            out.push((io.output.checked_add(io.out_pos as u64)?, a.to_vec()));
        }
    }
    match pair.a.kind {
        Kind::Deflate | Kind::Inflate | Kind::Xz => {
            let p = pair.a.progress()?;
            if Some(p) != pair.b.progress() {
                return None;
            }
            let mut bytes = read(m, pair.guest, 48)?;
            bytes[0..8].copy_from_slice(&io.input.checked_add(io.in_after as u64)?.to_le_bytes());
            bytes[24..32]
                .copy_from_slice(&io.output.checked_add(io.out_after as u64)?.to_le_bytes());
            if pair.a.kind == Kind::Xz {
                bytes[8..16].copy_from_slice(&(p.0 as u64).to_le_bytes());
                bytes[32..40].copy_from_slice(&(p.1 as u64).to_le_bytes());
            } else {
                bytes[8..12].copy_from_slice(&(p.0 as u32).to_le_bytes());
                bytes[32..36].copy_from_slice(&(p.1 as u32).to_le_bytes());
            }
            bytes[16..24].copy_from_slice(&p.2.to_le_bytes());
            bytes[40..48].copy_from_slice(&p.3.to_le_bytes());
            out.push((pair.guest, bytes));
            if pair.a.kind != Kind::Xz {
                out.push((
                    pair.guest + 48,
                    pair.message(u64::MAX)?.to_le_bytes().to_vec(),
                ));
                out.push((pair.guest + 88, p.5.to_le_bytes().to_vec()));
                out.push((pair.guest + 96, p.4.to_le_bytes().to_vec()));
            }
        }
        _ => {
            if io.input_record != 0 {
                out.push((
                    io.input_record + 16,
                    (io.in_after as u64).to_le_bytes().to_vec(),
                ));
            }
            out.push((
                io.output_record + 16,
                (io.out_after as u64).to_le_bytes().to_vec(),
            ));
        }
    }
    Some(out)
}
fn validate_output(m: &impl NativeMemory, pair: &mut Pair, io: Io) -> bool {
    if matches!(pair.a.kind, Kind::Deflate | Kind::Inflate) {
        let Some(msg) = read(m, pair.guest + 48, 8).and_then(|b| u64at(&b, 0)) else {
            return false;
        };
        if pair.message(msg).is_none() {
            return false;
        }
    }
    let Some(writes) = public_output(m, pair, io) else {
        return false;
    };
    writes
        .iter()
        .all(|(at, b)| read(m, *at, b.len()).as_deref() == Some(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use trames_memory::{Memory, Permissions};
    #[test]
    fn invalid_frames_and_missing_post_calls_preserve_original_return() {
        let cache = Shared::default();
        let mut m = Memory::default();
        let mut metrics = Metrics::default();
        assert!(
            execute(&cache, &mut m, [u64::MAX, 0, 0], &mut metrics)
                .unwrap()
                .fallback
        );
        let returned = execute(&cache, &mut m, [u64::MAX, 1, 0xffff_ffff], &mut metrics).unwrap();
        assert!(!returned.fallback);
        assert_eq!(returned.value, 0xffff_ffff);
        assert_eq!(metrics.stream_native_steps, 0);
    }
    #[cfg(all(target_arch = "aarch64", unix))]
    #[test]
    fn bundled_provider_layouts_cover_apple_and_elf_trace_builds() {
        let cache = Shared::default();
        for (op, abi, params) in [
            (1, 1, [6, 8, 15, 8, 0]),
            (2, 2, [15, 0, 0, 0, 0]),
            (12, 4, [3, 4, 0, 0, 0]),
            (14, 4, [64 * 1024 * 1024, 0, 0, 0, 0]),
            (19, 3, [0; 5]),
            (20, 5, [0; 5]),
        ] {
            let mut args = [0; 9];
            args[1..6].copy_from_slice(&params);
            assert!(cache.begin(op, args, abi).is_some(), "op {op} abi {abi}");
        }
    }
    #[cfg(all(target_arch = "aarch64", unix))]
    #[test]
    fn zstd_default_level_zero_uses_the_reviewed_default_profile() {
        let cache = Shared::default();
        let mut pair = cache.begin(19, [0; 9], 3).unwrap();
        assert_eq!(
            advance(&cache, &Memory::default(), &mut pair, 23, [0; 9], &mut None),
            Some(0)
        );
    }
    #[test]
    fn heap_alignment_budget_and_all_callback_events_are_accounted() {
        let budget = Arc::new(Budget::default());
        let mut h = heap::Heap::new(0xa5, budget.clone());
        h.alignment = Some(48);
        let a = h.allocate(32769);
        assert!(!a.is_null());
        assert_eq!(a as usize % 64, 48);
        assert_eq!(h.events, 1);
        h.free(a);
        assert_eq!(h.events, 2);
        h.free(std::ptr::null_mut());
        assert_eq!(h.events, 3);
        let mut reservation = heap::Reservation::new(budget.clone());
        assert!(reservation.resize(heap::LIMIT));
        assert!(h.allocate(1).is_null());
        assert_eq!(h.events, 4);
        drop(reservation);
        assert!(!h.allocate(1).is_null());
    }
    #[cfg(all(target_arch = "aarch64", unix))]
    #[test]
    fn corrupted_guest_opaque_pointer_declines_before_native_execution() {
        let cache = Shared::default();
        let mut args = [0; 9];
        args[1..6].copy_from_slice(&[6, 8, 15, 8, 0]);
        let mut pair = cache.begin(1, args, 1).unwrap();
        let mut m = Memory::default();
        m.map(0x2000, 4096, Permissions::RW).unwrap();
        pair.test_guest_image(&mut m, 0x2000);
        assert!(pair.learn(&m, 0x2000).is_some());
        assert!(pair.matches(&m));
        // A guest must never be able to turn this into a host pointer. The
        // cache compares it with its own valid C state and declines the call.
        let state = u64::from_le_bytes(m.read_array::<8>(0x2038).unwrap());
        m.write(state, &u64::MAX.to_le_bytes()).unwrap();
        cache.ready(pair);
        let mut frame = [0u8; 72];
        frame[..8].copy_from_slice(&0x2000u64.to_le_bytes());
        frame[64..].copy_from_slice(&((1u64 << 16) | 3).to_le_bytes());
        m.write(0x2800, &frame).unwrap();
        let mut metrics = Metrics::default();
        assert!(
            execute(&cache, &mut m, [0x2800, 0, 0], &mut metrics)
                .unwrap()
                .fallback
        );
        assert_eq!(metrics.stream_native_steps, 0);
        assert_eq!(metrics.stream_cache_misses, 1);
        assert_eq!(m.read_array::<8>(state).unwrap(), u64::MAX.to_le_bytes());
    }
}
