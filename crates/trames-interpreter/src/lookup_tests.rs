// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use trames_memory::{Memory, Permissions, PAGE_SIZE};
#[path = "lookup_test_support.rs"]
mod oracle;
#[test]
fn entire_fixed_width_lut_register_and_segment_encoding_space_matches_independent_oracle() {
    let mut m = Memory::default();
    m.map(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
    let mut cached = Cpu::new(0, 0);
    let mut step = cached.clone();
    let mut state = oracle::state();
    let mut count = 0;
    for (base, index, bits, segment) in oracle::forms() {
        for n in 0..32 {
            for reg in 0..32 {
                for d in 0..32 {
                    let word = (base & !0x1f03ff) | (reg << 16) | (n << 5) | d;
                    m.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
                    m.write_array(0x1000, word.to_le_bytes()).unwrap();
                    m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
                    state.nzcv = ((count & 15) as u32) << 28;
                    let wanted = oracle::expected(
                        &state,
                        index,
                        bits,
                        segment,
                        d as usize,
                        n as usize,
                        reg as usize,
                    );
                    cached.restore_architectural_state(&state);
                    step.restore_architectural_state(&state);
                    assert!(
                        matches!(
                            cached.run_slice(&mut m, 1),
                            SliceOutcome::Yield { instructions: 1 }
                        ),
                        "{word:x}"
                    );
                    assert!(matches!(step.step(&mut m), Ok(Event::Continue)), "{word:x}");
                    assert_eq!(cached.architectural_state(), wanted, "cached {word:x}");
                    assert_eq!(step.architectural_state(), wanted, "step {word:x}");
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 589824);
    eprintln!("LUT full legal encoding space: {count} programs in each reference path");
}

#[test]
fn all_primary_fp_simd_selectors_match_independent_lut_admission() {
    let valid: std::collections::BTreeSet<_> =
        include_str!("../../../docs/validation/jit-lut-20261128/selectors.words")
            .lines()
            .map(|raw| u32::from_str_radix(raw, 16).unwrap())
            .collect();
    let mut accepted = 0;
    for high in 0..512u32 {
        if !matches!((high >> 2) & 15, 7 | 15) {
            continue;
        }
        for middle in 0..8192 {
            let word = (high << 23) | (middle << 10) | (1 << 5);
            let op = lookup::Lookup::decode(word);
            assert_eq!(op.is_some(), valid.contains(&word), "{word:x}");
            accepted += usize::from(op.is_some());
        }
    }
    assert_eq!(accepted, 576);
}
