// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
#[path = "../../../trames-interpreter/src/lookup_test_support.rs"]
mod oracle;
fn install(m: &mut Memory, word: u32) {
    m.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
    m.write_array(0x1000, word.to_le_bytes()).unwrap();
    m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
}
fn available() -> bool {
    let host = <Cpu as Backend>::features();
    host.hwcap & 2 != 0 && host.hwcap2 & (1 << 49) != 0
}
#[test]
fn full_lut_encoding_space_runs_real_host_instructions_or_exact_host_rejections() {
    if !NATIVE_COMPILED {
        return;
    }
    let mut m = memory(&[]);
    let mut c = Cpu::new(0, 0);
    let mut e = Engine::default();
    let mut state = oracle::state();
    let mut count = 0;
    let positive = available();
    for (base, index, bits, segment) in oracle::forms() {
        for n in 0..32 {
            for reg in 0..32 {
                for d in 0..32 {
                    let word = (base & !0x1f03ff) | (reg << 16) | (n << 5) | d;
                    install(&mut m, word);
                    state.nzcv = ((count & 15) as u32) << 28;
                    c.restore_architectural_state(&state);
                    let outcome = c.run_slice(&mut e, &mut m, 1);
                    if positive {
                        assert!(
                            matches!(outcome, SliceOutcome::Yield { instructions: 1 }),
                            "{word:x}/{outcome:?}"
                        );
                        assert_eq!(
                            c.architectural_state(),
                            oracle::expected(
                                &state,
                                index,
                                bits,
                                segment,
                                d as usize,
                                n as usize,
                                reg as usize
                            ),
                            "{word:x}"
                        );
                    } else {
                        assert!(
                            matches!(outcome,SliceOutcome::Fault(trames_engine::CpuFault::Instruction{pc:0x1000,instruction})if instruction==word),
                            "{word:x}/{outcome:?}"
                        );
                        assert_eq!(c.architectural_state(), state);
                        assert_eq!(c.fault_retired(), 0);
                    }
                    assert_eq!(m.fetch_u32(0x1000).unwrap(), word);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 589824);
    assert_eq!(e.stats.fallback_instructions, 0);
    eprintln!("LUT entire legal encoding space: {count} programs; real host admitted={positive}");
}
#[test]
fn all_lut_packed_index_values_and_table_destination_aliases_match_independent_bytes() {
    if !NATIVE_COMPILED || !available() {
        return;
    }
    let mut m = memory(&[]);
    let mut c = Cpu::new(0, 0);
    let mut e = Engine::default();
    let mut count = 0;
    for (base, index, bits, segment) in oracle::forms() {
        for (d, n, reg) in [
            (0, 1, 2),
            (31, 31, 0),
            (0, 31, 0),
            (31, 30, 31),
            (18, 18, 18),
            (30, 29, 30),
            (2, 1, 2),
            (1, 1, 2),
        ] {
            let word = (base & !0x1f03ff) | (reg << 16) | (n << 5) | d;
            install(&mut m, word);
            for pattern in 0..256 {
                let mut state = oracle::state();
                state.vectors[reg as usize] = u128::from_le_bytes([pattern as u8; 16]);
                state.fpcr = (pattern & 3) << 22;
                state.nzcv = (pattern & 15) << 28;
                c.restore_architectural_state(&state);
                assert!(matches!(
                    c.run_slice(&mut e, &mut m, 1),
                    SliceOutcome::Yield { instructions: 1 }
                ));
                assert_eq!(
                    c.architectural_state(),
                    oracle::expected(
                        &state,
                        index,
                        bits,
                        segment,
                        d as usize,
                        n as usize,
                        reg as usize
                    ),
                    "{word:x}/{pattern}"
                );
                count += 1;
            }
        }
    }
    assert_eq!(count, 36864);
    assert_eq!(e.stats.fallback_instructions, 0);
}
#[test]
fn lut_tables_repack_dirty_banks_and_preserve_callbacks_budgets_and_fault_prefixes() {
    if !NATIVE_COMPILED || !available() {
        return;
    }
    for (base, _, _, _) in oracle::forms() {
        for (d, n, reg) in [
            (0, 1, 2),
            (31, 31, 0),
            (0, 31, 0),
            (31, 30, 31),
            (18, 18, 18),
            (30, 29, 30),
        ] {
            let word = (base & !0x1f03ff) | (reg << 16) | (n << 5) | d;
            let mut prefix: Vec<_> = (0..31).map(|r| 0x91000400 | (r << 5) | r).collect();
            prefix.extend((0..32).map(|r| 0x4ea01c00 | (r << 16) | (r << 5) | r));
            for fault in [false, true] {
                let mut words = prefix.clone();
                if fault {
                    words.extend_from_slice(&[word, 0xf9400274]);
                } else {
                    words.extend_from_slice(&[0xf9400274, word, word, 0xd4000001]);
                }
                let mut state = oracle::state();
                state.registers[19] = if fault {
                    0x10000 + hardware_span() - 1
                } else {
                    0x10000 - 1
                };
                for budget in [1, 3, 64] {
                    for mode in [0, 1, 2] {
                        let e = compare_memory_sequence_mode(&words, &state, budget, mode);
                        assert_eq!(e.stats.fallback_instructions, 0, "{word:x}/{budget}/{mode}");
                    }
                }
            }
        }
    }
}
#[test]
fn lut_missing_host_faults_retire_only_dirty_prefix_and_preserve_original_reservations() {
    if !NATIVE_COMPILED {
        return;
    }
    for (base, _, _, _) in oracle::forms() {
        let word = (base & !0x1f03ff) | (2 << 16) | (1 << 5);
        if available() {
            let words = [0xc85f7e60, word, 0xc8037e64, 0xd4000001]; // LDXR/STXR through original reservation domain
            let mut state = oracle::state();
            state.registers[19] = 0x10000;
            for budget in [1, 64] {
                for mode in [0, 1, 2] {
                    let e = compare_memory_sequence_mode(&words, &state, budget, mode);
                    assert_eq!(e.stats.fallback_instructions, 0);
                }
            }
        } else {
            for budget in [1, 2, 64] {
                let words = [0xb1000529, 0x1e622820, word];
                let mut a = memory(&words);
                let mut b = memory(&words);
                let state = oracle::state();
                let mut c = Cpu::new(0, 0);
                let mut r = trames_interpreter::Cpu::new(0, 0);
                c.restore_architectural_state(&state);
                r.restore_architectural_state(&state);
                let mut e = Engine::default();
                assert!(matches!(
                    r.run_slice(&mut b, 2),
                    SliceOutcome::Yield { instructions: 2 }
                ));
                let mut retired = 0;
                loop {
                    let outcome = c.run_slice(&mut e, &mut a, budget);
                    if let SliceOutcome::Yield { instructions } = outcome {
                        retired += instructions;
                        continue;
                    }
                    assert!(
                        matches!(outcome,SliceOutcome::Fault(trames_engine::CpuFault::Instruction{pc:0x1008,instruction})if instruction==word)
                    );
                    assert_eq!(retired + c.fault_retired(), 2);
                    break;
                }
                assert_eq!(c.architectural_state(), r.architectural_state());
                assert_eq!(e.stats.fallback_instructions, 0);
            }
        }
    }
}

#[test]
fn lut_raw_host_capability_is_passed_through_to_native_cpu() {
    if !NATIVE_COMPILED {
        return;
    }
    let raw = trames_jit::host_features().unwrap();
    let guest = <Cpu as Backend>::features();
    assert_eq!(
        guest.hwcap2 & (1 << 49) != 0,
        raw.hwcap2 & (1 << 49) != 0 && raw.hwcap & 2 != 0
    );
}

#[test]
fn lut_syscall_resumption_and_guest_authorized_code_changes_keep_original_authority() {
    if !NATIVE_COMPILED || !available() {
        return;
    }
    for (base, _, _, _) in oracle::forms() {
        let word = (base & !0x1f03ff) | (2 << 16) | (31 << 5);
        let words = [word, 0xd4000001, 0x17fffffe];
        let mut a = memory(&words);
        let mut b = memory(&words);
        let mut e = Engine::default();
        let mut c = Cpu::new(0, 0);
        let mut r = trames_interpreter::Cpu::new(0, 0);
        let state = oracle::state();
        c.restore_architectural_state(&state);
        r.restore_architectural_state(&state);
        for _ in 0..4 {
            let x = c.run_slice(&mut e, &mut a, 64);
            let y = r.run_slice(&mut b, 64);
            assert_eq!(format!("{x:?}"), format!("{y:?}"));
            assert_eq!(c.architectural_state(), r.architectural_state());
        }
        assert!(e.link_stats().syscall_resumes > 0);
        // Only the guest-authorized write changes executable bytes.
        let replacement = word ^ 0x4000;
        for m in [&mut a, &mut b] {
            m.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
            m.write_array(0x1000, replacement.to_le_bytes()).unwrap();
            m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
        }
        c.restore_architectural_state(&state);
        r.restore_architectural_state(&state);
        assert!(matches!(
            c.run_slice(&mut e, &mut a, 1),
            SliceOutcome::Yield { instructions: 1 }
        ));
        assert!(matches!(
            r.run_slice(&mut b, 1),
            SliceOutcome::Yield { instructions: 1 }
        ));
        assert_eq!(c.architectural_state(), r.architectural_state());
        c.set_pc(0x1000);
        a.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
        assert!(matches!(
            c.run_slice(&mut e, &mut a, 64),
            SliceOutcome::Fault(trames_engine::CpuFault::Memory { pc: 0x1000, .. })
        ));
        assert_eq!(e.stats.fallback_instructions, 0);
    }
}
