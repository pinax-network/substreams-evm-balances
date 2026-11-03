#!/usr/bin/env python3
"""eth_getBalance at the last pre-Bedrock block (105,235,999) for every seeded address. Resumable:
appends to chain_at_S1.tsv (address, wei) and skips addresses already there. Never prints the key."""
import json, os, ssl, sys, time, urllib.request, urllib.error
KEY = (os.environ.get("PINAX_KEY") or os.environ.get("SUBSTREAMS_API_KEY", ""))
URL = f"https://optimism.rpc.pinax.network/v1/{KEY}/"
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")
BLOCK = hex(105235999)
SHARD, SHARDS = int(sys.argv[1]), int(sys.argv[2])
OUT = f"chain_at_S1.part{SHARD}.tsv"
import glob, zlib
done = set()
for path in ["chain_at_S1.tsv"] + glob.glob("chain_at_S1.part*.tsv"):
    if os.path.exists(path):
        for line in open(path):
            done.add(line.split("\t", 1)[0])
todo = []
for line in open("seeded_rows.tsv"):
    a = line.split("\t", 1)[0]
    if a != "address" and a not in done and zlib.crc32(a.encode()) % SHARDS == SHARD:
        todo.append(a)
print(f"{len(done)} done, {len(todo)} to fetch", flush=True)
def post(payload):
    req = urllib.request.Request(URL, data=json.dumps(payload).encode(), headers={"content-type": "application/json"})
    for attempt in range(8):
        try:
            with urllib.request.urlopen(req, timeout=120, context=CTX) as r:
                return json.loads(r.read())
        except Exception as e:  # noqa: BLE001
            time.sleep(min(60, 5 * (attempt + 1)))
    raise SystemExit("rpc: 8 failures in a row")
t0 = time.time(); n = 0; errors = 0
with open(OUT, "a") as f:
    for i in range(0, len(todo), 50):
        chunk = todo[i:i + 50]
        res = post([{"jsonrpc": "2.0", "id": k, "method": "eth_getBalance", "params": [a, BLOCK]} for k, a in enumerate(chunk)])
        by = {r["id"]: r for r in res}
        for k, a in enumerate(chunk):
            r = by.get(k, {})
            if "result" in r:
                f.write(f"{a}\t{int(r['result'], 16)}\n"); n += 1
            else:
                errors += 1
        f.flush()
        if (i // 50) % 200 == 0:
            el = time.time() - t0
            print(f"{n} fetched, {errors} errors, {el:.0f}s, {n / el if el else 0:.0f}/s", flush=True)
print(f"finished: {n} fetched, {errors} errors, {time.time() - t0:.0f}s", flush=True)
