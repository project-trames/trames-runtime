#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Inspect upstream AArch64 LP64 headers; emit numeric streaming ABI maps.

These source trees are reference inputs only. No old codec implementation is
compiled into Trames. The executing providers remain the pinned sys crates.
"""
import argparse
import json
from pathlib import Path
import platform
import subprocess

p = argparse.ArgumentParser(description=__doc__)
for name in ('guest-zlib132', 'guest-zlib131', 'guest-zstd157', 'native-zlib', 'native-zstd', 'native-xz'):
    p.add_argument('--' + name, type=Path, required=True)
p.add_argument('--records', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
if platform.machine().lower() not in ('arm64', 'aarch64'):
    raise SystemExit('Run on AArch64: these are native LP64 layout probes')
a.records.mkdir(parents=True, exist_ok=True)
configs = [('old-zlib', a.guest_zlib132, 'zlib', []),
           ('old131-zlib', a.guest_zlib131, 'zlib', []),
           ('native-zlib', a.native_zlib, 'zlib', []),
           ('old-zstd', a.guest_zstd157 / 'lib', 'zstd', ['-DZSTD_MULTITHREAD=1', '-DZSTD_LEGACY_SUPPORT=1', '-DZSTD_TRACE=0']),
           ('old-zstd-trace', a.guest_zstd157 / 'lib', 'zstd', ['-DZSTD_MULTITHREAD=1', '-DZSTD_LEGACY_SUPPORT=1', '-DZSTD_TRACE=1']),
           ('native-zstd', a.native_zstd / 'lib', 'zstd', ['-DZSTD_LEGACY_SUPPORT=0', '-DZSTD_TRACE=0']),
           ('native-zstd-trace', a.native_zstd / 'lib', 'zstd', ['-DZSTD_LEGACY_SUPPORT=0', '-DZSTD_TRACE=1'])]
layouts = {}
for name, source, kind, flags in configs:
    includes = ('#include "zutil.h"\n#include "inftrees.h"\n#include "inflate.h"\n#include "deflate.h"\n'
                if kind == 'zlib' else '#include "compress/zstd_compress_internal.h"\n#include "decompress/zstd_decompress_internal.h"\n')
    probe = a.records / (name + '.c')
    probe.write_text(includes)
    compiler = ['clang', '-O2', '-I', str(source.resolve()), '-DDYNAMIC_BMI2=0', *flags]
    ast = json.loads(subprocess.check_output([*compiler, '-Xclang', '-ast-dump=json', '-fsyntax-only', str(probe)]))
    records = []
    def walk(node):
        if node.get('kind') == 'RecordDecl' and node.get('completeDefinition'):
            records.append((node.get('name', ''), [f['name'] for f in node.get('inner', []) if f.get('kind') == 'FieldDecl' and 'name' in f]))
        for child in node.get('inner', []):
            walk(child)
    walk(ast)
    wanted = ('inflate_state', 'internal_state') if kind == 'zlib' else ('ZSTD_CCtx_s', 'ZSTD_DCtx_s')
    types = {n: ('struct ' + n, fields) for n, fields in records if n in wanted}
    if kind == 'zstd':
        fields = next(fields for _, fields in records if 'partitions' in fields and 'entropyMetadata' in fields)
        types['blockSplitCtx'] = ('ZSTD_blockSplitCtx', fields)
    rows = ['#include <stddef.h>', '#include <stdio.h>', includes, 'int main(void){puts("{");']
    for i, (label, (ctype, fields)) in enumerate(types.items()):
        rows.append(f'printf("\\\"{label}\\\":{{\\\"size\\\":%zu,\\\"fields\\\":{{",sizeof({ctype}));')
        for j, field in enumerate(fields):
            comma = ',' if j + 1 < len(fields) else ''
            rows.append(f'printf("\\\"{field}\\\":[%zu,%zu]{comma}",offsetof({ctype},{field}),sizeof((({ctype}*)0)->{field}));')
        rows.append('puts("}}' + (',' if i + 1 < len(types) else '') + '");')
    rows.append('puts("}");return 0;}')
    probe.write_text('\n'.join(rows))
    exe = a.records / name
    subprocess.run([*compiler, str(probe), '-o', str(exe)], check=True)
    layouts[name] = json.loads(subprocess.check_output([str(exe.resolve())]))
    (a.records / (name + '.json')).write_text(json.dumps(layouts[name], indent=2) + '\n')
# The only skipped copied stack fields are unused easy_encoder LZMA2 options.
xz = a.native_xz
include_dirs = [xz, xz / 'xz/src/common', *[xz / 'xz/src/liblzma' / n for n in ('api', 'lzma', 'lz', 'check', 'simple', 'delta', 'common', 'rangecoder')]]
checks = {'stream_encoder.c': 'sizeof(lzma_stream_coder)==1504 && offsetof(lzma_stream_coder,filters)==296',
          'lzma2_encoder.c': 'sizeof(lzma_lzma2_coder)==65704 && offsetof(lzma_lzma2_coder,opt_cur)==16 && sizeof(lzma_options_lzma)==112 && offsetof(lzma_options_lzma,ext_flags)==48'}
for filename, condition in checks.items():
    probe = a.records / (filename + '-check.c')
    probe.write_text(f'#include "{filename}"\n#include <stddef.h>\n_Static_assert({condition},"review stream option masks");\n')
    subprocess.run(['clang', '-DHAVE_CONFIG_H=1', *[v for d in include_dirs for v in ('-I', str(d.resolve()))], '-fsyntax-only', str(probe)], check=True)
items = [('ZLIB132_DEFLATE', 'old-zlib', 'internal_state', 'native-zlib'),
         ('ZLIB131_DEFLATE', 'old131-zlib', 'internal_state', 'native-zlib'),
         ('ZLIB_INFLATE', 'old-zlib', 'inflate_state', 'native-zlib')]
for guest_suffix in ('', '-trace'):
    for native_suffix in ('', '-trace'):
        for kind, typename in [('C', 'ZSTD_CCtx_s'), ('D', 'ZSTD_DCtx_s')]:
            label=f'ZSTD157_{kind}_G{int(bool(guest_suffix))}_N{int(bool(native_suffix))}'
            items.append((label, 'old-zstd'+guest_suffix, typename, 'native-zstd'+native_suffix))
lines = ['// SPDX-License-Identifier: GPL-3.0-or-later',
         '//! LP64 AArch64 maps from scripts/generate-stream-schema.py.',
         '//! Only the current zlib 1.4 / zstd 1.6 implementations execute.',
         'use super::codec::Kind;',
         "pub(super) struct Layout {pub native_len:usize,pub guest_len:usize,pub pieces:&'static[(usize,usize,usize)],pub zero:&'static[(usize,usize)]}"]
for label, old, typename, provider in items:
    guest, native = layouts[old], layouts[provider]
    g, n = guest[typename], native[typename]
    pieces, zero = [], []
    for field, (go, gs) in g['fields'].items():
        if field not in n['fields']:
            zero.append((go, gs))
            continue
        no, ns = n['fields'][field]
        if field == 'blockSplitCtx':
            for child, (xg, xs) in guest['blockSplitCtx']['fields'].items():
                xn, xns = native['blockSplitCtx']['fields'][child]
                assert xs == xns or child == 'partitions'
                pieces.append((no + xn, go + xg, min(xs, xns)))
        else:
            assert gs == ns, (field, gs, ns)
            pieces.append((no, go, gs))
    merged = []
    for n0, g0, size in sorted(pieces):
        if merged and merged[-1][0] + merged[-1][2] == n0 and merged[-1][1] + merged[-1][2] == g0:
            x, y, z = merged[-1]
            merged[-1] = (x, y, z + size)
        else:
            merged.append((n0, g0, size))
    lines.append(f'static {label}:Layout=Layout{{native_len:{n["size"]},guest_len:{g["size"]},pieces:&{merged!r},zero:&{zero!r}}};')
lines += ["pub(super) fn select(kind:Kind,abi:u64,native_len:usize)->Option<Option<&'static Layout>> {Some(match (kind,abi,native_len) {",
          '(Kind::Deflate,1,5968)=>Some(&ZLIB132_DEFLATE),', '(Kind::Deflate,2,5968)=>Some(&ZLIB131_DEFLATE),',
          '(Kind::Inflate,1|2,7176)=>Some(&ZLIB_INFLATE),']
for abi, guest_trace in [(3, 0), (5, 1)]:
    for native_trace, provider in [(0, 'native-zstd'), (1, 'native-zstd-trace')]:
        for kind, typename, enum in [('C', 'ZSTD_CCtx_s', 'ZstdC'), ('D', 'ZSTD_DCtx_s', 'ZstdD')]:
            size=layouts[provider][typename]['size']
            lines.append(f'(Kind::{enum},{abi},{size})=>Some(&ZSTD157_{kind}_G{guest_trace}_N{native_trace}),')
lines += ['(Kind::Xz,4|6,_)=>None,', '_=>return None,', '})}']
a.output.write_text('\n'.join(lines) + '\n')
subprocess.run(['rustfmt', '--edition', '2021', str(a.output)], check=True)
