// SPDX-License-Identifier: GPL-3.0-or-later
//! Only native-initialized contexts execute. Guest opaque bytes are compared
//! to a projection, never reinterpreted as C state or made into host pointers.
use super::{
    codec::{Kind, Native, ALLOCATOR, ROOT},
    heap::{Block, Reservation},
    image,
    schema::Layout,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};
use trames_memory::NativeMemory;
pub(super) const IO_ID: u64 = 1 << 63;
pub(super) struct External {
    pub id: u64,
    pub guest: u64,
    pub a: Block,
    pub b: Block,
}
#[derive(Clone, Copy)]
struct Region {
    id: u64,
    a: *mut u8,
    b: *mut u8,
    nlen: usize,
    glen: usize,
    guest: Option<u64>,
    owned: bool,
    layout: Option<&'static Layout>,
    skip: Option<(usize, usize)>,
}
impl Region {
    fn offset(self, n: usize) -> Option<usize> {
        if n == self.nlen {
            return Some(self.glen);
        }
        if let Some(l) = self.layout {
            l.pieces.iter().find_map(|(np, gp, len)| {
                if n >= *np && n - *np < *len {
                    Some(*gp + n - *np)
                } else {
                    None
                }
            })
        } else {
            (n < self.nlen).then_some(n)
        }
    }
    fn pieces(self) -> Vec<(usize, usize, usize)> {
        if let Some((at, len)) = self.skip {
            return [(0, 0, at), (at + len, at + len, self.nlen - at - len)]
                .into_iter()
                .filter(|p| p.2 != 0)
                .collect();
        }
        self.layout
            .map_or_else(|| vec![(0, 0, self.nlen)], |l| l.pieces.to_vec())
    }
}
#[derive(Clone, Copy)]
struct Exact {
    a: u64,
    b: u64,
    guest: u64,
}
struct Snapshot {
    id: u64,
    guest: u64,
    a: Vec<u8>,
    b: Vec<u8>,
    bytes: Vec<u8>,
}
pub(super) struct Pair {
    pub a: Native,
    pub b: Native,
    pub guest: u64,
    pub abi: u64,
    pub controls: Vec<(u64, [u64; 9])>,
    pub started: bool,
    pub learned: bool,
    pub external: Vec<External>,
    pub next_io: u64,
    pub guard: Vec<(u64, Vec<u8>)>,
    reservation: Reservation,
    snapshots: Vec<Snapshot>,
    maps: BTreeMap<u64, u64>,
    exact: BTreeMap<(u64, usize), Exact>,
    padding: BTreeMap<u64, Vec<(usize, u8)>>,
    malloc: BTreeMap<u64, Vec<Option<u8>>>,
    images: Vec<(u64, u64)>,
    io_refs: RefCell<BTreeSet<u64>>,
}
unsafe impl Send for Pair {}
fn word(p: *const u8, at: usize) -> u64 {
    unsafe { u64::from_le(std::ptr::read_unaligned(p.add(at).cast::<u64>())) }
}
fn read(m: &impl NativeMemory, at: u64, len: usize) -> Option<Vec<u8>> {
    let mut b = Vec::new();
    b.try_reserve_exact(len).ok()?;
    b.resize(len, 0);
    m.native_read(at, &mut b).ok()?;
    Some(b)
}
impl Pair {
    pub fn new(a: Native, b: Native, abi: u64) -> Option<Self> {
        if a.kind != b.kind {
            return None;
        }
        super::schema::select(
            a.kind,
            abi,
            a.heap.blocks.iter().find(|r| r.pointer == a.root())?.length,
        )?;
        let mut images = Vec::new();
        for probe in a.image_probes() {
            let span = image::bounds(probe)?;
            if !images.contains(&span) {
                images.push(span);
            }
        }
        let reservation = Reservation::new(a.heap.budget.clone());
        let p = Self {
            a,
            b,
            guest: 0,
            abi,
            controls: Vec::new(),
            started: false,
            learned: false,
            external: Vec::new(),
            next_io: IO_ID,
            guard: Vec::new(),
            reservation,
            snapshots: Vec::new(),
            maps: BTreeMap::new(),
            exact: BTreeMap::new(),
            padding: BTreeMap::new(),
            malloc: BTreeMap::new(),
            images,
            io_refs: RefCell::new(BTreeSet::new()),
        };
        p.regions()?;
        Some(p)
    }
    fn regions(&self) -> Option<Vec<Region>> {
        if self.a.heap.blocks.len() != self.b.heap.blocks.len() {
            return None;
        }
        let root_a = self.a.root();
        let root_b = self.b.root();
        let mut r = Vec::new();
        let root_layout = super::schema::select(
            self.a.kind,
            self.abi,
            self.a
                .heap
                .blocks
                .iter()
                .find(|r| r.pointer == root_a)?
                .length,
        )?;
        for a in &self.a.heap.blocks {
            let b = self.b.heap.block(a.id)?;
            if a.length != b.length {
                return None;
            }
            let root = a.pointer == root_a;
            if root != (b.pointer == root_b) {
                return None;
            }
            let layout = if root { root_layout } else { None };
            if layout.is_some_and(|l| l.native_len != a.length) {
                return None;
            }
            // xz 5.8 easy_encoder uses one LZMA2 filter. filters[] after its
            // terminator, and the reserved LZMA2 options bytes (including the
            // copy in lzma_lzma2_coder.opt_cur), may contain copied stack
            // garbage. Preserve the guest bytes; never infer pointers there.
            // Sizes/offsets are checked against these pinned upstream structs.
            let skip = if self.a.encoder
                && a.length == 1504
                && a.pointer as u64 == word(root_a, 0)
                && b.pointer as u64 == word(root_b, 0)
            {
                let end = (0..5).find(|i| {
                    word(a.pointer, 296 + i * 16) == u64::MAX
                        && word(b.pointer, 296 + i * 16) == u64::MAX
                })?;
                Some((296 + end * 16 + 8, 376 - (296 + end * 16 + 8)))
            } else if self.a.encoder && a.length == 65704 {
                Some((64, 64))
            } else if self.a.encoder && a.length == 112 {
                let coder_a = word(root_a, 0) as *const u8;
                let coder_b = word(root_b, 0) as *const u8;
                if self
                    .a
                    .heap
                    .blocks
                    .iter()
                    .any(|r| std::ptr::eq(r.pointer, coder_a) && r.length == 1504)
                    && self
                        .b
                        .heap
                        .blocks
                        .iter()
                        .any(|r| std::ptr::eq(r.pointer, coder_b) && r.length == 1504)
                    && word(coder_a, 296) == 33
                    && word(coder_b, 296) == 33
                    && word(coder_a, 304) == a.pointer as u64
                    && word(coder_b, 304) == b.pointer as u64
                {
                    Some((48, 64))
                } else {
                    None
                }
            } else {
                None
            };
            r.push(Region {
                id: a.id,
                a: a.pointer,
                b: b.pointer,
                nlen: a.length,
                glen: layout.map_or(a.length, |l| l.guest_len),
                guest: self.maps.get(&a.id).copied(),
                owned: true,
                layout,
                skip,
            });
        }
        for (a, b) in self.a.special().into_iter().zip(self.b.special()) {
            if a.0 != b.0 || a.2 != b.2 {
                return None;
            }
            if a.1 != 0 {
                r.push(Region {
                    id: a.0,
                    a: a.1 as _,
                    b: b.1 as _,
                    nlen: a.2,
                    glen: a.2,
                    guest: Some(if a.0 == ROOT { self.guest } else { 0 }),
                    owned: false,
                    layout: None,
                    skip: None,
                });
            }
        }
        for e in &self.external {
            r.push(Region {
                id: e.id,
                a: e.a.pointer,
                b: e.b.pointer,
                nlen: e.a.length,
                glen: e.a.length,
                guest: Some(e.guest),
                owned: false,
                layout: None,
                skip: None,
            });
        }
        Some(r)
    }
    fn static_pointer(&self, v: u64) -> bool {
        self.images.iter().any(|(lo, hi)| v >= *lo && v < *hi)
    }
    fn pointers(regions: &[Region], a: u64, b: u64) -> Vec<(u64, usize)> {
        if a == b || (a == 0xa5a5a5a5a5a5a5a5 && b == 0x5a5a5a5a5a5a5a5a) {
            return Vec::new();
        }
        let interior = regions.iter().any(|r| {
            a >= r.a as u64
                && a < (r.a as u64).saturating_add(r.nlen as u64)
                && b >= r.b as u64
                && b < (r.b as u64).saturating_add(r.nlen as u64)
        });
        regions
            .iter()
            .filter_map(|r| {
                let x = a.checked_sub(r.a as u64)?;
                let y = b.checked_sub(r.b as u64)?;
                if x != y || x > r.nlen as u64 || (interior && x == r.nlen as u64) {
                    return None;
                }
                Some((r.id, r.offset(x as usize)?))
            })
            .collect()
    }
    fn affine(regions: &[Region], a: u64, b: u64, g: u64) -> Vec<u64> {
        regions
            .iter()
            .filter_map(|r| {
                if r.layout.is_some() {
                    return None;
                }
                let base = r.guest?;
                if base == 0 {
                    return None;
                }
                let x = a as i128 - r.a as u64 as i128;
                let y = b as i128 - r.b as u64 as i128;
                (x == y && x.abs() <= super::heap::LIMIT as i128 && base as i128 + x == g as i128)
                    .then_some(r.id)
            })
            .collect()
    }
    fn reference(&self, id: u64) {
        if (IO_ID..ALLOCATOR).contains(&id) {
            self.io_refs.borrow_mut().insert(id);
        }
    }
    fn words(r: Region) -> impl Iterator<Item = (usize, usize)> {
        r.pieces().into_iter().flat_map(|(n, g, len)| {
            let first = (8 - n % 8) % 8;
            (first..len)
                .step_by(8)
                .take_while(move |at| at + 8 <= len)
                .map(move |at| (n + at, g + at))
        })
    }
    /// Original guest code completes first. The two native executions have
    /// different heaps and malloc fill patterns. Initialized scalars must
    /// match; bytes left untouched by both project the original guest malloc bytes.
    /// Guest bytes are never copied into either native opaque state.
    pub fn learn(&mut self, m: &impl NativeMemory, guest: u64) -> Option<()> {
        let bytes = self
            .regions()?
            .iter()
            .filter(|r| r.owned)
            .try_fold(0usize, |sum, r| {
                sum.checked_add(r.nlen.checked_mul(4)?.checked_add(r.glen.checked_mul(2)?)?)
            })?;
        if !self.reservation.resize(bytes.checked_add(512 * 1024)?) {
            return None;
        }
        self.learned = false;
        self.guest = guest;
        self.maps.clear();
        self.exact.clear();
        self.padding.clear();
        self.malloc.clear();
        self.guard.clear();
        self.io_refs.borrow_mut().clear();
        let root = match self.a.kind {
            Kind::Deflate | Kind::Inflate | Kind::Xz => {
                u64::from_le_bytes(m.native_read_array::<8>(guest.checked_add(56)?).ok()?)
            }
            _ => guest,
        };
        let regions = self.regions()?;
        let roots = Self::pointers(&regions, self.a.root() as u64, self.b.root() as u64);
        if roots.len() != 1 {
            return None;
        }
        self.maps
            .insert(roots[0].0, root.checked_sub(roots[0].1 as u64)?);
        let mut visited = BTreeSet::new();
        loop {
            let regions = self.regions()?;
            let Some(r) = regions
                .iter()
                .find(|r| r.owned && r.guest.is_some() && !visited.contains(&r.id))
                .copied()
            else {
                break;
            };
            let bytes = read(m, r.guest?, r.glen)?;
            for (n, g) in Self::words(r) {
                let (a, b) = (word(r.a, n), word(r.b, n));
                let refs = Self::pointers(&regions, a, b);
                if refs.len() > 1 {
                    return None;
                }
                if let Some((id, offset)) = refs.first().copied() {
                    let v = u64::from_le_bytes(bytes[g..g + 8].try_into().ok()?);
                    let target = regions.iter().find(|r| r.id == id)?;
                    if target.owned {
                        let base = v.checked_sub(offset as u64)?;
                        if self.maps.get(&id).is_some_and(|v| *v != base) {
                            return None;
                        }
                        self.maps.insert(id, base);
                    } else if target.guest?.checked_add(offset as u64)? != v {
                        return None;
                    }
                }
            }
            visited.insert(r.id);
        }
        let regions = self.regions()?;
        if regions.iter().any(|r| r.owned && r.guest.is_none()) {
            return None;
        }
        for r in &regions {
            if !r.owned {
                continue;
            }
            let bytes = read(m, r.guest?, r.glen)?;
            if let Some((at, len)) = r.skip {
                self.padding
                    .insert(r.id, (at..at + len).map(|i| (i, bytes[i])).collect());
            }
            if let Some(layout) = r.layout {
                for (at, len) in layout.zero {
                    if bytes.get(*at..at + len)?.iter().any(|v| *v != 0) {
                        return None;
                    }
                }
                let mut covered = vec![false; r.glen];
                for (_, g, len) in layout.pieces {
                    covered[*g..g + len].fill(true);
                }
                self.padding.insert(
                    r.id,
                    covered
                        .into_iter()
                        .enumerate()
                        .filter_map(|(at, c)| (!c).then_some((at, bytes[at])))
                        .collect(),
                );
            }
            for (start, gstart, len) in r.pieces() {
                let mut at = 0;
                while at < len {
                    let (n, g) = (start + at, gstart + at);
                    if n % 8 == 0 && len - at >= 8 {
                        let (a, b) = (word(r.a, n), word(r.b, n));
                        let v = u64::from_le_bytes(bytes[g..g + 8].try_into().ok()?);
                        if a == u64::from_le_bytes([self.a.heap.fill; 8])
                            && b == u64::from_le_bytes([self.b.heap.fill; 8])
                        {
                            let saved = self
                                .malloc
                                .entry(r.id)
                                .or_insert_with(|| vec![None; r.nlen]);
                            for j in 0..8 {
                                saved[n + j] = Some(bytes[g + j]);
                            }
                            at += 8;
                            continue;
                        }
                        let refs = Self::pointers(&regions, a, b);
                        if refs.len() > 1 {
                            return None;
                        }
                        if let Some((id, offset)) = refs.first() {
                            self.reference(*id);
                            if regions
                                .iter()
                                .find(|r| r.id == *id)?
                                .guest?
                                .checked_add(*offset as u64)?
                                != v
                            {
                                return None;
                            }
                        } else if a == b && self.static_pointer(a) {
                            self.exact.insert((r.id, n), Exact { a, b, guest: v });
                        } else if a != b && !Self::affine(&regions, a, b, v).is_empty() {
                            for id in Self::affine(&regions, a, b, v) {
                                self.reference(id);
                            }
                            self.exact.insert((r.id, n), Exact { a, b, guest: v });
                        } else {
                            for j in 0..8 {
                                self.learn_byte(*r, n + j, bytes[g + j])?;
                            }
                        }
                        at += 8;
                    } else {
                        self.learn_byte(*r, n, bytes[g])?;
                        at += 1;
                    }
                    if self.exact.len() > 4096 {
                        return None;
                    }
                }
            }
        }
        self.snapshots = self
            .regions()?
            .iter()
            .filter(|r| r.owned)
            .map(|r| {
                Some(Snapshot {
                    id: r.id,
                    guest: r.guest?,
                    a: unsafe { std::slice::from_raw_parts(r.a, r.nlen) }.to_vec(),
                    b: unsafe { std::slice::from_raw_parts(r.b, r.nlen) }.to_vec(),
                    bytes: read(m, r.guest?, r.glen)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        self.learned = true;
        Some(())
    }
    fn learn_byte(&mut self, r: Region, n: usize, guest: u8) -> Option<()> {
        let (a, b) = unsafe { (*r.a.add(n), *r.b.add(n)) };
        if a == b {
            if a != guest {
                return None;
            }
        } else if a == self.a.heap.fill && b == self.b.heap.fill {
            self.malloc
                .entry(r.id)
                .or_insert_with(|| vec![None; r.nlen])[n] = Some(guest);
        } else {
            return None;
        }
        Some(())
    }
    fn project_word(&self, regions: &[Region], r: Region, n: usize) -> Option<u64> {
        let (a, b) = (word(r.a, n), word(r.b, n));
        let refs = Self::pointers(regions, a, b);
        if refs.len() == 1 {
            let (id, offset) = refs[0];
            self.reference(id);
            regions
                .iter()
                .find(|r| r.id == id)?
                .guest?
                .checked_add(offset as u64)
        } else if refs.len() > 1 {
            None
        } else if a == b && !self.static_pointer(a) {
            Some(a)
        } else if let Some(e) = self.exact.get(&(r.id, n)).filter(|e| e.a == a && e.b == b) {
            for id in Self::affine(regions, a, b, e.guest) {
                self.reference(id);
            }
            Some(e.guest)
        } else {
            let mut bytes = [0; 8];
            for (j, v) in bytes.iter_mut().enumerate() {
                *v = self.project_byte(r, n + j)?;
            }
            Some(u64::from_le_bytes(bytes))
        }
    }
    fn project_byte(&self, r: Region, n: usize) -> Option<u8> {
        let (a, b) = unsafe { (*r.a.add(n), *r.b.add(n)) };
        if a == b {
            Some(a)
        } else if a == self.a.heap.fill && b == self.b.heap.fill {
            self.malloc.get(&r.id)?.get(n).copied().flatten()
        } else {
            None
        }
    }
    fn chunk(&self, regions: &[Region], r: Region, at: usize, len: usize) -> Option<Vec<u8>> {
        let mut out = vec![0; len];
        for (n, g, bytes) in r.pieces() {
            let low = g.max(at);
            let high = (g + bytes).min(at + len);
            if low >= high {
                continue;
            }
            let mut pos = low - g;
            while g + pos < high {
                let source = n + pos;
                let dest = g + pos - at;
                let remain = high - g - pos;
                if source % 8 == 0 && remain >= 8 {
                    out[dest..dest + 8]
                        .copy_from_slice(&self.project_word(regions, r, source)?.to_le_bytes());
                    pos += 8;
                } else {
                    out[dest] = self.project_byte(r, source)?;
                    pos += 1;
                }
            }
        }
        if let Some(padding) = self.padding.get(&r.id) {
            for (g, v) in padding {
                if *g >= at && *g < at + len {
                    out[*g - at] = *v;
                }
            }
        }
        Some(out)
    }
    pub fn align_workspace(&mut self, m: &impl NativeMemory) -> Option<()> {
        if self.a.kind != Kind::ZstdC || self.started || self.controls.len() > 16 {
            return None;
        }
        let root = self.guest;
        let workspace = u64::from_le_bytes(m.native_read_array::<8>(root.checked_add(704)?).ok()?);
        if workspace == 0 {
            return None;
        }
        let mut a = Native::new(19, [0; 9], 0xa5, self.a.heap.budget.clone())?;
        let mut b = Native::new(19, [0; 9], 0x5a, self.b.heap.budget.clone())?;
        a.heap.alignment = Some((workspace & 48) as usize);
        b.heap.alignment = a.heap.alignment;
        let mut input = super::codec::Buffer {
            data: std::ptr::null_mut(),
            size: 0,
            pos: 0,
        };
        let mut output = super::codec::Buffer {
            data: std::ptr::null_mut(),
            size: 0,
            pos: 0,
        };
        for (op, args) in &self.controls {
            if !a.call(*op, *args, &mut input, &mut output, std::ptr::null())
                || !b.call(*op, *args, &mut input, &mut output, std::ptr::null())
                || a.result != b.result
            {
                return None;
            }
        }
        self.a = a;
        self.b = b;
        Some(())
    }
    pub fn overlaps_state(&self, at: u64, len: usize) -> bool {
        if len == 0 {
            return false;
        }
        let Some(end) = at.checked_add(len as u64) else {
            return true;
        };
        let Some(regions) = self.regions() else {
            return true;
        };
        regions.iter().filter(|r| r.owned || r.id == ROOT).any(|r| {
            r.guest.is_some_and(|base| {
                base.checked_add(r.glen as u64)
                    .is_none_or(|last| at < last && base < end)
            })
        })
    }
    pub fn matches(&self, m: &impl NativeMemory) -> bool {
        for (at, b) in &self.guard {
            if read(m, *at, b.len()).as_deref() != Some(b) {
                return false;
            }
        }
        for r in &self.snapshots {
            let mut at = 0;
            while at < r.bytes.len() {
                let len = (r.bytes.len() - at).min(4096 - ((r.guest + at as u64) & 4095) as usize);
                if !m
                    .with_read_chunk(r.guest + at as u64, len, |b| b == &r.bytes[at..at + len])
                    .is_ok_and(|v| v.0)
                {
                    return false;
                }
                at += len;
            }
        }
        true
    }
    pub fn changed(&mut self, _m: &impl NativeMemory) -> Option<Vec<(u64, Vec<u8>)>> {
        if !self.learned {
            return None;
        }
        let regions = self.regions()?;
        if regions.iter().filter(|r| r.owned).count() != self.maps.len() {
            return None;
        }
        let mut writes = Vec::new();
        for r in &regions {
            if !r.owned {
                continue;
            }
            let base = r.guest?;
            let previous = self
                .snapshots
                .iter()
                .find(|s| s.id == r.id && s.guest == base)?;
            let a = unsafe { std::slice::from_raw_parts(r.a, r.nlen) };
            let b = unsafe { std::slice::from_raw_parts(r.b, r.nlen) };
            let first_write = writes.len();
            let mut refreshed = Vec::new();
            let mut at = 0;
            while at < r.glen {
                let len = (r.glen - at).min(4096 - ((base + at as u64) & 4095) as usize);
                let changed = r.pieces().iter().any(|(n, g, size)| {
                    let lo = (*g).max(at);
                    let hi = (*g + *size).min(at + len);
                    if lo >= hi {
                        return false;
                    }
                    let start = *n + lo - *g;
                    let end = start + hi - lo;
                    a[start..end] != previous.a[start..end]
                        || b[start..end] != previous.b[start..end]
                });
                if changed {
                    for (n, g, size) in r.pieces() {
                        let low = g.max(at);
                        let high = (g + size).min(at + len);
                        if low < high {
                            refreshed.push((n + low - g, n + high - g));
                        }
                    }
                    let projected = self.chunk(&regions, *r, at, len)?;
                    if projected != previous.bytes[at..at + len] {
                        writes.push((base + at as u64, projected));
                    }
                }
                at += len;
            }
            let previous = self.snapshots.iter_mut().find(|s| s.id == r.id)?;
            for (start, end) in refreshed {
                previous.a[start..end].copy_from_slice(&a[start..end]);
                previous.b[start..end].copy_from_slice(&b[start..end]);
            }
            for (at, bytes) in &writes[first_write..] {
                let offset = (*at - base) as usize;
                previous.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
            }
        }
        Some(writes)
    }
    pub fn message(&mut self, guest: u64) -> Option<u64> {
        let (a, b) = (self.a.progress()?.6, self.b.progress()?.6);
        if a == 0 && b == 0 {
            return Some(0);
        }
        if a != b || !self.static_pointer(a) {
            return None;
        }
        if guest != u64::MAX {
            self.exact.insert((ROOT, 48), Exact { a, b, guest });
        }
        self.exact
            .get(&(ROOT, 48))
            .filter(|e| e.a == a && e.b == b)
            .map(|e| e.guest)
    }
    pub fn prune_external(&mut self) {
        self.external
            .retain(|e| self.io_refs.borrow().contains(&e.id));
    }
}

#[cfg(test)]
impl Pair {
    pub(super) fn test_guest_image(&mut self, m: &mut trames_memory::Memory, root: u64) {
        use trames_memory::Permissions;
        self.guest = root;
        let mut at = 0x10000u64;
        for b in &self.a.heap.blocks {
            self.maps.insert(b.id, at);
            let pages = (b.length + 4095) & !4095;
            m.map(at, pages as u64, Permissions::RW).unwrap();
            at += pages as u64 + 4096;
            self.malloc.insert(b.id, vec![Some(0); b.length]);
        }
        let regions = self.regions().unwrap();
        for r in &regions {
            if !r.owned {
                continue;
            }
            for (n, _) in Self::words(*r) {
                let (a, b) = (word(r.a, n), word(r.b, n));
                if a == b && self.static_pointer(a) {
                    self.exact.insert(
                        (r.id, n),
                        Exact {
                            a,
                            b,
                            guest: 0x2000,
                        },
                    );
                }
            }
        }
        for r in &regions {
            if r.owned {
                let bytes = self.chunk(&regions, *r, 0, r.glen).unwrap();
                m.write(r.guest.unwrap(), &bytes).unwrap();
            }
        }
        let region = regions.iter().find(|r| r.a == self.a.root()).unwrap();
        m.write(root + 56, &region.guest.unwrap().to_le_bytes())
            .unwrap();
    }
}
