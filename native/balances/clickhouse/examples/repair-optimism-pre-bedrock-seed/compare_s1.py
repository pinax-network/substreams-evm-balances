#!/usr/bin/env python3
"""Build the corrected pre-Bedrock rows for `optimism:evm-balances-native@v0.7.11`.

Inputs (same directory):
  seeded_rows_hash.tsv  address, block_num, block_hash, ts, balance_wei   (current rows below 105,236,000, FINAL)
  chain_at_S1.tsv       address, wei                                      (eth_getBalance at 105,235,999)
Second read, resumable into chain_at_row.tsv: eth_getBalance at the row's own block, only for rows whose
seeded balance differs from the chain at 105,235,999.

Output corrected_rows.tsv (no header), one row per address to correct:
  address, block_num, block_hash, timestamp (UTC 'YYYY-MM-DD HH:MM:SS'), balance_wei, old_balance_wei, fix_case
  fix_case = receipt_block   the chain already held the final value at the row's block: keep block, hash, timestamp
           = bedrock_boundary the value changed between the row's block and Bedrock: row moves to 105,235,999
"""
import datetime, json, os, ssl, time, urllib.request

KEY = (os.environ.get("PINAX_KEY") or os.environ.get("SUBSTREAMS_API_KEY", ""))
URL = f"https://optimism.rpc.pinax.network/v1/{KEY}/"
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")
S1 = 105235999
S1_HASH = "0x33534e1fcc96457cc297e9e04db4f81245cff6b28399d40fe795e2623b366f47"
S1_TS = 1686070775


def post(payload):
    req = urllib.request.Request(URL, data=json.dumps(payload).encode(), headers={"content-type": "application/json"})
    for attempt in range(8):
        try:
            with urllib.request.urlopen(req, timeout=120, context=CTX) as r:
                return json.loads(r.read())
        except Exception:  # noqa: BLE001
            time.sleep(min(60, 5 * (attempt + 1)))
    raise SystemExit("rpc: 8 failures in a row")


def utc(ts):
    return datetime.datetime.fromtimestamp(ts, datetime.timezone.utc).strftime("%Y-%m-%d %H:%M:%S")


seeded = {}
for line in open("seeded_rows_hash.tsv"):
    p = line.rstrip("\n").split("\t")
    if p[0] == "address":
        continue
    seeded[p[0]] = (int(p[1]), p[2], int(p[3]), int(p[4]))
import glob
chain_s1 = {}
for path in ["chain_at_S1.tsv"] + sorted(glob.glob("chain_at_S1.part*.tsv")):
    if os.path.exists(path):
        for line in open(path):
            a, w = line.rstrip("\n").split("\t")
            chain_s1[a] = int(w)
missing = [a for a in seeded if a not in chain_s1]
if missing:
    raise SystemExit(f"{len(missing)} seeded addresses have no chain value yet; finish fetch_s1.py first")

equal = [a for a in seeded if seeded[a][3] == chain_s1[a]]
differs = [a for a in seeded if seeded[a][3] != chain_s1[a]]
print(f"seeded rows {len(seeded)}: equal to the chain at {S1}: {len(equal)}, differ: {len(differs)}")
zero_wrong = sum(1 for a in differs if seeded[a][3] == 0)
print(f"  of the differing rows, seeded as zero: {zero_wrong}, seeded non-zero: {len(differs) - zero_wrong}")
print(f"  chain value at {S1} summed over differing rows: {sum(chain_s1[a] for a in differs) / 1e18:.4f} ETH"
      f" (seeded sum {sum(seeded[a][3] for a in differs) / 1e18:.4f} ETH)")

# second read: the chain at each differing row's own block
at_row = {}
if os.path.exists("chain_at_row.tsv"):
    for line in open("chain_at_row.tsv"):
        a, b, w = line.rstrip("\n").split("\t")
        at_row[(a, int(b))] = int(w)
todo = [(a, seeded[a][0]) for a in differs if (a, seeded[a][0]) not in at_row]
print(f"second read at the row's block: {len(at_row)} done, {len(todo)} to fetch")
import concurrent.futures
def read_chunk(chunk):
    res = post([{"jsonrpc": "2.0", "id": k, "method": "eth_getBalance", "params": [a, hex(b)]} for k, (a, b) in enumerate(chunk)])
    by = {r["id"]: r for r in res}
    return [(a, b, int(by[k]["result"], 16)) for k, (a, b) in enumerate(chunk) if "result" in by.get(k, {})]
chunks = [todo[i:i + 50] for i in range(0, len(todo), 50)]
t0 = time.time()
with open("chain_at_row.tsv", "a") as f, concurrent.futures.ThreadPoolExecutor(8) as ex:
    for n, got in enumerate(ex.map(read_chunk, chunks)):
        for a, b, v in got:
            at_row[(a, b)] = v
            f.write(f"{a}\t{b}\t{v}\n")
        if n % 500 == 0:
            f.flush(); print(f"  {n * 50} read, {time.time() - t0:.0f}s", flush=True)

receipt = boundary = 0
with open("corrected_rows.tsv", "w") as f:
    for a in differs:
        b, h, ts, old = seeded[a]
        final = chain_s1[a]
        v_row = at_row.get((a, b))
        if v_row is None:
            raise SystemExit(f"no chain value at block {b} for {a}")
        if v_row == final:
            f.write(f"{a}\t{b}\t{h}\t{utc(ts)}\t{final}\t{old}\treceipt_block\n"); receipt += 1
        else:
            f.write(f"{a}\t{S1}\t{S1_HASH}\t{utc(S1_TS)}\t{final}\t{old}\tbedrock_boundary\n"); boundary += 1
print(f"corrected_rows.tsv: {receipt} keep their block (receipt_block), {boundary} move to {S1} (bedrock_boundary)")
