#!/usr/bin/env python3
"""Build the corrected seeded ERC-20 rows of one chain.

usage: compare.py <chain-dir> <S> <S1_hash> <S1_unix_ts>
Inputs in <chain-dir>: seeded_rows.tsv (contract, address, block, hash, ts, balance), at_S1.part*.tsv (contract,
address, value at S-1 or empty). For each row whose value differs from the chain at S-1, it reads the chain at the
row's own block (resumable into at_row.tsv): when the final value was already there the row keeps its block, hash
and timestamp (receipt_block); otherwise it moves to S-1 (boundary). Rows whose call returned no word are left.
Output corrected_rows.tsv: contract, address, block, hash, timestamp UTC, balance, old_balance, case.
RPC from PINAX_KEY and RPC_ID (never printed).
"""
import collections, concurrent.futures, datetime, glob, json, os, ssl, sys, time, urllib.request

d, S, S1_HASH, S1_TS = sys.argv[1], int(sys.argv[2]), sys.argv[3], int(sys.argv[4])
URL = f"https://{os.environ['RPC_ID']}.rpc.pinax.network/v1/{os.environ.get('PINAX_KEY', '')}/"
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")
utc = lambda ts: datetime.datetime.fromtimestamp(ts, datetime.timezone.utc).strftime("%Y-%m-%d %H:%M:%S")


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



s1 = {}
for p in glob.glob(f"{d}/at_S1.part*.tsv"):
    for line in open(p):
        c, a, v = line.rstrip("\n").split("\t")
        s1[(c, a)] = int(v) if v else None
at_h = {}
for p in glob.glob(f"{d}/at_H.part*.tsv"):
    for line in open(p):
        c, a, v = line.rstrip("\n").split("\t")
        at_h[(c, a)] = int(v) if v else None
rows, missing = [], 0
cnt = collections.Counter()
need_s1 = []
for line in open(f"{d}/seeded_rows.tsv"):
    c, a, b, h, ts, w = line.rstrip("\n").split("\t")
    key = (c, a)
    if key in s1:
        # keep only the rows that need work; count the rest here (memory: 12 M rows on Optimism)
        hv = at_h.get(key)
        if s1[key] is None:
            cnt["no_word"] += 1
        elif s1[key] == int(w) and (hv is None or hv == int(w)):
            cnt["equal"] += 1
        else:
            rows.append((c, a, int(b), h, int(ts), int(w)))
    elif key in at_h:
        # Read at the head block only (chains where S-1 is slow): a row equal at head is taken as right, since the
        # package never rewrote it after S; a row that differs is read at S-1 below.
        if at_h[key] is None:
            cnt["no_word"] += 1
        elif at_h[key] == int(w):
            cnt["equal_at_head"] += 1
        else:
            need_s1.append(key)
            rows.append((c, a, int(b), h, int(ts), int(w)))
    else:
        missing += 1
if missing:
    raise SystemExit(f"{missing} seeded rows have no value yet; finish fetch.py first")
if need_s1:
    print(f"{len(need_s1)} rows differ at head: reading them at S-1", flush=True)
    res = []
    with concurrent.futures.ThreadPoolExecutor(8) as ex0:
        def s1_one(chunk):
            r = post([{"jsonrpc": "2.0", "id": k, "method": "eth_call",
                       "params": [{"to": c, "data": "0x70a08231" + "0" * 24 + a[2:]}, hex(S - 1)]} for k, (c, a) in enumerate(chunk)])
            by = {x.get("id"): x for x in r} if isinstance(r, list) else {}
            return [(c, a, (lambda v: int(v[:66], 16) if isinstance(v, str) and len(v) >= 66 else None)((by.get(k) or {}).get("result")))
                    for k, (c, a) in enumerate(chunk)]
        for got in ex0.map(s1_one, [need_s1[i:i + 50] for i in range(0, len(need_s1), 50)]):
            res += got
    with open(f"{d}/at_S1.part_from_head.tsv", "a") as f:
        for c, a, v in res:
            s1[(c, a)] = v
            f.write(f"{c}\t{a}\t{'' if v is None else v}\n")
differ = []
post_start = []
for c, a, b, h, ts, w in rows:
    v = s1[(c, a)]
    if v is not None and v == w and (c, a) in at_h and at_h[(c, a)] is not None and at_h[(c, a)] != w:
        cnt["right_at_S1_changed_after"] += 1
        post_start.append((c, a, b, w, at_h[(c, a)]))
        continue
    if v is None:
        cnt["no_word"] += 1
    elif v == w:
        cnt["equal"] += 1
    else:
        cnt["differs_zero" if w == 0 else "differs_nonzero"] += 1
        differ.append((c, a, b, h, ts, w, v))
print(f"seeded rows {sum(cnt.values())}: {dict(cnt)}", flush=True)

at_row = {}
if os.path.exists(f"{d}/at_row.tsv"):
    for line in open(f"{d}/at_row.tsv"):
        c, a, b, v = line.rstrip("\n").split("\t")
        at_row[(c, a, int(b))] = int(v) if v else None
# Tokens with many differing rows are computed balances (index-scaled, rebasing, Optimism's legacy ETH): their
# balanceOf moves without a stored change, so the read at the row's block cannot match and is skipped; their rows
# are corrected at S-1 (case boundary_computed).
MAXR = int(os.environ.get("ROW_READ_MAX_PER_TOKEN", "1000"))
per_token = collections.Counter(c for c, *_ in differ)
many = {c for c, n in per_token.items() if n > MAXR}
print(f"{len(many)} tokens with more than {MAXR} differing rows ({sum(per_token[c] for c in many)} rows) skip the row-block read", flush=True)
todo = [(c, a, b) for c, a, b, *_ in differ if c not in many and (c, a, b) not in at_row]
print(f"second read at the row's block: {len(at_row)} done, {len(todo)} to fetch", flush=True)
# ROW_READ=0 skips the reads not done yet (Arbitrum One's classic node does about 4 a second): an unread row is
# corrected at S-1 like a boundary row, which is right for its current balance; only its history before S-1 keeps
# the old value, and the reads that finished show that case is rare (17 of 18,236 rows on Arbitrum One).
if os.environ.get("ROW_READ", "1") == "0":
    print(f"ROW_READ=0: {len(todo)} rows without a read are corrected at S-1", flush=True)
    todo = []


def one(chunk):
    res = post([{"jsonrpc": "2.0", "id": k, "method": "eth_call",
                 "params": [{"to": c, "data": "0x70a08231" + "0" * 24 + a[2:]}, hex(b)]} for k, (c, a, b) in enumerate(chunk)])
    by = {r.get("id"): r for r in res} if isinstance(res, list) else {}
    out = []
    for k, (c, a, b) in enumerate(chunk):
        v = (by.get(k) or {}).get("result")
        out.append((c, a, b, int(v[:66], 16) if isinstance(v, str) and len(v) >= 66 else None))
    return out


RB, RW = int(os.environ.get("ROW_BATCH", "50")), int(os.environ.get("ROW_WORKERS", "8"))
with open(f"{d}/at_row.tsv", "a") as f, concurrent.futures.ThreadPoolExecutor(RW) as ex:
    for got in ex.map(one, [todo[i:i + RB] for i in range(0, len(todo), RB)]):
        for c, a, b, v in got:
            at_row[(c, a, b)] = v
            f.write(f"{c}\t{a}\t{b}\t{'' if v is None else v}\n")

case = collections.Counter(); by_token = collections.Counter(); delta = collections.Counter()
with open(f"{d}/corrected_rows.tsv", "w") as f:
    for c, a, b, h, ts, w, v in differ:
        if c in many:
            f.write(f"{c}\t{a}\t{S - 1}\t{S1_HASH}\t{utc(S1_TS)}\t{v}\t{w}\tboundary_computed\n"); case["boundary_computed"] += 1
        elif at_row.get((c, a, b)) == v:
            f.write(f"{c}\t{a}\t{b}\t{h}\t{utc(ts)}\t{v}\t{w}\treceipt_block\n"); case["receipt_block"] += 1
        else:
            f.write(f"{c}\t{a}\t{S - 1}\t{S1_HASH}\t{utc(S1_TS)}\t{v}\t{w}\tboundary\n"); case["boundary"] += 1
        by_token[c] += 1; delta[c] += v - w
print(f"corrected_rows.tsv: {sum(case.values())} rows {dict(case)} in {len(by_token)} tokens")
with open(f"{d}/changed_after_start.tsv", "w") as f2:
    for c, a, b, w, vh in post_start:
        f2.write(f"{c}\t{a}\t{b}\t{w}\t{vh}\n")
print(f"right at S-1 but different at head (changes after S the package missed, not corrected here): {len(post_start)} -> changed_after_start.tsv")
print("tokens with the most corrections (rows, chain minus db in raw units):")
for c, n in by_token.most_common(12):
    print(f"  {c} {n:7} {delta[c]}")
