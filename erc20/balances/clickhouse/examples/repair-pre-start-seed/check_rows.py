#!/usr/bin/env python3
"""Check a random sample of the seeded pairs in the database against the chain at the last block before S.

usage: check_rows.py <chain-dir> <S> <n>
Draws n random (contract, holder) pairs from <chain-dir>/seeded_rows.tsv, reads each pair's current row from
`<chain>:evm-balances-erc20@v0.7.4` (by primary key, read-only), and for every pair still current below S reads
balanceOf at block S-1: a row current below S claims its balance there. (A read at the row's own block cannot see
the defect: a stale seeded row was right when the RPC era wrote it.) Prints the counts and writes
<chain-dir>/check_rows.tsv. Run it before and after the repair.
Env: CHAIN, RPC_ID, PINAX_KEY (never printed), CH_URL with CH_CURL_CONFIG (a curl -K file) or CH_USER/CH_PASSWORD.
"""
import collections, concurrent.futures, json, os, random, ssl, subprocess, sys, time, urllib.request

d, S, n = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
DB = f"`{os.environ['CHAIN']}:evm-balances-erc20@v0.7.4`.erc20_balances"
URL = f"https://{os.environ['RPC_ID']}.rpc.pinax.network/v1/{os.environ.get('PINAX_KEY', '')}/"
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")


def ch(sql):
    url = os.environ["CH_URL"] + "/?readonly=2&max_execution_time=600"
    cmd = ["curl", "-sS", "--fail-with-body", "--max-time", "660", url, "--data-binary", "@-"]
    cmd[1:1] = ["-K", os.environ["CH_CURL_CONFIG"]] if os.environ.get("CH_CURL_CONFIG") else \
        ["-u", f"{os.environ.get('CH_USER', 'default')}:{os.environ.get('CH_PASSWORD', '')}"]
    r = subprocess.run(cmd, input=(sql + "\nFORMAT TSV").encode(), capture_output=True)
    if r.returncode != 0:
        raise SystemExit("clickhouse: " + (r.stdout + r.stderr).decode(errors="replace")[:400])
    return [l.split("\t") for l in r.stdout.decode().splitlines() if l]


def balance_of(c, a, b):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "eth_call",
                       "params": [{"to": c, "data": "0x70a08231" + "0" * 24 + a[2:]}, hex(b)]}).encode()
    for attempt in range(8):
        try:
            req = urllib.request.Request(URL, data=body, headers={"content-type": "application/json"})
            with urllib.request.urlopen(req, timeout=120, context=CTX) as r:
                v = json.loads(r.read()).get("result")
            return int(v[:66], 16) if isinstance(v, str) and len(v) >= 66 else None
        except Exception:  # noqa: BLE001
            time.sleep(3 * (attempt + 1))
    return None


pairs = [tuple(l.split("\t", 2)[:2]) for l in open(f"{d}/seeded_rows.tsv")]
random.seed(20261103)
pick = random.sample(pairs, n)
cur = {}
for i in range(0, len(pick), 1000):
    chunk = pick[i:i + 1000]
    tup = ",".join(f"('{c}','{a}')" for c, a in chunk)
    for c, a, b, w in ch(f"SELECT contract, address, max(block_num), argMax(toString(balance), block_num) FROM {DB} "
                         f"WHERE (contract, address) IN ({tup}) GROUP BY contract, address"):
        cur[(c, a)] = (int(b), int(w))
todo = [(c, a, *cur[(c, a)]) for c, a in pick if (c, a) in cur and cur[(c, a)][0] < S]
cnt = collections.Counter(after_S=sum(1 for k in pick if k in cur and cur[k][0] >= S))
with open(f"{d}/check_rows.tsv", "w") as f, concurrent.futures.ThreadPoolExecutor(16) as ex:
    for (c, a, b, w), v in zip(todo, ex.map(lambda r: balance_of(r[0], r[1], S - 1), todo)):
        k = "no_word" if v is None else "equal" if v == w else "differs"
        cnt[k] += 1
        f.write(f"{c}\t{a}\t{b}\t{w}\t{'' if v is None else v}\t{k}\n")
print(f"{n} sampled pairs: {dict(cnt)} (after_S: the pair has a row at or after S, not checked here)")
