# Complete FP/AdvSIMD encoding and error category — 2026-11-28

Baseline Runtime `d251bc26eac982314a6872dcec1ab939c001438e`, paired Android
`2b74ba3e9152d1947d78d87039e4f265592362b4`. This batch closes the declared
fixed-width FP/AdvSIMD selector spaces and their instruction-error paths. It does
not claim every possible operand/literal word or later/stateful ISA extension.

## Actual gap and shared reference correction

The entire primary groups 7/15 sweep has **524,288 selectors**, holding Rn=1 and
Rd=0 while enumerating every high/middle opcode field. Unlike the earlier selected
prefix audit, it also covers every high primary-group prefix including crypto.
The pinned existing LLVM 23 disassembler independently identifies **74,072**
valid admitted selectors, with **zero invalid admissions and zero valid missing
selectors** under the declared v9.4/FP/AdvSIMD/crypto extension mask. The retained
compressed admission stream and [standalone audit script](../../scripts/audit-jit-vectors.py)
reproduce the result without importing any Runtime decoder. Library/source/data
identities are frozen in the receipt. The opcode/format source is LLVM's
[AArch64 instruction definitions](https://raw.githubusercontent.com/llvm/llvm-project/main/llvm/lib/Target/AArch64/AArch64InstrInfo.td)
and [encoding formats](https://raw.githubusercontent.com/llvm/llvm-project/main/llvm/lib/Target/AArch64/AArch64InstrFormats.td).

The original reference interpreter successfully executed **1,776** unallocated
selectors in three families:

| Reserved form | Corrected selectors | Existing constraint corrected |
|---|---:|---|
| PMULL with reserved U=1 | 128 | Polynomial long multiply does not acquire an unsigned form. |
| Scalar SHRN/RSHRN with U=0 | 112 | Scalar plain narrowing shifts are unallocated; allocated SQSHRUN/SQRSHRUN remain valid. |
| Scalar non-saturating indexed long multiply | 1,536 | SMLAL/UMLAL/SMLSL/UMLSL/SMULL/UMULL indexed long forms are vector instructions; the allocated scalar saturating forms remain valid. |

A standalone ARM64 Linux process executes **all 1,776 actual words** through a
separate W^X validation mapping: all **1,776 produce SIGILL**, with zero normal
returns. Core dumps are disabled in that process. Its C source, exact binary
identity and output are retained. This is validation only: Runtime has no new
probe, signal preflight or per-instruction capability check.

The shared decoder corrects its existing three family constraints before any
architectural publication. Cached and stepped interpreter paths each check four
high/zero/aliased operand layouts, **7,104 reserved programs** plus **7,104 nearest
allocated neighbors**, preserving all state on faults and matching on valid
execution. Independent LLVM accepts all 1,776 original-neighbor forms and rejects
all original reserved words. The JIT never substitutes software execution for a
hardware-unallocated instruction.

## Native classification, execution and fault authority

Native admission and lowering of valid instructions are unchanged. Missing host
features remain a reached-PC instruction fault according to the existing immutable
process profile. No new extension state, feature advertisement, executable guest
rewrite, memory model, fence, lock, service thread, JNI field or hot native check
is introduced. Pure interpreter/iPad builds still exclude JIT code.

Untranslated groups 7/15 now install the original instruction-fault descriptor
after the existing native/system/memory classification. The full reference sweep,
after the three shared corrections, has **450,216 instruction faults**: 448,440
already rejected by the baseline and 1,776 corrected reserved selectors. All leave
complete architectural state unchanged and perform no data-memory callback.

The entire **74,072-selector** execution test checks complete state, retirement
and unchanged text. Actual 4-KiB ARM64 Linux executes **72,439** natively and
rejects **1,633** when their required host extensions are absent, without fallback.
This is a semantic test over the declared selector/value fixture, not every
possible data value or runtime mode. The existing wider arithmetic/crypto alias,
rounding, FPCR/FPSR, memory and reservation tests remain part of scoped acceptance.

A traced provider checks **7,035** distributed rejected selectors plus all **1,776**
corrected words, at cold/warm boundaries. An extra interpreter decoder-page query
fails the test even if no instruction retires; zero retired-fallback counters are
not used as sole evidence. Cached descriptors retain original budget keys and
identity/page invalidation, avoid warm fetch/recompile, and honor changed text and
execute permission. Absent-host gates may reject before a cache entry is installed;
those cases are distinguished in the harness. Dirty GPR/vector/NZCV/FP prefixes,
a checked memory callback, budgets 1/2/64 and serial/guarded/private modes compare
whole state, canonical bytes and exact fault retirement. The test-only slice loop
allows 128 slices to cover the 67-instruction pressure prefix at budget 1; no
production scheduling/wait policy changes.

Two separate old-source diagnostics each run one test and fail: the old shared
CPU executes a reserved PMULL selector, and the old JIT fault path enters the
interpreter decoder. Both use baseline production source plus only the same
new diagnostic files/fixtures, in a distinct Cargo target. Neither diagnostic
failure is a compilation failure or a zero-test run.

## Scoped platform acceptance

- Actual ARM64 Linux, 4-KiB host: full **37 unit + 135 integration (172)** JIT
  checks pass in Debug and Release; shared interpreter **53 unit + 3 integration
  (56)** checks pass. Strict scoped native Clippy and amd64 cross-Clippy pass.
  amd64 checks compilation, not native x86 JIT execution.
- Apple: **130** JIT fallback and **56** interpreter checks, native/default
  scoped strict lint and a fresh current-interpreter iOS build pass. Previous
  isolated iPad CPU graph evidence is reused only after verifying unchanged
  owner manifests/lock and every Runtime dependency manifest. The current iOS
  source compiles; the owner root/pin/lock remain untouched. The earlier owner-root
  lock mismatch is not relabeled as a successful `--locked` owner-root graph.
- Android core: explicit JIT **8**, default **1** and strict scoped lint pass on
  Linux/Apple. The new private probe has 30 reserved selector families and 30
  allocated neighbors, budgets 1/64 and three cold/warm entries (**360 programs**
  on this host), dirty GPR/FP prefixes, complete state/retirement and unchanged
  text. Schema 1 / 40 bytes / flag meanings remain unchanged. Feature selection
  and notice checks pass. SDK/NDK/JDK, APK and app-UID/ART/large-page acceptance
  remain Windows/device tasks.

Unrelated source-unchanged networking/graphics/memory acceptance is reused.

## Generated code and positive ISA-model acceptance

Actual compiled before/current receipts compare **1,094 FP/AdvSIMD programs**
across 261 prefix/form/capability classes, with/without full GPR/vector pressure,
plus **2,816 branch** and **838 scalar** programs. All published machine-code
streams are **byte-for-byte identical**. Operation/Block/Node/Frame remain
**192/152/264/120 bytes**. No CPU bank or decoded cache layout field is added.
Compressed streams and exact comparisons are retained.

The existing QEMU 10.0.13 max model passes the two complete encoding/execution
unit tests and both new fault-boundary integration tests. It executes **all
74,072 admitted selectors natively within the model, with zero absent-host
selectors**, including optional crypto families absent on the actual Linux host.
The model also passes the Android encoding probe. This closes positive ISA-model
acceptance of the declared selector matrix; it is not physical optional-extension
hardware, Android app-UID/ART or throughput acceptance. Exact process capability
words and model/test executable hashes are retained.

## Remaining interpreter entry

Completed scalar, fixed-width FP/AdvSIMD and owned control/exception spaces now
have direct native or instruction-fault paths. Generic unrecognized encodings
outside those audited spaces still have a one-instruction interpreter entry.
System and load/store error spaces require their own closure audit; this batch
does not bypass them or claim a zero-entry guarantee for every possible program.
SVC exits, checked memory callbacks and cross-page staging are not interpreter
execution. Full hardware PAC/BTI/MTE, SVE/SME, LS64 and SETG contracts remain
unadvertised pending real shared CPU/context/memory ownership.

## Matched ordinary-path CPU controls

Identical existing harness sources run on separate baseline/current source roots,
Cargo targets and uniquely copied binary hashes, pinned to vCPU 0 of the same
ARM64 Linux VM. Four balanced pairs alternate order; each process has **256 samples, 32 warmups
and 224 timed values**. Integer/memory controls run 64 rounds of 65,536 guest
trips per sample; SVC controls run 200,000 simulated returns. Whole state/data,
exact retirement and unchanged text are verified outside timing. Every row
requires zero fallback. All **10,240** rows, including warmups, are retained.

Ratios are geometric means of four paired median before/current time ratios;
greater than one means current is faster.

| CPU-only control | Before/current time ratio | Paired ratio range |
|---|---:|---:|
| Ordinary integer | 0.925x | 0.840–1.047x |
| Ordinary scalar memory | 1.001x | 0.993–1.016x |
| Cached SVC-only exit | 1.002x | 1.000–1.005x |
| ADD + SVC round trip | 1.284x | 0.992–2.760x |
| Branch continuation + ADD + SVC | 1.001x | 0.990–1.024x |

The earlier 32-sample/eight-warmup cohort is retained under `preliminary/`, with
all 1,280 rows and original provenance. It reported integer **0.946x** with
paired range **0.837–1.092x**, and ADD+SVC **1.260x**, driven by a **2.439x**
paired outlier. The longer cohort uses the exact same binary/harness hashes;
all new outcomes and remaining spread remain visible above. Timing variation
persists and is not attributed to a proved source mechanism. Published native
code is independently identical, but that does not establish an invariant
end-to-end timing guarantee or rule out placement/scheduling effects.

These measurements do not establish an ordinary-path speedup or a universal
regression bound. They do not measure new extension throughput, illegal-instruction
traffic, Linux syscall handlers, KVM, graphics, JNI, ART or Android lifecycle.
Source-bound artifacts and [receipt](jit-vector-encoding-20261128/receipt.json)
retain the exact scope and observed variation.

A separate repeated-process diagnostic runs each *identical* binary 12 times per
workload, 128 samples/process with 16 warmups, with paired duplicate-version
entries and unchanged vCPU affinity. All **6,144** rows and each row's exact
binary hash are retained. Integer medians for the same baseline binary span
**1.398x** while the current binary spans **1.027x**; the aggregate before/current
ratio reverses to **1.071x**. ADD+SVC spans **1.031x/1.040x** and aggregates to
**0.998x**. This establishes observable repeated-process variability and a
context-dependent sign, not its cause or a performance guarantee. The short and
long balanced cohorts above remain part of the evidence. Further profiling is
needed before attributing timing differences to a particular mechanism.
