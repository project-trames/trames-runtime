import os,json,subprocess,hashlib,statistics,math
from pathlib import Path
root=Path("/mnt/trames-output")
rows=[];medians={}
for workload,name in [("control-integer","compatibility-category"),("svc-round-trip","execution-entry")]:
 medians[workload]={"before":[],"after":[]}
 # Repeated identical-binary entries make process-placement variation observable.
 labels=["before","before","after","after","after","after","before","before"]*3
 for index,label in enumerate(labels):
  binary=root/f"jit-vector-encoding-bench-{label}-{name}"
  env=os.environ.copy();env.update(TRAMES_BENCH_FILTER=workload,TRAMES_BENCH_SAMPLES="128",TRAMES_BENCH_WARMUP="16",TRAMES_BENCH_REQUIRE_NATIVE="1")
  data=subprocess.check_output(["taskset","-c","0",str(binary)],env=env,text=True)
  samples=[json.loads(x) for x in data.splitlines()];assert len(samples)==128 and all(x["fallback"]==0 for x in samples)
  median=statistics.median(x["ns"] for x in samples if not x["warmup"])
  medians[workload][label].append(median)
  rows.extend(dict(x,cohort_index=index,version=label,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest()) for x in samples)
 summary={label:{"medians_ns":values,"range_ratio":max(values)/min(values),"geomean_ns":math.exp(statistics.mean(math.log(x) for x in values))} for label,values in medians[workload].items()}
 print(workload,json.dumps(summary),flush=True)
(root/"jit-vector-encoding-layout-samples.jsonl").write_text("".join(json.dumps(x,separators=(",",":"))+"\n" for x in rows))
(root/"jit-vector-encoding-layout-medians.json").write_text(json.dumps(medians,indent=2)+"\n")
