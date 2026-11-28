// SPDX-License-Identifier: GPL-3.0-or-later
//! Read only this process's already-loaded native codec image metadata.
use std::ffi::{c_char, c_int, c_void};
#[repr(C)]
struct DlInfo {
    name: *const c_char,
    base: *mut c_void,
    symbol: *const c_char,
    address: *mut c_void,
}
#[cfg_attr(target_os = "linux", link(name = "dl"))]
unsafe extern "C" {
    fn dladdr(symbol: *const c_void, info: *mut DlInfo) -> c_int;
}
pub(super) fn bounds(symbol: *const c_void) -> Option<(u64, u64)> {
    let mut info = std::mem::MaybeUninit::<DlInfo>::uninit();
    // dladdr does not dereference `symbol`. The returned image belongs to the
    // running host binary, never to guest memory or a guest-provided ELF.
    if unsafe { dladdr(symbol, info.as_mut_ptr()) } == 0 {
        return None;
    }
    let base = unsafe { info.assume_init().base.cast::<u8>() };
    if base.is_null() {
        return None;
    }
    let read32 =
        |offset| unsafe { u32::from_le(std::ptr::read_unaligned(base.add(offset).cast::<u32>())) };
    let read64 =
        |offset| unsafe { u64::from_le(std::ptr::read_unaligned(base.add(offset).cast::<u64>())) };
    #[cfg(target_vendor = "apple")]
    {
        if read32(0) != 0xfeedfacf {
            return None;
        }
        let commands = read32(16) as usize;
        let size = read32(20) as usize;
        if commands > 4096 || size > 16 * 1024 * 1024 {
            return None;
        }
        let mut at = 32;
        let mut segments = Vec::new();
        let mut anchor = None;
        for _ in 0..commands {
            if at + 8 > 32 + size {
                return None;
            }
            let command = read32(at);
            let length = read32(at + 4) as usize;
            if length < 8 || at + length > 32 + size {
                return None;
            }
            if command == 0x19 && length >= 72 && read32(at + 60) & 1 != 0 {
                let address = read64(at + 24);
                let bytes = read64(at + 32);
                if read64(at + 40) == 0 && read64(at + 48) > 0 {
                    anchor = Some(address);
                }
                segments.push((address, address.checked_add(bytes)?));
            }
            at += length;
        }
        let slide = base as u64 as i128 - anchor? as i128;
        let low = segments.iter().map(|p| p.0).min()? as i128 + slide;
        let high = segments.iter().map(|p| p.1).max()? as i128 + slide;
        Some((u64::try_from(low).ok()?, u64::try_from(high).ok()?))
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        if read32(0) != 0x464c457f || unsafe { *base.add(4) } != 2 {
            return None;
        }
        let u16at = |offset| unsafe {
            u16::from_le(std::ptr::read_unaligned(base.add(offset).cast::<u16>()))
        };
        let offset = usize::try_from(read64(32)).ok()?;
        let count = u16at(56) as usize;
        if u16at(54) != 56 || count > 1024 || offset.checked_add(count * 56)? > 16 * 1024 * 1024 {
            return None;
        }
        let bias = if u16at(16) == 3 { base as u64 } else { 0 };
        let mut low = u64::MAX;
        let mut high = 0;
        for i in 0..count {
            let at = offset + i * 56;
            if read32(at) == 1 && read32(at + 4) & 4 != 0 {
                let address = bias.checked_add(read64(at + 16))?;
                low = low.min(address);
                high = high.max(address.checked_add(read64(at + 40))?);
            }
        }
        (low < high).then_some((low, high))
    }
}
