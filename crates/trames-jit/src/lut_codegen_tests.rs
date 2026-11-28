// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::io::Write;
use trames_memory::{Memory, Permissions};
#[test]
fn lut_hardware_opcodes_keep_fields_without_helpers_and_do_not_grow_native_layouts() {
    let mut output = std::env::var_os("TRAMES_JIT_LUT_CODEGEN")
        .map(|p| std::io::BufWriter::new(std::fs::File::create(p).unwrap()));
    if let Some(out) = &mut output {
        writeln!(
            out,
            "LAYOUT {} {} {} {}",
            std::mem::size_of::<crate::memory_ops::Operation>(),
            std::mem::size_of::<Block>(),
            std::mem::size_of::<Node>(),
            std::mem::size_of::<Frame>()
        )
        .unwrap();
    }
    let mut m = Memory::default();
    m.map(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
    let mut count = 0;
    for line in include_str!("../../../docs/validation/jit-lut-20261128/forms.words").lines() {
        let base = u32::from_str_radix(line.split_whitespace().next().unwrap(), 16).unwrap();
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
            for pressure in [false, true] {
                for mode in [8, 31] {
                    let mut words = if pressure {
                        (0..31)
                            .map(|r| 0x91000400 | (r << 5) | r)
                            .chain((0..32).map(|r| 0x4ea01c00 | (r << 16) | (r << 5) | r))
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };
                    words.extend_from_slice(&[word, 0xd4000001]);
                    m.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
                    for (i, w) in words.iter().enumerate() {
                        m.write_array(0x1000 + 4 * i as u64, w.to_le_bytes())
                            .unwrap();
                    }
                    m.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
                    let compiled = compile_with_features(
                        &m,
                        0x1000,
                        64,
                        0,
                        mode,
                        trames_execution::Features {
                            hwcap: u64::MAX,
                            hwcap2: u64::MAX,
                        },
                    )
                    .unwrap()
                    .unwrap();
                    let published = compiled.block.code.published_words();
                    let found: Vec<_> = published
                        .iter()
                        .filter(|&&w| trames_interpreter::lookup::Lookup::decode(w).is_some())
                        .collect();
                    assert!(!found.is_empty(), "{word:x}");
                    assert!(
                        found.iter().all(|&&w| w & !0x1f03ff == word & !0x1f03ff),
                        "{word:x}"
                    );
                    assert!(
                        !published
                            .iter()
                            .any(|&w| w & 0xfc000000 == 0x94000000 || w & 0xfffffc1f == 0xd63f0000),
                        "LUT has no native helper call"
                    );
                    if let Some(out) = &mut output {
                        write!(out, "{word:08x} {pressure} {mode}").unwrap();
                        for w in published {
                            write!(out, " {w:08x}").unwrap();
                        }
                        writeln!(out).unwrap();
                    }
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 576);
    eprintln!("LUT opcode/alias/pressure/mode receipt: {count} compiled programs");
}
