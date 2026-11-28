import hashlib,json,os,pathlib,shutil,subprocess,ctypes
root=pathlib.Path("/mnt/trames-output");source=root/"jit-lut-source";target=root/"jit-lut-linux-target"
env=os.environ.copy();env["CARGO_TARGET_DIR"]=str(target)
with (root/"jit-lut-benchmark-build.jsonl").open("w") as out,(root/"jit-lut-benchmark-build.log").open("w") as err:
 subprocess.run(["cargo","build","--release","--locked","-p","trames-jit","--features","native-aarch64","--example","lut-category","--message-format=json"],cwd=source,env=env,stdout=out,stderr=err,check=True)
messages=[json.loads(x) for x in (root/"jit-lut-benchmark-build.jsonl").read_text().splitlines()]
path=next(x["executable"]for x in messages if x.get("reason")=="compiler-artifact" and x.get("target",{}).get("name")=="lut-category" and x.get("executable"))
binary=root/"jit-lut-benchmark";shutil.copy2(path,binary)
libc=ctypes.CDLL(None);libc.getauxval.restype=ctypes.c_ulong
meta={"baseline_runtime":"7d74ff5809364b8e41081f507da2cf1d6e7a661b","rustc":subprocess.check_output(["rustc","-Vv"],text=True),"source":str(source),"target":str(target),"binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),"harness_sha256":hashlib.sha256((source/"crates/trames-jit/examples/lut-category.rs").read_bytes()).hexdigest(),"cpu_affinity":0,"page_size":os.sysconf("SC_PAGESIZE"),"hwcap":hex(libc.getauxval(16)),"hwcap2":hex(libc.getauxval(26)),"samples":16,"warmup":4,"rounds":8,"trips":65536,"order":["jit","interpreter","interpreter","jit","jit","interpreter","interpreter","jit"]}
(root/"jit-lut-benchmark-provenance.json").write_text(json.dumps(meta,indent=2)+"\n")
for workload in ["luti2-byte","luti2-half","luti4-byte","luti4-half"]:
 for index,backend in enumerate(meta["order"]):
  sample_env=os.environ.copy();sample_env.update(TRAMES_BENCH_BACKEND=backend,TRAMES_BENCH_FILTER=workload,TRAMES_BENCH_SAMPLES=str(meta["samples"]),TRAMES_BENCH_WARMUP=str(meta["warmup"]))
  output=root/f"jit-lut-benchmark-{workload}-{index}-{backend}.jsonl"
  with output.open("w") as out:subprocess.run(["taskset","-c","0",str(binary)],env=sample_env,stdout=out,check=True)
  rows=[json.loads(x)for x in output.read_text().splitlines()];assert len(rows)==16 and all(x["fallback"]==0 for x in rows)
  print("finished",workload,index,backend,flush=True)
