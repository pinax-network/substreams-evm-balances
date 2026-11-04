#!/usr/bin/env python3
"""balanceOf(holder) at block S-1 for every seeded ERC-20 row. Resumable and sharded.

usage: fetch.py <chain-dir> <block> <shard> <shards> [workers]
Reads <chain-dir>/seeded_rows.tsv (contract, address, block, hash, ts, balance), writes
<chain-dir>/at_S1.part<shard>.tsv (contract, address, value or empty when the call returned no word).
RPC from PINAX_KEY (never printed); RPC_ID env selects the Pinax chain id.
"""
import concurrent.futures, glob, json, os, ssl, sys, time, urllib.request, zlib

d, block, shard, shards = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
workers = int(sys.argv[5]) if len(sys.argv) > 5 else 8
KEY = os.environ.get("PINAX_KEY", "")
URL = f"https://{os.environ['RPC_ID']}.rpc.pinax.network/v1/{KEY}/"
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")
PREFIX = os.environ.get("OUT_PREFIX", "at_S1")
OUT = f"{d}/{PREFIX}.part{shard}.tsv"
done = set()
for p in glob.glob(f"{d}/at_S1.part*.tsv") + glob.glob(f"{d}/{PREFIX}.part*.tsv"):
    for line in open(p):
        c, a, _ = line.rstrip("\n").split("\t")
        done.add((c, a))
todo = []
for line in open(f"{d}/seeded_rows.tsv"):
    c, a = line.split("\t", 2)[:2]
    if (c, a) not in done and zlib.crc32((c + a).encode()) % shards == shard:
        todo.append((c, a))
print(f"shard {shard}: {len(done)} done overall, {len(todo)} to fetch", flush=True)


def post(payload):
    data = json.dumps(payload).encode()
    for attempt in range(10):
        try:
            req = urllib.request.Request(URL, data=data, headers={"content-type": "application/json"})
            with urllib.request.urlopen(req, timeout=120, context=CTX) as r:
                return json.loads(r.read())
        except Exception:  # noqa: BLE001
            time.sleep(min(60, 3 * (attempt + 1)))
    raise SystemExit("rpc: 10 failures in a row")


def one(chunk):
    payload = [{"jsonrpc": "2.0", "id": k, "method": "eth_call",
                "params": [{"to": c, "data": "0x70a08231" + "0" * 24 + a[2:]}, hex(block)]} for k, (c, a) in enumerate(chunk)]
    res = post(payload)
    by = {r.get("id"): r for r in res} if isinstance(res, list) else {}
    out = []
    for k, (c, a) in enumerate(chunk):
        v = (by.get(k) or {}).get("result")
        out.append((c, a, str(int(v[:66], 16)) if isinstance(v, str) and len(v) >= 66 else ""))
    return out


chunks = [todo[i:i + 50] for i in range(0, len(todo), 50)]
t0 = time.time(); n = 0
with open(OUT, "a") as f, concurrent.futures.ThreadPoolExecutor(workers) as ex:
    for i, got in enumerate(ex.map(one, chunks)):
        for c, a, v in got:
            f.write(f"{c}\t{a}\t{v}\n")
        n += len(got)
        if i % 400 == 0:
            f.flush()
            print(f"shard {shard}: {n} fetched, {time.time() - t0:.0f}s, {n / max(1, time.time() - t0):.0f}/s", flush=True)
print(f"shard {shard}: finished {n} in {time.time() - t0:.0f}s", flush=True)
