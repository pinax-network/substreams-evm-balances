#!/usr/bin/env python3
"""Stale rows of one token at a fixed block H: usage: compare.py <dir> <rpc-id>
Reads <dir>/seeded_rows.tsv (contract, address, block, hash, ts, balance: the token's current rows at or below H),
<dir>/at_H.part*.tsv (contract, address, balanceOf at H or empty) and <dir>/H. Writes <dir>/corrected_rows.tsv
(contract, address, H, H's hash, H's time UTC, chain balance, old balance, case) for every row whose value differs
from the chain at H, and prints counts and the largest differences. H's hash and time come from the RPC (PINAX_KEY).
"""
import collections, datetime, glob, json, os, ssl, sys, urllib.request
d, rid = sys.argv[1], sys.argv[2]
H = int(open(f"{d}/H").read())
U = f"https://{rid}.rpc.pinax.network/v1/{os.environ.get('PINAX_KEY', '')}/"
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")
q = {"jsonrpc": "2.0", "id": 1, "method": "eth_getBlockByNumber", "params": [hex(H), False]}
blk = json.loads(urllib.request.urlopen(urllib.request.Request(U, data=json.dumps(q).encode(), headers={"content-type": "application/json"}), timeout=60, context=CTX).read())["result"]
hh, ts = blk["hash"], int(blk["timestamp"], 16)
utc = datetime.datetime.fromtimestamp(ts, datetime.timezone.utc).strftime("%Y-%m-%d %H:%M:%S")
at = {}
for p in glob.glob(f"{d}/at_H.part*.tsv"):
    for l in open(p):
        c, a, v = l.rstrip("\n").split("\t"); at[(c, a)] = int(v) if v else None
cnt = collections.Counter(); out = []; delta = 0
for l in open(f"{d}/seeded_rows.tsv"):
    c, a, b, h, t, w = l.rstrip("\n").split("\t"); w = int(w)
    v = at.get((c, a), "missing")
    if v == "missing": cnt["not read"] += 1
    elif v is None: cnt["no word"] += 1
    elif v == w: cnt["equal"] += 1
    else:
        cnt["stale, row nonzero" if w else "stale, row zero"] += 1
        out.append((c, a, v, w)); delta += v - w
with open(f"{d}/corrected_rows.tsv", "w") as f:
    for c, a, v, w in out:
        f.write(f"{c}\t{a}\t{H}\t{hh}\t{utc}\t{v}\t{w}\tstale_at_H\n")
print(f"{d}: H {H} ({utc}) rows {sum(cnt.values())} {dict(cnt)} chain minus db {delta}")
for c, a, v, w in sorted(out, key=lambda x: -abs(x[2] - x[3]))[:5]:
    print(f"  {a} db {w} chain {v}")
