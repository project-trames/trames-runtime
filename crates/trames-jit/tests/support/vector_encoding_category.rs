// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::cell::Cell;
use trames_memory::{Access, InstructionEpochs, MemoryError};
const REJECTED: &str =
    include_str!("../../../../docs/validation/jit-vector-encoding-20261128/rejected.words");
const CORRECTED: &str = include_str!(
    "../../../../docs/validation/jit-vector-encoding-20261128/reference-corrections.words"
);
fn install(m: &mut Memory, words: &[u32]) {
    m.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
    for (i, w) in words.iter().enumerate() {
        m.write_array(0x1000 + 4 * i as u64, w.to_le_bytes())
            .unwrap();
    }
    m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
}
#[test]
fn corrected_vector_errors_preserve_dirty_fp_gpr_flags_memory_prefix_and_retirement() {
    if !NATIVE_COMPILED {
        return;
    }
    for raw in CORRECTED.lines().step_by(31) {
        let word = u32::from_str_radix(raw, 16).unwrap();
        let mut words: Vec<_> = (0..31).map(|r| 0x91000400 | (r << 5) | r).collect();
        words.extend((0..32).map(|r| 0x4ea01c00 | (r << 16) | (r << 5) | r));
        words.extend_from_slice(&[0xb1000529, 0x1e622820, 0xf9400274, word]);
        let mut state = initial();
        state.registers[19] = 0x10000 - 1;
        for budget in [1, 2, 64] {
            for mode in [0, 1, 2] {
                let e = compare_memory_sequence_mode(&words, &state, budget, mode);
                assert_eq!(e.stats.fallback_instructions, 0, "{word:x}");
            }
        }
    }
}
struct Traced {
    memory: Memory,
    pages: Cell<u64>,
    fetches: Cell<u64>,
    limit: Cell<u64>,
    epochs: bool,
}
impl CpuMemory for Traced {
    fn mapped_at_or_after(&self, a: u64) -> bool {
        self.memory.mapped_at_or_after(a)
    }
    fn read(&self, _a: u64, _out: &mut [u8]) -> Result<(), MemoryError> {
        panic!("invalid FP/SIMD data read")
    }
    fn write(&mut self, _a: u64, _bytes: &[u8]) -> Result<(), MemoryError> {
        panic!("invalid FP/SIMD data write")
    }
    fn check(&self, _a: u64, _len: usize, _access: Access) -> Result<(), MemoryError> {
        panic!("invalid FP/SIMD permission query")
    }
    fn check_translation(&self, _a: u64) -> Result<(), MemoryError> {
        panic!("invalid FP/SIMD data translation")
    }
    fn fetch_u32(&self, a: u64) -> Result<u32, MemoryError> {
        self.fetches.set(self.fetches.get() + 1);
        self.memory.fetch_u32(a)
    }
    fn instruction_identity(&self) -> (u64, u64, u64) {
        self.memory.instruction_identity()
    }
    fn instruction_page_identity(&self, a: u64) -> Result<(u64, u64, u64), MemoryError> {
        self.pages.set(self.pages.get() + 1);
        assert!(
            self.pages.get() <= self.limit.get(),
            "extra decoder page query: interpreter fault entry"
        );
        self.memory.instruction_page_identity(a)
    }
    fn instruction_epochs(&self) -> Option<InstructionEpochs<'_>> {
        if self.epochs {
            self.memory.instruction_epochs()
        } else {
            None
        }
    }
    fn atomic_update<T>(
        &mut self,
        _a: u64,
        _len: usize,
        _f: impl FnOnce(&mut [u8]) -> (T, bool),
    ) -> Result<T, MemoryError> {
        panic!("invalid FP/SIMD transaction")
    }
    fn exclusive_read(&mut self, _a: u64, _out: &mut [u8]) -> Result<u128, MemoryError> {
        panic!("invalid FP/SIMD exclusive read")
    }
    fn exclusive_write(
        &mut self,
        _a: u64,
        _bytes: &[u8],
        _token: Option<u128>,
    ) -> Result<bool, MemoryError> {
        panic!("invalid FP/SIMD exclusive write")
    }
}

#[test]
fn all_sampled_fp_simd_errors_fault_directly_and_revalidate_without_decoder_entry() {
    if !NATIVE_COMPILED {
        return;
    }
    let newly_allocated: std::collections::BTreeSet<_> =
        include_str!("../../../../docs/validation/jit-lut-20261128/selectors.words")
            .lines()
            .collect();
    for (i, raw) in REJECTED
        .lines()
        .chain(CORRECTED.lines())
        .filter(|raw| !newly_allocated.contains(raw))
        .enumerate()
    {
        let word = u32::from_str_radix(raw, 16).unwrap();
        let mut m = Traced {
            memory: memory(&[word, 0xd4000001]),
            pages: Cell::new(0),
            fetches: Cell::new(0),
            limit: Cell::new(1),
            epochs: i % 2 != 0,
        };
        let state = initial();
        let mut c = Cpu::new(0, 0);
        c.restore_architectural_state(&state);
        let mut e = Engine::default();
        for attempt in 0..3 {
            let before = (m.fetches.get(), e.compilation_stats().blocks);
            // An absent-feature gate can fault before installing a descriptor.
            if attempt != 0 && before.1 == 0 {
                break;
            }
            // Non-chaining embeddings intentionally retain exact-budget keys.
            let budget = [1, 2, 64][if m.epochs { attempt } else { i % 3 }];
            assert!(
                matches!(c.run_slice(&mut e,&mut m,budget),SliceOutcome::Fault(trames_engine::CpuFault::Instruction{pc:0x1000,instruction}) if instruction==word)
            );
            assert_eq!(c.architectural_state(), state);
            assert_eq!(c.fault_retired(), 0);
            assert_eq!(m.pages.get(), 1);
            if attempt != 0 {
                assert_eq!((m.fetches.get(), e.compilation_stats().blocks), before);
            }
        }
        if i % 97 == 0 {
            m.limit.set(u64::MAX);
            install(&mut m.memory, &[0x91000400, 0xd4000001]);
            assert!(matches!(
                c.run_slice(&mut e, &mut m, 64),
                SliceOutcome::Syscall { instructions: 2 }
            ));
            assert_eq!(c.x(0), state.registers[0].wrapping_add(1));
            c.set_pc(0x1000);
            m.memory
                .protect(0x1000, PAGE_SIZE, Permissions::RW)
                .unwrap();
            assert!(matches!(
                c.run_slice(&mut e, &mut m, 64),
                SliceOutcome::Fault(trames_engine::CpuFault::Memory { pc: 0x1000, .. })
            ));
        }
        assert_eq!(e.stats.fallback_instructions, 0);
    }
}
