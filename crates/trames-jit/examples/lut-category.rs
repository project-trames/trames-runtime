// SPDX-License-Identifier: GPL-3.0-or-later
//! CPU-only lookup loops; validation and canonical state checks stay outside timing.
use std::{hint::black_box, time::Instant};
use trames_engine::{Cpu as Backend, SliceOutcome};
use trames_execution::Aarch64State;
use trames_memory::{Memory, Permissions, PAGE_SIZE};
const TRIPS: u64 = 65536;
fn run<C: Backend>(cpu: &mut C, engine: &mut C::Engine, m: &mut Memory) -> u64 {
    let mut retired = 0;
    loop {
        match cpu.run(engine, m, 1_048_576) {
            SliceOutcome::Yield { instructions } => retired += instructions,
            SliceOutcome::Syscall { instructions } => return retired + instructions,
            other => panic!("{other:?}"),
        }
    }
}
fn measure<C: Backend>(name: &str, backend: &str, word: u32, stats: fn(&C::Engine) -> (u64, u64)) {
    let words = [word, 0xf1000463, 0x54ffffc1, 0xd4000001]; // LUT; SUBS x3,x3,#1; B.NE -8; SVC
    let mut m = Memory::default();
    m.map(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
    for (i, w) in words.iter().enumerate() {
        m.write_array(0x1000 + 4 * i as u64, w.to_le_bytes())
            .unwrap();
    }
    m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
    let mut state = Aarch64State {
        pc: 0x1000,
        sp: 0x2000,
        nzcv: 0xb0000000,
        fpsr: 0x08000010,
        tpidr_el0: 123,
        tpidr2_el0: 456,
        ..Default::default()
    };
    state.registers = std::array::from_fn(|r| (r as u64).wrapping_mul(0x123456789));
    state.registers[3] = TRIPS;
    state.vectors =
        std::array::from_fn(|r| (r as u128 + 1).wrapping_mul(0x123456789abcdef_fedcba9876543211));
    let mut cpu = C::default();
    let mut engine = C::Engine::default();
    let mut reference = trames_interpreter::Cpu::default();
    let mut reference_engine = trames_interpreter::Engine::default();
    reference.restore_architectural_state(&state);
    assert_eq!(
        run(&mut reference, &mut reference_engine, &mut m),
        TRIPS * 3 + 1
    );
    let wanted = reference.architectural_state();
    cpu.restore_architectural_state(&state);
    assert_eq!(run(&mut cpu, &mut engine, &mut m), TRIPS * 3 + 1);
    assert_eq!(cpu.architectural_state(), wanted);
    assert_eq!(wanted.registers[3], 0);
    assert_eq!(wanted.nzcv, 0x60000000);
    let samples = std::env::var("TRAMES_BENCH_SAMPLES").map_or(16, |n| n.parse().unwrap());
    let warmup = std::env::var("TRAMES_BENCH_WARMUP").map_or(4, |n| n.parse().unwrap());
    for sample in 0..samples {
        let before = stats(&engine);
        let mut retired = 0;
        let mut ns = 0;
        for _ in 0..8 {
            cpu.restore_architectural_state(&state);
            let start = Instant::now();
            retired += black_box(run(&mut cpu, &mut engine, &mut m));
            ns += start.elapsed().as_nanos();
            assert_eq!(cpu.architectural_state(), wanted);
        }
        assert_eq!(retired, 8 * (TRIPS * 3 + 1));
        let after = stats(&engine);
        assert_eq!(after.0 - before.0, 0);
        for (i, w) in words.iter().enumerate() {
            assert_eq!(m.fetch_u32(0x1000 + 4 * i as u64).unwrap(), *w);
        }
        println!("{{\"workload\":\"{name}\",\"backend\":\"{backend}\",\"sample\":{sample},\"warmup\":{},\"ns\":{ns},\"rounds\":8,\"retired\":{retired},\"fallback\":0,\"native\":{}}}",sample<warmup,after.1-before.1);
    }
}
fn main() {
    let host = trames_jit::host_features().expect("requires native ARM64 Linux/Android");
    assert!(trames_jit::NATIVE_COMPILED && host.hwcap & 2 != 0 && host.hwcap2 & (1 << 49) != 0);
    let filter = std::env::var("TRAMES_BENCH_FILTER").ok();
    let backend = std::env::var("TRAMES_BENCH_BACKEND").unwrap_or_else(|_| "jit".into());
    for (name, base) in [
        ("luti2-byte", 0x4e801000u32),
        ("luti2-half", 0x4ec00000),
        ("luti4-byte", 0x4e402000),
        ("luti4-half", 0x4e401000),
    ] {
        if filter.as_ref().is_some_and(|s| s != name) {
            continue;
        }
        let word = base | (3 << 16) | (1 << 5);
        if backend == "jit" {
            measure::<trames_jit::Cpu>(name, "jit", word, |e| {
                (e.stats.fallback_instructions, e.stats.native_instructions)
            });
        } else {
            assert_eq!(backend, "interpreter");
            measure::<trames_interpreter::Cpu>(name, "interpreter", word, |_| (0, 0));
        }
    }
}
