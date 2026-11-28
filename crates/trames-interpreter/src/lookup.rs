// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed-width lookup metadata over the original vector bank; no extension state.
#[derive(Clone, Copy, Debug)]
pub struct Lookup {
    destination: u8,
    table: u8,
    indices: u8,
    index_bits: u8,
    element_bits: u8,
    segment: u8,
}
impl Lookup {
    pub fn decode(word: u32) -> Option<Self> {
        if word & 0xff208c00 != 0x4e000000 {
            return None;
        }
        let class = (word >> 22) & 3;
        let half = word & (1 << 12) != 0;
        let len = ((word >> 13) & 3) as u8;
        let (index_bits, element_bits, segment) = match class {
            1 if half => (4, 16, len),
            1 if len & 1 != 0 => (4, 8, len >> 1),
            2 if half => (2, 8, len),
            3 => (2, 16, (len << 1) | u8::from(half)),
            _ => return None,
        };
        Some(Self {
            destination: (word & 31) as u8,
            table: ((word >> 5) & 31) as u8,
            indices: ((word >> 16) & 31) as u8,
            index_bits,
            element_bits,
            segment,
        })
    }
    pub fn registers(self) -> usize {
        if self.index_bits == 4 && self.element_bits == 16 {
            2
        } else {
            1
        }
    }
    fn result(self, vectors: &[u128; 32]) -> u128 {
        let indices = vectors[self.indices as usize];
        let tables = [
            vectors[self.table as usize],
            vectors[(self.table as usize + 1) & 31],
        ];
        let lanes = 128 / self.element_bits as usize;
        let mut result = 0;
        for lane in 0..lanes {
            let index = ((indices
                >> ((lanes * self.segment as usize + lane) * self.index_bits as usize))
                & ((1 << self.index_bits) - 1)) as usize;
            let element = (tables[index / lanes] >> ((index % lanes) * self.element_bits as usize))
                & ((1 << self.element_bits) - 1);
            result |= element << (lane * self.element_bits as usize);
        }
        result
    }
}
impl super::Cpu {
    pub(super) fn apply_lookup(&mut self, lookup: Lookup) {
        self.vectors[lookup.destination as usize] = lookup.result(&self.vectors);
    }
}
