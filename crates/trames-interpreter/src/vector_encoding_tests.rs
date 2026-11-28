// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use trames_memory::{Memory, Permissions, PAGE_SIZE};

#[test]
fn reserved_vector_families_fault_without_state_changes_in_cached_and_step_paths() {
    let mut m = Memory::default();
    m.map(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
    let mut cached = Cpu::new(0x1000, 0x2000);
    let mut step = cached.clone();
    let mut count = 0;
    for raw in include_str!(
        "../../../docs/validation/jit-vector-encoding-20261128/reference-corrections.words"
    )
    .lines()
    {
        let original = u32::from_str_radix(raw, 16).unwrap();
        for (d, n, r) in [(0, 1, 2), (31, 31, 31), (18, 18, 18), (30, 29, 30)] {
            let word = if original & 0x0f000000 == 0x0f000000 {
                // Narrow shifts encode the shift amount in the middle field;
                // indexed H forms split the index and a low-register operand.
                let variable = original & (1 << 10) == 0;
                let low = (original >> 22) & 3 == 1;
                let mask = if variable {
                    if low {
                        15 << 16
                    } else {
                        31 << 16
                    }
                } else {
                    0
                };
                (original & !(0x3ff | mask))
                    | (n << 5)
                    | d
                    | if variable {
                        (r & if low { 15 } else { 31 }) << 16
                    } else {
                        0
                    }
            } else {
                (original & !0x1f03ff) | (r << 16) | (n << 5) | d
            };
            m.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
            m.write_array(0x1000, word.to_le_bytes()).unwrap();
            m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
            let mut state = Aarch64State {
                pc: 0x1000,
                sp: 0x2000,
                nzcv: ((count & 15) as u32) << 28,
                fpsr: 0x10,
                tpidr_el0: 123,
                tpidr2_el0: 456,
                ..Default::default()
            };
            state.registers = std::array::from_fn(|i| (i as u64).wrapping_mul(0x8765432112345678));
            state.vectors = std::array::from_fn(|i| (u128::MAX / 33).wrapping_mul(i as u128 + 1));
            cached.restore_architectural_state(&state);
            step.restore_architectural_state(&state);
            let a = cached.run_slice(&mut m, 1);
            let b = step.step(&mut m);
            assert!(
                matches!(a,SliceOutcome::Fault(CpuFault::Instruction{pc:0x1000,instruction})if instruction==word),
                "{word:x}/{a:?}"
            );
            assert!(
                matches!(b,Err(CpuFault::Instruction{pc:0x1000,instruction})if instruction==word),
                "{word:x}/{b:?}"
            );
            assert_eq!(cached.architectural_state(), state, "{word:x}");
            assert_eq!(step.architectural_state(), state, "{word:x}");
            // Independently LLVM-checked nearest allocated instruction.
            let neighbor = if word & 0x0f000000 == 0x0e000000 {
                word & !(1 << 29)
            } else if word & (1 << 10) != 0 {
                word | (1 << 29)
            } else {
                word & !(1 << 28)
            };
            m.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
            m.write_array(0x1000, neighbor.to_le_bytes()).unwrap();
            m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
            cached.restore_architectural_state(&state);
            step.restore_architectural_state(&state);
            assert!(
                matches!(
                    cached.run_slice(&mut m, 1),
                    SliceOutcome::Yield { instructions: 1 }
                ),
                "neighbor {neighbor:x}"
            );
            assert!(
                matches!(step.step(&mut m), Ok(Event::Continue)),
                "neighbor {neighbor:x}"
            );
            assert_eq!(
                cached.architectural_state(),
                step.architectural_state(),
                "neighbor {neighbor:x}"
            );
            count += 1;
        }
    }
    assert_eq!(count, 7104);
}
