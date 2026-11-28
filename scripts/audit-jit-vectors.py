#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent complete FP/AdvSIMD selector oracle; requires existing Linux LLVM.

Capture admission with TRAMES_JIT_VECTOR_ADMITTED=<file> cargo test --locked
-p trames-jit --features native-aarch64 entire_fp_simd_selector. This script
does not import the Runtime decoder and is never an execution dependency.
"""
import argparse
import collections
import ctypes
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("admitted", type=Path)
parser.add_argument("--library", type=Path)
parser.add_argument("--output", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
if args.library is None:
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    args.library = next((sysroot / "lib").glob("libLLVM.so.23*"))
llvm = ctypes.CDLL(str(args.library))
for name in ("LLVMInitializeAArch64TargetInfo", "LLVMInitializeAArch64TargetMC", "LLVMInitializeAArch64Disassembler"):
    getattr(llvm, name)()
llvm.LLVMCreateDisasmCPUFeatures.restype = ctypes.c_void_p
llvm.LLVMCreateDisasmCPUFeatures.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p]
llvm.LLVMDisasmInstruction.restype = ctypes.c_size_t
llvm.LLVMDisasmInstruction.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint64, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_size_t]
llvm.LLVMDisasmDispose.argtypes = [ctypes.c_void_p]
features = "+v9.4a,+neon,+fp-armv8,+fullfp16,+fp16fml,+bf16,+i8mm,+complxnum,+rdm,+dotprod,+jsconv,+fptoint,+crypto,+sha3,+sm4"
context = llvm.LLVMCreateDisasmCPUFeatures(b"aarch64-linux-gnu", b"generic", features.encode(), None, 0, None, None)
assert context

def decode(word):
    data = ctypes.create_string_buffer(word.to_bytes(4, "little"))
    output = ctypes.create_string_buffer(256)
    size = llvm.LLVMDisasmInstruction(context, data, 4, 0, output, len(output))
    return output.value.decode().strip() if size == 4 else None

data = args.admitted.read_bytes()
if args.admitted.suffix == ".gz":
    data = gzip.decompress(data)
admitted = {int(line.split()[0], 16): line for line in data.decode().splitlines()}
invalid = []
outside = []
counts = collections.Counter()
selectors = 0
for high in range(512):
    if (high >> 2) & 15 not in (7, 15):
        continue
    for middle in range(8192):
        word = (high << 23) | (middle << 10) | (1 << 5)
        selectors += 1
        assembly = decode(word)
        if assembly:
            counts[assembly.split()[0]] += 1
        if word in admitted and not assembly:
            invalid.append(admitted[word])
        elif word not in admitted and assembly:
            outside.append(f"{word:08x} {assembly}")

corrections = root / "docs/validation/jit-vector-encoding-20261128/reference-corrections.words"
neighbors = collections.Counter()
for raw in corrections.read_text().splitlines():
    word = int(raw, 16)
    assert decode(word) is None, raw
    if word & 0x0f000000 == 0x0e000000:
        neighbor = word & ~(1 << 29)
    elif word & (1 << 10):
        neighbor = word | (1 << 29)
    else:
        neighbor = word & ~(1 << 28)
    assembly = decode(neighbor)
    assert assembly is not None, f"{neighbor:08x}"
    neighbors[assembly.split()[0]] += 1
llvm.LLVMDisasmDispose(context)
result = {
    "features": features,
    "library_sha256": hashlib.sha256(args.library.read_bytes()).hexdigest(),
    "admission_sha256": hashlib.sha256(data).hexdigest(),
    "selectors": selectors,
    "admitted": len(admitted),
    "invalid_admissions": invalid,
    "llvm_valid_unadmitted": outside,
    "corrections": sum(neighbors.values()),
    "allocated_neighbors": dict(sorted(neighbors.items())),
    "instructions": dict(sorted(counts.items())),
}
assert selectors == 524288
assert len(admitted) == sum(counts.values()) == 74072
assert not invalid and not outside
assert sum(neighbors.values()) == 1776
encoded = json.dumps(result, indent=2) + "\n"
if args.output:
    args.output.write_text(encoded)
else:
    print(encoded, end="")
