#!/usr/bin/env python3
"""Re-read a random sample of corrected_rows.tsv at each row's block: usage spot.py <chain-dir> <n>; RPC_ID, PINAX_KEY."""
import json, os, random, ssl, sys, urllib.request, collections
d, n = sys.argv[1], int(sys.argv[2])
URL = f"https://{os.environ['RPC_ID']}.rpc.pinax.network/v1/{os.environ.get('PINAX_KEY', '')}/"
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")
rows = [l.rstrip("\n").split("\t") for l in open(f"{d}/corrected_rows.tsv")]
random.seed(7); pick = random.sample(rows, n)
by = collections.defaultdict(list)
for r in rows: by[r[7]].append(r)
for k in by: pick += random.sample(by[k], min(3, len(by[k])))
ok = bad = 0
for c, a, b, h, ts, v, old, case in pick:
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "eth_call", "params": [{"to": c, "data": "0x70a08231" + "0" * 24 + a[2:]}, hex(int(b))]}).encode()
    r = json.loads(urllib.request.urlopen(urllib.request.Request(URL, data=body, headers={"content-type": "application/json"}), timeout=120, context=CTX).read())
    got = int(r["result"][:66], 16)
    if got == int(v): ok += 1
    else: bad += 1; print("MISMATCH", c, a, b, case, v, got)
print(f"{d}: {ok} match, {bad} differ of {ok + bad}")
