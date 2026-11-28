// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{collections::BTreeSet, io::Write};
use trames_memory::{Memory, Permissions};
#[test]
fn vector_generated_code_receipt() {
    let Some(path) = std::env::var_os("TRAMES_JIT_VECTOR_CODEGEN") else {
        return;
    };
    let mut output = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    writeln!(
        output,
        "LAYOUT {} {} {} {}",
        std::mem::size_of::<crate::memory_ops::Operation>(),
        std::mem::size_of::<Block>(),
        std::mem::size_of::<Node>(),
        std::mem::size_of::<Frame>()
    )
    .unwrap();
    let mut seen = BTreeSet::new();
    let mut count = 0;
    let mut index = 0;
    let mut memory = Memory::default();
    memory.map(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
    for high in 0..512u32 {
        if !matches!((high >> 2) & 15, 7 | 15) {
            continue;
        }
        for middle in 0..8192 {
            let word = (high << 23) | (middle << 10) | (1 << 5);
            let Some(op) = crate::vector_ops::decode(word) else {
                continue;
            };
            // Preserve the original before/after control cohort; LUT has its own receipt.
            if op.required.hwcap2 & (1 << 49) != 0 {
                continue;
            }
            let first = seen.insert(format!(
                "{high:x}/{:?}/{:x}/{:x}",
                op.form, op.required.hwcap, op.required.hwcap2
            ));
            index += 1;
            if index % 257 != 0 && !first {
                continue;
            }
            for pressure in [false, true] {
                let mut words = if pressure {
                    (0..31)
                        .map(|r| 0x91000400 | (r << 5) | r)
                        .chain((0..32).map(|r| 0x4ea01c00 | (r << 16) | (r << 5) | r))
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                words.extend_from_slice(&[word, 0xd4000001]);
                memory.protect(0x1000, PAGE_SIZE, Permissions::RW).unwrap();
                for (i, w) in words.iter().enumerate() {
                    memory
                        .write_array(0x1000 + 4 * i as u64, w.to_le_bytes())
                        .unwrap();
                }
                memory.protect(0x1000, PAGE_SIZE, Permissions::RX).unwrap();
                let compiled = compile_with_features(
                    &memory,
                    0x1000,
                    64,
                    0,
                    8,
                    trames_execution::Features {
                        hwcap: u64::MAX,
                        hwcap2: u64::MAX,
                    },
                )
                .unwrap()
                .unwrap();
                write!(output, "{word:08x} {pressure}").unwrap();
                for w in compiled.block.code.published_words() {
                    write!(output, " {w:08x}").unwrap();
                }
                writeln!(output).unwrap();
                count += 1;
            }
        }
    }
    eprintln!(
        "Vector native-code receipt: {count} programs / {} prefix-form-capability classes",
        seen.len()
    );
}
