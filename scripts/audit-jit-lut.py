#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent fixed-width LUT assembly and selector audit using existing LLVM 23."""
import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--write',action='store_true')
args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
out=root/'docs/validation/jit-lut-20261128'
cases=[]
for index_bits in [2,4]:
    for bits in [8,16]:
        for segment in range(128//((128//bits)*index_bits)):
            cases.append((index_bits,bits,segment))
assembly='.arch_extension lut\n.text\n'
for index_bits,bits,segment in cases:
    suffix='16b' if bits==8 else '8h'
    table='{v0.'+suffix+'}' if index_bits==2 or bits==8 else '{v0.8h,v1.8h}'
    assembly+=f'luti{index_bits} v0.{suffix}, {table}, v0[{segment}]\n'
with tempfile.TemporaryDirectory(prefix='trames-lut-llvm-') as directory:
    directory=Path(directory);source=directory/'fixture.rs';obj=directory/'fixture.o'
    source.write_text('#![no_std]\ncore::arch::global_asm!(r###"'+assembly+'"###,options(raw));\n')
    subprocess.run(['rustc','--edition=2021','--crate-type=lib','--emit=obj',str(source),'-o',str(obj)],check=True)
    data=obj.read_bytes()
assert data[:6]==b'\x7fELF\x02\x01'
offset=struct.unpack_from('<Q',data,40)[0];size,count,names_index=struct.unpack_from('<HHH',data,58)
sections=[struct.unpack_from('<IIQQQQIIQQ',data,offset+i*size) for i in range(count)]
names_section=sections[names_index];names=data[names_section[4]:names_section[4]+names_section[5]]
text=[data[s[4]:s[4]+s[5]] for s in sections if names[s[0]:].split(b'\0',1)[0]==b'.text']
assert len(text)==1 and len(text[0])==18*4
words=struct.unpack('<18I',text[0])
sysroot=Path(subprocess.check_output(['rustc','--print','sysroot'],text=True).strip())
library=next((sysroot/'lib').glob('libLLVM.so.23*'));llvm=ctypes.CDLL(str(library))
for name in ['LLVMInitializeAArch64TargetInfo','LLVMInitializeAArch64TargetMC','LLVMInitializeAArch64Disassembler']:getattr(llvm,name)()
llvm.LLVMCreateDisasmCPUFeatures.restype=ctypes.c_void_p
llvm.LLVMCreateDisasmCPUFeatures.argtypes=[ctypes.c_char_p,ctypes.c_char_p,ctypes.c_char_p,ctypes.c_void_p,ctypes.c_int,ctypes.c_void_p,ctypes.c_void_p]
llvm.LLVMDisasmInstruction.restype=ctypes.c_size_t
llvm.LLVMDisasmInstruction.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_uint64,ctypes.c_uint64,ctypes.c_void_p,ctypes.c_size_t]
context=llvm.LLVMCreateDisasmCPUFeatures(b'aarch64-linux-gnu',b'generic',b'+v9.4a,+lut',None,0,None,None)
def decode(word):
    data=ctypes.create_string_buffer(word.to_bytes(4,'little'));output=ctypes.create_string_buffer(256)
    return output.value.decode().strip() if llvm.LLVMDisasmInstruction(context,data,4,0,output,256)==4 else ''
for word in words:assert decode(word).startswith(('luti2','luti4'))
selectors=[]
for high in range(512):
    if (high>>2)&15 not in [7,15]:continue
    for middle in range(8192):
        word=(high<<23)|(middle<<10)|(1<<5)
        if decode(word).startswith(('luti2','luti4')):selectors.append(word)
assert len(selectors)==576
files={'category.S':assembly,'forms.words':''.join(f'{w:08x} {i} {b} {s}\n' for w,(i,b,s) in zip(words,cases)),
       'selectors.words':''.join(f'{w:08x}\n' for w in selectors),
       'oracle.json':json.dumps({'forms':18,'complete_legal_encodings':18*32**3,'selector_scope':524288,'lut_selectors':576,'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'library_sha256':hashlib.sha256(library.read_bytes()).hexdigest()},indent=2)+'\n'}
if args.write:out.mkdir(parents=True,exist_ok=True)
for name,content in files.items():
    if args.write:(out/name).write_text(content)
    else:assert (out/name).read_text()==content,name
print(files['oracle.json'],end='')
