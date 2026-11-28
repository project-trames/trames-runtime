# Native compression and crypto computation

Protocol 1/module 11 (`0x1000b`, auxv `0x5452000c`) extends the shared NativeCall
registry. It follows the existing `pure_accelerator` selection. The iPad
interpreter can use it; Mac HVF and Linux KVM retain their existing disabled
computation-thunk default and execute original library instructions on hardware.
There is no product UI or additional user setting.

At session creation, the runtime considers private-root AArch64 shared libraries
`libz.so.1`, `liblzma.so.5`, `libzstd.so.1` and `libcrypto.so.3` in the usual
`/lib`, `/usr/lib` and multiarch directories. Missing, read-only, busy, unsupported
or concurrently changed files retain original execution. Host mounts and other
mounted volumes are not rewritten. This is cold selection: package upgrades
inside a running session are reconsidered at the next session start.

Original SHA256 identities are pinned in `compute-catalog.tsv`. Unregistered,
modified or upgraded package images retain their complete original behavior;
matching an export name alone never opts a custom implementation into a thunk.

## Default adapters

- zlib: `crc32`, `adler32`, `compress2`, `uncompress`.
- liblzma: `lzma_crc32`, `lzma_crc64`, `lzma_easy_buffer_encode`,
  `lzma_stream_buffer_decode` with the default allocator.
- zstd: `ZSTD_compress`, `ZSTD_decompress`.
- libcrypto: SHA-1/256/512 `Init`, `Update`, `Final`, `Transform`, plus AArch64
  `AES_set_encrypt_key`, `AES_set_decrypt_key`, `AES_encrypt`, `AES_decrypt` and
  full-block `AES_cbc_encrypt`.

Static binaries, custom allocator initialization, advanced unsupported codec
controls and EVP/TLS contexts retain original code. Public SHA contexts stay in guest memory; their
inline integer state and bounds are marshalled per call. The high-level SHA/EVP
provider-selection path remains guest-owned. Pure one-shot SHA operations are
also available in the protocol, but the ELF adapter does not replace the guest's
provider-selecting one-shot exports. This is not a claim that apt/apk/git/curl or
HTTPS are completely native, or that every installation becomes faster.

## ELF and fallback behavior

The cold adapter checks ELF64/AArch64 shared-image headers, dynamic symbols,
bounded program/section tables and relocatable entry instructions. Unsupported
entries remain original. It retains the original load segments, algorithms,
symbols, versions, TLS and dynamic loader. New RX wrappers and RW dynamic tables
are separate; the loader resolves `getauxval` and `__errno_location` through normal
guest relocations. Segment alignment respects the original maximum load alignment.

The library initializer selects a scalar guest capability once, preserves errno
and constructor arguments, then invokes the original initializer array in order.
Legacy `DT_INIT` stays intact; the new capability initializer uses `INIT_ARRAY`,
which both AArch64 musl and glibc execute. Calls made
before initialization retain original execution without requiring libc TLS.
Wrappers preserve BTI/PAC landing instructions, authenticate successful PAC
returns, retain sign-extended relative branch destinations and restore original
arguments before fallback. A missing/disabled capability selects original code.
The original bytes can be restored only after validating the embedded exact-original
SHA256 recovery recipe. Guest templates are assembled into the cold ELF derivative; no host JIT or
host executable-memory allocation is involved.

The call frame contains nine integer/pointer arguments; the ninth is read from
the caller stack only for the nine-argument liblzma decoder. Native authorization
uses the same five-word executable-page identity check as other pure calls.
Worker and source execution use the same `NativeMemory` transport and metrics.

## Memory and concurrency

Guest addresses are never host pointers. Checked reads gather call-owned bytes;
only call-owned host buffers and validated public SHA/AES integer contexts cross
the static FFI boundary. Output preflight uses the existing CPU permissions,
backing and resource admission. Ineligible ranges select original execution
before guest writes. Output scratch reserves capacity without zero-filling unused
bytes; only ranges the library reports initialized are published.

SHA `num`/digest-width and AES round counts are checked before native indexing.
SHA512 initialization preserves its originally untouched pending block. Host
hash operations use low-level arithmetic routines, not a host EVP provider or TLS
session. AES schedule marshalling is enabled on AArch64 hosts; other host
architectures retain the original AES implementation.

State remains guest-owned, including across fork. The optional stream cache
described below never makes a host pointer or authoritative host handle part of
a guest object. No host TLS pointer, global algorithm lock, new coordinator or
polling bus is introduced. Reusing a non-thread-safe context
concurrently remains the guest caller's responsibility. Each native invocation
runs on its executing worker and uses private staging, so guest races cannot
turn into races over an opaque host C context.

Static dependencies are pinned by Cargo.lock and built from bundled zlib, xz,
zstd and OpenSSL sources for Apple and Linux. Product dependency catalogs include
their original notices. This module does not add networking or host filesystem
authority.


## Streaming codecs

Protocol 1/module 12 (`0x1000c`, auxv `0x5452000d`) adds one pre/post NativeCall
entry. Its frame contains eight original arguments and an operation/ABI selector.
A pre-call decline restores the arguments and runs the original guest body;
a post-call observes its return and may learn a cache entry. Old/disabled hosts
execute the original body directly. The reversible `TRMCMP02` ELF derivative
upgrades a verified `TRMCMP01` derivative through its exact original image.
Nested fallback calls sign their own PAC link before entering an authenticated
original function body.

The default profile covers `deflateInit2_`, `inflateInit2_`, `deflate`, `inflate`,
reset/end/dictionary controls; xz `lzma_easy_encoder`, stream/auto/alone decoder,
`lzma_code` and end; Zstd create/init/reset, ordinary level/window/checksum
parameters, compress/decompress stream and flush/end/free. Custom allocator
initialization and unreviewed advanced settings continue in the guest. Allocating,
freeing or allocation-failure calls always execute the original guest call, even
for the default allocator; temporary allocator events count too. Unhooked controls
which change opaque state cause a cache mismatch and original continuation.

A worker pool owns a small shared resource cache (32 ready and 32 pending entries,
256 MiB accounting for native allocations, I/O shadows and conservative projection
metadata). Native codec calls run outside its resource-list mutex on the existing
executing worker. Cache misses, eviction, concurrent ownership transfer and fork
use the complete original guest state; no input-history log or service thread is
needed. A context remains subject to its upstream external synchronization rules.

Each entry contains two independently initialized **current** native C contexts,
with separate allocations and malloc fill patterns. Stable UnsafeCell allocations retain C self/allocator pointers without
borrowing immutable Rust objects across C callbacks. Native opaque objects,
vtables and callbacks always originate from those initializers. Guest opaque
bytes are compared with a projection and are never installed into a C object.
The paired contexts identify dynamic pointer correspondences. Initialized scalar
bytes must match; untouched malloc bytes and explicitly unused copied LZMA2
options project the original guest bytes without copying them into native state.
An init whose unused bytes do not yet match can retain a cold candidate: its
first data operation remains a shadow until a complete image/output match is
learned. Exact static pointer correspondences come from the executing native image.
`schema.rs` contains reviewed numeric AArch64 LP64 field maps, generated by
`scripts/generate-stream-schema.py`; provider versions, actual native root sizes, and original ELF SHA256
identities gate their use. ELF/Apple tracing builds have separate numeric maps.
Registered guest Zstd weak trace callbacks disable both codec shortcuts in the
library initializer, including when a callback returns zero. Only the reviewed Alpine/Debian codec images opt
into streaming. Debian zlib 1.3.1 inflate and xz 5.8.1 easy-encoder remain original:
unchanged private fields differ from the current providers even after a shadow
step. Their stateless adapters are unaffected. Other approved stateless images
keep their original stream APIs. Large xz encoder caches are declined before
constructing duplicate native encoders when the reported memory use cannot fit
the cache budget. The original guest encoder still runs normally.

The native providers remain zlib **1.4**, xz **5.8.4** and Zstd **1.6.0** from the
existing pinned sys crates. Older upstream headers are ABI inspection inputs;
older codec implementations are not linked. In particular this does not downgrade
native providers to match a guest private layout. Zstd's first workspace admission
can reconstruct its fresh context with the guest workspace's 64-byte alignment
class and replay its bounded pre-data parameter list plus that single shadow
step. It never replays a running stream's input history. Layout or output mismatch
still declines acceleration.

Before a fast step the cache compares the full guest opaque image, public state
handles and counters. It stages input and output in owned byte buffers, checks
both native results, builds the changed private/public/output writes and preflights
all reached output ranges before publishing anything. Unchanged native pages
retain their existing projection. On a failed admission no guest output or state
has been published; an original call can continue immediately. No fallback occurs
after successful publication. Host addresses do not become guest pointers.

Full-state validation has a cost. Zstd decompression requests with less than
64 KiB of compressed input retire the optional cache and execute the original
library; the 16 KiB benchmark showed a regression without this gate. This changes
only acceleration eligibility, with no new guest error, input limit or UI setting.
Throughput figures are host/interpreter measurements, not physical-iPad or
end-to-end apt/code-server timings.

Default gzip output describes the Linux guest (OS byte 3), including on Darwin.
Both emitted and still-pending header bytes are normalized, preserving original
continuation after an output fragment ends inside the header. Custom gzip-header
controls remain original through the state-mismatch path.
