#!/usr/bin/env python3
"""Compare sampled native balance rows of a database with Etherscan, then settle every difference
block-exactly with eth_getBalance; or check every row block-exactly (--rpc-only).

Input: sample.tsv (address, block_num, balance_wei, stratum), see sample.sh.

Pass 1 (Etherscan, latest): account/balancemulti, 20 addresses per call, <= 4 calls/s.
  equal      -> Etherscan's current balance equals the sampled row.
  differs    -> the address changed after the sample, or one side is wrong: go to pass 2.
Pass 2 (settle a difference): re-read the row from ClickHouse (FINAL) and query the chain at that
  row's block with eth_getBalance (Pinax RPC, archive). Classification:
  changed_since   -> the chain at the row's block equals the row (Etherscan simply saw a later block).
  mismatch        -> the chain at the row's block differs from the row: the new database is wrong there.

Environment (never printed): ETHERSCAN_API_KEY, PINAX_KEY (Pinax RPC; or RPC_URL), CH_URL, CH_USER, CH_PASSWORD
(ClickHouse, read-only), DB_NAME (database holding native_balances), RPC_ID (eth, bsc, polygon, arbone, base, optimism,
unichain, hyperevm), ES_CHAINID (Etherscan v2 chain id), RPC_BATCH (addresses per eth_getBalance batch, default 50).
Usage: compare.py sample.tsv out_dir [--rpc-only]   (--rpc-only skips Etherscan and checks every row at its block)
"""
import json, os, ssl, sys, time, urllib.error, urllib.parse, urllib.request

ES_KEY = os.environ.get("ETHERSCAN_API_KEY", "")
RPC_KEY = (os.environ.get("PINAX_KEY") or os.environ.get("SUBSTREAMS_API_KEY", ""))
RPC_ID = os.environ.get("RPC_ID", "eth")
ES_CHAINID = os.environ.get("ES_CHAINID", "1")
RPC_URL = os.environ.get("RPC_URL") or f"https://{RPC_ID}.rpc.pinax.network/v1/{RPC_KEY}/"
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")
CH_URL = os.environ.get("CH_URL", "http://127.0.0.1:8123")
CH_USER = os.environ.get("CH_USER", "default")
CH_PASSWORD = os.environ.get("CH_PASSWORD", "")
DB = "`" + os.environ.get("DB_NAME", "mainnet:evm-balances-native@v0.7.11") + "`.native_balances"


def redact(s):
    for k in (ES_KEY, RPC_KEY):
        if k:
            s = s.replace(k, "<key>")
    return s


def http(url, data=None, headers=None):
    req = urllib.request.Request(url, data=data, headers=headers or {})
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=60, context=CTX) as r:
                return json.loads(r.read())
        except urllib.error.HTTPError as e:
            if e.code in (429, 500, 502, 503, 504) and attempt < 4:
                time.sleep(5 * (attempt + 1)); continue
            raise SystemExit(redact(f"request failed: HTTP {e.code}"))
        except Exception as e:  # noqa: BLE001
            if attempt == 4:
                raise SystemExit(redact(f"request failed: {e}"))
            time.sleep(2 * (attempt + 1))


def etherscan_balances(addresses):
    """Latest balances for up to 20 addresses: {address: wei}."""
    q = urllib.parse.urlencode({"chainid": ES_CHAINID, "module": "account", "action": "balancemulti", "address": ",".join(addresses), "tag": "latest", "apikey": ES_KEY})
    for attempt in range(8):
        r = http("https://api.etherscan.io/v2/api?" + q)
        if r.get("status") == "1":
            return {x["account"].lower(): int(x["balance"]) for x in r["result"]}
        msg = f"{r.get('message')} {r.get('result')}"
        if "rate limit" in msg.lower() or "429" in msg or "busy" in msg.lower():
            time.sleep(1.5 * (attempt + 1))
            continue
        raise SystemExit(redact(f"etherscan: {msg[:200]}"))
    raise SystemExit("etherscan: rate limited 8 times in a row")


def rpc_balances(rows):
    """eth_getBalance at each row's block: {(address, block): wei}."""
    out = {}
    step = int(os.environ.get("RPC_BATCH", "50"))
    for i in range(0, len(rows), step):
        chunk = rows[i:i + step]
        payload = [{"jsonrpc": "2.0", "id": k, "method": "eth_getBalance", "params": [a, hex(b)]} for k, (a, b) in enumerate(chunk)]
        res = http(RPC_URL, data=json.dumps(payload).encode(), headers={"content-type": "application/json"})
        by = {r["id"]: r for r in res}
        for k, (a, b) in enumerate(chunk):
            r = by.get(k, {})
            out[(a, b)] = int(r["result"], 16) if "result" in r else None
    return out


def ch_rows(addresses):
    """Current FINAL rows of the database for these addresses: {address: (block, wei)}."""
    lst = ",".join("'" + a + "'" for a in addresses)
    sql = f"SELECT address, block_num, toString(balance) FROM {DB} FINAL WHERE address IN ({lst}) FORMAT TSV"
    req = urllib.request.Request(CH_URL + "/?readonly=2", data=sql.encode(), headers={"X-ClickHouse-User": CH_USER, "X-ClickHouse-Key": CH_PASSWORD})
    with urllib.request.urlopen(req, timeout=600) as r:
        out = r.read().decode()
    res = {}
    for line in out.splitlines():
        a, b, w = line.split("\t")
        res[a] = (int(b), int(w))
    return res


def main():
    sample_path, out_dir = sys.argv[1], sys.argv[2]
    rpc_only = "--rpc-only" in sys.argv
    os.makedirs(out_dir, exist_ok=True)
    rows = []
    for line in open(sample_path):
        p = line.rstrip("\n").split("\t")
        if p[0] == "address":
            continue
        rows.append((p[0].lower(), int(p[1]), int(p[2]), p[3]))
    print(f"{len(rows)} sampled rows")

    differs = rows
    if not rpc_only:
        if not ES_KEY:
            raise SystemExit("ETHERSCAN_API_KEY is not set")
        equal, differs, latest = 0, [], {}
        t0 = time.time()
        for i in range(0, len(rows), 20):
            chunk = rows[i:i + 20]
            bal = etherscan_balances([r[0] for r in chunk])
            for r in chunk:
                v = bal.get(r[0])
                latest[r[0]] = v
                if v == r[2]:
                    equal += 1
                else:
                    differs.append(r)
            time.sleep(0.34)
        with open(os.path.join(out_dir, "etherscan_latest.tsv"), "w") as f:
            f.write("address\tblock_num\tdb_wei\tstratum\tetherscan_latest_wei\n")
            for r in rows:
                f.write(f"{r[0]}\t{r[1]}\t{r[2]}\t{r[3]}\t{latest.get(r[0])}\n")
        print(f"pass 1 (Etherscan latest): equal {equal}, differs {len(differs)}")
        if not differs:
            return

    # pass 2: settle each remaining row at its block
    current = ch_rows([r[0] for r in differs])
    rpc_rows = [(a, current.get(a, (b, None))[0]) for a, b, _, _ in differs]
    chain = rpc_balances(rpc_rows)
    changed, mismatch, unknown = 0, [], 0
    with open(os.path.join(out_dir, "settled.tsv"), "w") as f:
        f.write("address\tstratum\tsample_block\tsample_wei\tcurrent_block\tcurrent_wei\tchain_wei_at_current_block\tverdict\n")
        for a, b, w, s in differs:
            cb, cw = current.get(a, (b, None))
            cv = chain.get((a, cb))
            if cv is None or cw is None:
                verdict = "unknown"; unknown += 1
            elif cv == cw:
                verdict = "changed_since" if not rpc_only else "equal_at_block"; changed += 1
            else:
                verdict = "mismatch"; mismatch.append((a, s, cb, cw, cv))
            f.write(f"{a}\t{s}\t{b}\t{w}\t{cb}\t{cw}\t{cv}\t{verdict}\n")
    label = "equal at block" if rpc_only else "changed since sample"
    print(f"pass 2 (chain at the row's block): {label} {changed}, mismatch {len(mismatch)}, unknown {unknown}")
    for a, s, cb, cw, cv in mismatch[:20]:
        print(f"  MISMATCH {a} [{s}] block {cb}: db {cw / 1e18:.6f} chain {cv / 1e18:.6f} diff {(cv - cw) / 1e18:+.6f}")


if __name__ == "__main__":
    main()
