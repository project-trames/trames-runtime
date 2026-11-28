// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::io::Write;
use trames_engine::CpuFault;
use trames_memory::{CpuMemory, MemoryError};
struct Word(u32);
impl CpuMemory for Word {
    fn mapped_at_or_after(&self, _: u64) -> bool {
        false
    }
    fn read(&self, _: u64, _: &mut [u8]) -> Result<(), MemoryError> {
        panic!("FP selector data read")
    }
    fn write(&mut self, _: u64, _: &[u8]) -> Result<(), MemoryError> {
        panic!("FP selector data write")
    }
    fn check(&self, _: u64, _: usize, _: trames_memory::Access) -> Result<(), MemoryError> {
        panic!("FP selector data check")
    }
    fn check_translation(&self, _: u64) -> Result<(), MemoryError> {
        panic!("FP selector translation")
    }
    fn fetch_u32(&self, _: u64) -> Result<u32, MemoryError> {
        Ok(self.0)
    }
    fn instruction_identity(&self) -> (u64, u64, u64) {
        (0, 0, self.0 as u64)
    }
    fn instruction_page_identity(&self, _: u64) -> Result<(u64, u64, u64), MemoryError> {
        Ok(self.instruction_identity())
    }
    fn atomic_update<T>(
        &mut self,
        _: u64,
        _: usize,
        _: impl FnOnce(&mut [u8]) -> (T, bool),
    ) -> Result<T, MemoryError> {
        panic!("FP selector transaction")
    }
    fn exclusive_read(&mut self, _: u64, _: &mut [u8]) -> Result<u128, MemoryError> {
        panic!("FP selector reservation")
    }
    fn exclusive_write(&mut self, _: u64, _: &[u8], _: Option<u128>) -> Result<bool, MemoryError> {
        panic!("FP selector reservation")
    }
}
#[test]
fn entire_fp_simd_selector_space_has_native_or_original_fault_classification() {
    let mut admitted = std::env::var_os("TRAMES_JIT_VECTOR_ADMITTED")
        .map(|p| std::io::BufWriter::new(std::fs::File::create(p).unwrap()));
    let mut rejected = std::env::var_os("TRAMES_JIT_VECTOR_REJECTED")
        .map(|p| std::io::BufWriter::new(std::fs::File::create(p).unwrap()));
    let mut missing_output = std::env::var_os("TRAMES_JIT_VECTOR_MISSING")
        .map(|p| std::io::BufWriter::new(std::fs::File::create(p).unwrap()));
    let mut cpu = trames_interpreter::Cpu::new(0x1000, 0x2000);
    let mut state = cpu.architectural_state();
    state.nzcv = 0xb0000000;
    state.registers = std::array::from_fn(|r| (r as u64).wrapping_mul(0x123456789abcdef));
    state.vectors = std::array::from_fn(|r| (u128::MAX / 33).wrapping_mul(r as u128 + 1));
    let mut counts = [0u64; 3];
    let mut missing = Vec::new();
    for high in 0..512u32 {
        if !owns(high << 23) {
            continue;
        }
        for middle in 0..8192 {
            let word = (high << 23) | (middle << 10) | (1 << 5);
            if let Some(op) = decode(word) {
                counts[0] += 1;
                if let Some(out) = &mut admitted {
                    writeln!(
                        out,
                        "{word:08x} {:x} {:x} {:?}",
                        op.required.hwcap, op.required.hwcap2, op.form
                    )
                    .unwrap();
                }
                continue;
            }
            cpu.restore_architectural_state(&state);
            let result = cpu.step(&mut Word(word));
            if !matches!(result,Err(CpuFault::Instruction{pc:0x1000,instruction}) if instruction==word)
            {
                counts[2] += 1;
                if let Some(out) = &mut missing_output {
                    writeln!(out, "{word:08x}").unwrap();
                }
                if missing.len() < 128 {
                    missing.push((word, format!("{result:?}")));
                }
            } else {
                assert_eq!(
                    cpu.architectural_state(),
                    state,
                    "fault changed state: {word:x}"
                );
                if counts[1] % 64 == 0 {
                    if let Some(out) = &mut rejected {
                        writeln!(out, "{word:08x}").unwrap();
                    }
                }
                counts[1] += 1;
            }
        }
    }
    eprintln!(
        "All FP/SIMD selectors: admitted {}, original faults {}, missing {}",
        counts[0], counts[1], counts[2]
    );
    assert!(
        missing.is_empty(),
        "Untranslated reference selectors: {missing:x?}"
    );
    assert_eq!(counts.iter().sum::<u64>(), 524288);
}

#[test]
fn every_admitted_fp_simd_selector_executes_natively_or_rejects_absent_host_features() {
    use trames_engine::{Cpu as _, SliceOutcome};
    use trames_memory::{Memory, Permissions, PAGE_SIZE};
    let mut m = Memory::default();
    m.map(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
    let mut engine = crate::Engine::default();
    let mut cpu = crate::Cpu::new(0x1000, 0x2000);
    let mut reference = trames_interpreter::Cpu::new(0x1000, 0x2000);
    let mut state = reference.architectural_state();
    state.nzcv = 0xb0000000;
    state.fpsr = 0x10;
    state.tpidr_el0 = 123;
    state.tpidr2_el0 = 456;
    state.registers = std::array::from_fn(|r| (r as u64).wrapping_mul(0x123456789abcdef));
    state.vectors = std::array::from_fn(|r| (u128::MAX / 33).wrapping_mul(r as u128 + 1));
    let host = <crate::Cpu as trames_engine::Cpu>::features();
    let mut counts = [0u64; 2];
    for high in 0..512u32 {
        if !owns(high << 23) {
            continue;
        }
        for middle in 0..8192 {
            let word = (high << 23) | (middle << 10) | (1 << 5);
            let Some(op) = decode(word) else {
                continue;
            };
            m.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
            m.write_array(0x1000, word.to_le_bytes()).unwrap();
            m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
            cpu.restore_architectural_state(&state);
            reference.restore_architectural_state(&state);
            let result = cpu.run_slice(&mut engine, &mut m, 1);
            if host.hwcap & op.required.hwcap == op.required.hwcap
                && host.hwcap2 & op.required.hwcap2 == op.required.hwcap2
            {
                let expected = reference.run_slice(&mut m, 1);
                assert!(
                    matches!(result, SliceOutcome::Yield { instructions: 1 }),
                    "{word:x}/{result:?}"
                );
                assert!(
                    matches!(expected, SliceOutcome::Yield { instructions: 1 }),
                    "{word:x}/{expected:?}"
                );
                assert_eq!(
                    cpu.architectural_state(),
                    reference.architectural_state(),
                    "{word:x}/{:?}",
                    op.form
                );
                counts[0] += 1;
            } else {
                assert!(
                    matches!(result,SliceOutcome::Fault(CpuFault::Instruction{pc:0x1000,instruction})if instruction==word),
                    "{word:x}/{result:?}"
                );
                assert_eq!(cpu.architectural_state(), state);
                assert_eq!(cpu.fault_retired(), 0);
                counts[1] += 1;
            }
            assert_eq!(m.fetch_u32(0x1000).unwrap(), word);
        }
    }
    assert_eq!(engine.stats.fallback_instructions, 0);
    assert_eq!(counts.iter().sum::<u64>(), 74072);
    eprintln!(
        "Complete FP/SIMD execution selectors: {} native, {} absent host",
        counts[0], counts[1]
    );
}
