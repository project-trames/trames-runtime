// SPDX-License-Identifier: GPL-3.0-or-later
//! Test oracle consumes independent assembly fields, not the production decoder.
use trames_execution::Aarch64State;
pub fn forms() -> impl Iterator<Item = (u32, u32, u32, u32)> {
    include_str!("../../../docs/validation/jit-lut-20261128/forms.words")
        .lines()
        .map(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            (
                u32::from_str_radix(f[0], 16).unwrap(),
                f[1].parse().unwrap(),
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
            )
        })
}
pub fn state() -> Aarch64State {
    let mut state = Aarch64State {
        pc: 0x1000,
        sp: 0x2000,
        nzcv: 0xb0000000,
        fpsr: 0x08000010,
        tpidr_el0: 123,
        tpidr2_el0: 456,
        ..Default::default()
    };
    state.registers = std::array::from_fn(|r| (r as u64).wrapping_mul(0x123456789abcdef));
    state.vectors = std::array::from_fn(|r| {
        let seed = (r as u128 + 1).wrapping_mul(0x0123456789abcdef_fedcba9876543211);
        seed.rotate_left((r * 3) as u32) ^ ((r as u128) << 73)
    });
    state
}
pub fn expected(
    state: &Aarch64State,
    index_bits: u32,
    element_bits: u32,
    segment: u32,
    d: usize,
    n: usize,
    m: usize,
) -> Aarch64State {
    let indices = state.vectors[m].to_le_bytes();
    let mut table = [0u8; 32];
    table[..16].copy_from_slice(&state.vectors[n].to_le_bytes());
    table[16..].copy_from_slice(&state.vectors[(n + 1) & 31].to_le_bytes());
    let bytes = (element_bits / 8) as usize;
    let count = 16 / bytes;
    let mut result = [0u8; 16];
    for lane in 0..count {
        let bit = (segment as usize * count + lane) * index_bits as usize;
        let index = (indices[bit / 8] >> (bit % 8)) & ((1u8 << index_bits) - 1);
        let source = index as usize * bytes;
        result[lane * bytes..(lane + 1) * bytes].copy_from_slice(&table[source..source + bytes]);
    }
    let mut wanted = state.clone();
    wanted.vectors[d] = u128::from_le_bytes(result);
    wanted.pc = state.pc.wrapping_add(4);
    wanted
}
