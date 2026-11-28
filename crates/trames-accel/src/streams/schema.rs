// SPDX-License-Identifier: GPL-3.0-or-later
//! LP64 AArch64 maps from scripts/generate-stream-schema.py.
//! Only the current zlib 1.4 / zstd 1.6 implementations execute.
use super::codec::Kind;
pub(super) struct Layout {
    pub native_len: usize,
    pub guest_len: usize,
    pub pieces: &'static [(usize, usize, usize)],
    pub zero: &'static [(usize, usize)],
}
static ZLIB132_DEFLATE: Layout = Layout {
    native_len: 5968,
    guest_len: 5968,
    pieces: &[
        (0, 0, 12),
        (16, 16, 36),
        (56, 56, 17),
        (76, 76, 16),
        (96, 96, 52),
        (152, 152, 5729),
        (5888, 5888, 20),
        (5912, 5912, 26),
        (5940, 5940, 8),
        (5952, 5952, 12),
    ],
    zero: &[],
};
static ZLIB131_DEFLATE: Layout = Layout {
    native_len: 5968,
    guest_len: 5952,
    pieces: &[
        (0, 0, 12),
        (16, 16, 36),
        (56, 56, 17),
        (76, 76, 16),
        (96, 96, 52),
        (152, 152, 5729),
        (5888, 5888, 20),
        (5912, 5912, 26),
        (5940, 5940, 4),
        (5952, 5944, 8),
    ],
    zero: &[],
};
static ZLIB_INFLATE: Layout = Layout {
    native_len: 7176,
    guest_len: 7160,
    pieces: &[
        (0, 0, 20),
        (24, 20, 12),
        (40, 32, 24),
        (68, 56, 16),
        (88, 72, 7084),
    ],
    zero: &[],
};
static ZSTD157_C_G0_N0: Layout = Layout {
    native_len: 5272,
    guest_len: 5280,
    pieces: &[
        (0, 0, 8),
        (8, 16, 676),
        (688, 696, 2868),
        (3560, 3568, 192),
        (3752, 3768, 1184),
        (4944, 4952, 328),
    ],
    zero: &[(8, 4), (3760, 8)],
};
static ZSTD157_D_G0_N0: Layout = Layout {
    native_len: 95968,
    guest_len: 95984,
    pieces: &[
        (0, 0, 29884),
        (29888, 29888, 228),
        (30120, 30120, 60),
        (30184, 30184, 28),
        (30216, 30216, 96),
        (30312, 30328, 12),
        (30328, 30344, 65630),
        (95960, 95976, 8),
    ],
    zero: &[(30312, 8), (30320, 4), (30324, 4)],
};
static ZSTD157_C_G0_N1: Layout = Layout {
    native_len: 5280,
    guest_len: 5280,
    pieces: &[
        (0, 0, 8),
        (8, 16, 676),
        (688, 696, 2868),
        (3560, 3568, 192),
        (3760, 3768, 1184),
        (4952, 4952, 328),
    ],
    zero: &[(8, 4), (3760, 8)],
};
static ZSTD157_D_G0_N1: Layout = Layout {
    native_len: 95984,
    guest_len: 95984,
    pieces: &[
        (0, 0, 29884),
        (29888, 29888, 228),
        (30120, 30120, 60),
        (30184, 30184, 28),
        (30216, 30216, 96),
        (30312, 30328, 12),
        (30328, 30344, 65630),
        (95960, 95976, 8),
    ],
    zero: &[(30312, 8), (30320, 4), (30324, 4)],
};
static ZSTD157_C_G1_N0: Layout = Layout {
    native_len: 5272,
    guest_len: 5288,
    pieces: &[
        (0, 0, 8),
        (8, 16, 676),
        (688, 696, 2868),
        (3560, 3568, 192),
        (3752, 3776, 1184),
        (4944, 4960, 328),
    ],
    zero: &[(8, 4), (3760, 8), (3768, 8)],
};
static ZSTD157_D_G1_N0: Layout = Layout {
    native_len: 95968,
    guest_len: 95992,
    pieces: &[
        (0, 0, 29884),
        (29888, 29888, 228),
        (30120, 30120, 60),
        (30184, 30184, 28),
        (30216, 30216, 96),
        (30312, 30328, 12),
        (30328, 30344, 65630),
        (95960, 95976, 8),
    ],
    zero: &[(30312, 8), (30320, 4), (30324, 4), (95984, 8)],
};
static ZSTD157_C_G1_N1: Layout = Layout {
    native_len: 5280,
    guest_len: 5288,
    pieces: &[
        (0, 0, 8),
        (8, 16, 676),
        (688, 696, 2868),
        (3560, 3568, 192),
        (3752, 3768, 1192),
        (4952, 4960, 328),
    ],
    zero: &[(8, 4), (3760, 8)],
};
static ZSTD157_D_G1_N1: Layout = Layout {
    native_len: 95984,
    guest_len: 95992,
    pieces: &[
        (0, 0, 29884),
        (29888, 29888, 228),
        (30120, 30120, 60),
        (30184, 30184, 28),
        (30216, 30216, 96),
        (30312, 30328, 12),
        (30328, 30344, 65630),
        (95960, 95976, 16),
    ],
    zero: &[(30312, 8), (30320, 4), (30324, 4)],
};
pub(super) fn select(kind: Kind, abi: u64, native_len: usize) -> Option<Option<&'static Layout>> {
    Some(match (kind, abi, native_len) {
        (Kind::Deflate, 1, 5968) => Some(&ZLIB132_DEFLATE),
        (Kind::Deflate, 2, 5968) => Some(&ZLIB131_DEFLATE),
        (Kind::Inflate, 1 | 2, 7176) => Some(&ZLIB_INFLATE),
        (Kind::ZstdC, 3, 5272) => Some(&ZSTD157_C_G0_N0),
        (Kind::ZstdD, 3, 95968) => Some(&ZSTD157_D_G0_N0),
        (Kind::ZstdC, 3, 5280) => Some(&ZSTD157_C_G0_N1),
        (Kind::ZstdD, 3, 95984) => Some(&ZSTD157_D_G0_N1),
        (Kind::ZstdC, 5, 5272) => Some(&ZSTD157_C_G1_N0),
        (Kind::ZstdD, 5, 95968) => Some(&ZSTD157_D_G1_N0),
        (Kind::ZstdC, 5, 5280) => Some(&ZSTD157_C_G1_N1),
        (Kind::ZstdD, 5, 95984) => Some(&ZSTD157_D_G1_N1),
        (Kind::Xz, 4 | 6, _) => None,
        _ => return None,
    })
}
