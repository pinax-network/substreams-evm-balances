#!/usr/bin/env python3
"""Check an evm-balances-erc20 database against the chain: precision at each row's block, freshness at a fixed
head block, coverage of independently sampled holders, supply of the top tokens, and (free-tier chains) Etherscan.

usage: erc20check.py <chain> <out_dir> [sample|check|coverage|supply|etherscan|all]

Environment (never printed):
  PINAX_KEY           Pinax RPC key (https://<rpc-id>.rpc.pinax.network/v1/<key>/), or RPC_URL
  ETHERSCAN_API_KEY   Etherscan v2 (etherscan step only)
  CH_URL              ClickHouse HTTP URL; credentials from CH_CURL_CONFIG (a curl -K file) or CH_USER/CH_PASSWORD
  ERC20_DB, TRANSFERS_DB  override the database names
"""
import concurrent.futures, json, os, random, ssl, subprocess, sys, time, urllib.error, urllib.parse, urllib.request

CHAINS = {  # chain: (pinax rpc id, etherscan chain id on the free tier or None)
    "mainnet": ("eth", 1), "bsc": ("bsc", None), "base": ("base", None), "arbitrum-one": ("arbone", 42161),
    "optimism": ("optimism", None), "polygon": ("polygon", 137), "unichain": ("unichain", 130), "hyperevm": ("hyperevm", 999),
}
SLICE = ("0x1a", "0x1b")          # 1/256 of addresses, away from vanity prefixes
BANDS = [("zero", None, 0), ("lt1e6", 1, 10**6), ("1e6-1e12", 10**6, 10**12), ("1e12-1e18", 10**12, 10**18), ("ge1e18", 10**18, None)]
PER_BAND, RECENT, TOP_TOKENS, TOP_HOLDERS = 900, 900, 20, 1000
COV_RANDOM, COV_TOP = 3000, 2000
ES_PAIRS = 1000
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem")
SEL_BALANCE, SEL_SUPPLY, SEL_SYMBOL = "0x70a08231", "0x18160ddd", "0x95d89b41"

chain = sys.argv[1]; OUT = sys.argv[2]; STEP = sys.argv[3] if len(sys.argv) > 3 else "all"
RPC_ID, ES_CHAIN = CHAINS[chain]
KEY = os.environ.get("PINAX_KEY") or os.environ.get("SUBSTREAMS_API_KEY", "")
RPC = os.environ.get("RPC_URL") or f"https://{RPC_ID}.rpc.pinax.network/v1/{KEY}/"
ES_KEY = os.environ.get("ETHERSCAN_API_KEY", "")
DB = os.environ.get("ERC20_DB", f"{chain}:evm-balances-erc20@v0.7.4")
TDB = os.environ.get("TRANSFERS_DB", f"{chain}:evm-transfers@v0.3.3")
T_BAL, T_BLOCKS, T_CURSORS, T_TRANSFERS = f"`{DB}`.erc20_balances", f"`{DB}`.blocks", f"`{DB}`.cursors", f"`{TDB}`.transfers"
os.makedirs(OUT, exist_ok=True)


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, flush=True)


def redact(s):
    for k in (KEY, ES_KEY):
        if k:
            s = s.replace(k, "<key>")
    return s


# ---------------------------------------------------------------- ClickHouse
def ch(sql, timeout=1800):
    """Run a read-only query, return rows as lists of strings (TSV)."""
    url = os.environ.get("CH_URL", "http://127.0.0.1:8123") + "/?readonly=2&max_execution_time=" + str(timeout)
    cmd = ["curl", "-sS", "--fail-with-body", "--max-time", str(timeout + 30), url, "--data-binary", "@-"]
    if os.environ.get("CH_CURL_CONFIG"):
        cmd[1:1] = ["-K", os.environ["CH_CURL_CONFIG"]]
    else:
        cmd[1:1] = ["-u", f"{os.environ.get('CH_USER', 'default')}:{os.environ.get('CH_PASSWORD', '')}"]
    r = subprocess.run(cmd, input=(sql + "\nFORMAT TSV").encode(), capture_output=True, timeout=timeout + 60)
    if r.returncode != 0:
        raise SystemExit("clickhouse: " + (r.stdout + r.stderr).decode(errors="replace")[:600] + "\nquery: " + sql[:400])
    return [l.split("\t") for l in r.stdout.decode().splitlines() if l]


def ch_pairs(pairs, extra_cols=""):
    """Current FINAL rows for (contract, address) pairs: {(c, a): (block, wei)}."""
    out = {}
    pairs = list(pairs)
    for i in range(0, len(pairs), 2000):
        chunk = pairs[i:i + 2000]
        tup = ",".join(f"('{c}','{a}')" for c, a in chunk)
        contracts = ",".join(sorted({f"'{c}'" for c, _ in chunk}))
        rows = ch(f"SELECT contract, address, max(block_num), argMax(toString(balance), block_num) FROM {T_BAL} "
                  f"WHERE contract IN ({contracts}) AND (contract, address) IN ({tup}) GROUP BY contract, address")
        for c, a, b, w in rows:
            out[(c, a)] = (int(b), int(w))
    return out


def cursor_block():
    rows = ch(f"SELECT max(block_num) FROM {T_CURSORS} FINAL")
    return int(rows[0][0])


# ---------------------------------------------------------------- RPC
def post(url, payload, tries=8):
    data = json.dumps(payload).encode()
    for attempt in range(tries):
        try:
            req = urllib.request.Request(url, data=data, headers={"content-type": "application/json"})
            with urllib.request.urlopen(req, timeout=120, context=CTX) as r:
                return json.loads(r.read())
        except urllib.error.HTTPError as e:
            if attempt == tries - 1:
                raise SystemExit(redact(f"rpc HTTP {e.code}"))
        except Exception as e:  # noqa: BLE001
            if attempt == tries - 1:
                raise SystemExit(redact(f"rpc: {e}"))
        time.sleep(min(60, 3 * (attempt + 1)))


def calls(reqs, batch=50, workers=8):
    """reqs: list of (to, data, block_tag). Returns list of int|None (None = call failed or empty)."""
    chunks = [reqs[i:i + batch] for i in range(0, len(reqs), batch)]

    def one(chunk):
        payload = [{"jsonrpc": "2.0", "id": k, "method": "eth_call", "params": [{"to": to, "data": d}, tag]}
                   for k, (to, d, tag) in enumerate(chunk)]
        res = post(RPC, payload)
        if isinstance(res, dict):  # whole-batch error
            return [None] * len(chunk)
        by = {r.get("id"): r for r in res}
        out = []
        for k in range(len(chunk)):
            r = by.get(k, {})
            v = r.get("result")
            out.append(int(v[:66], 16) if isinstance(v, str) and len(v) >= 66 else None)
        return out

    res = []
    with concurrent.futures.ThreadPoolExecutor(workers) as ex:
        for got in ex.map(one, chunks):
            res += got
    return res


def bal_data(addr):
    return SEL_BALANCE + "0" * 24 + addr[2:].lower()


def symbols(contracts):
    payload = [{"jsonrpc": "2.0", "id": k, "method": "eth_call", "params": [{"to": c, "data": SEL_SYMBOL}, "latest"]}
               for k, c in enumerate(contracts)]
    res = post(RPC, payload) if payload else []
    by = {r.get("id"): r for r in res} if isinstance(res, list) else {}
    out = {}
    for k, c in enumerate(contracts):
        v = (by.get(k) or {}).get("result") or ""
        s = ""
        try:
            raw = bytes.fromhex(v[2:])
            if len(raw) >= 96:
                n = int.from_bytes(raw[32:64], "big"); s = raw[64:64 + n].decode("utf-8", "replace")
            elif len(raw) == 32:
                s = raw.rstrip(b"\0").decode("utf-8", "replace")
        except Exception:  # noqa: BLE001
            pass
        out[c] = s.strip().replace("\t", " ")[:24] or c[:10]
    return out


def head_block():
    r = post(RPC, {"jsonrpc": "2.0", "id": 1, "method": "eth_blockNumber", "params": []})
    return int(r["result"], 16)


def write_tsv(path, header, rows):
    with open(path, "w") as f:
        f.write("\t".join(header) + "\n")
        for r in rows:
            f.write("\t".join(str(x) for x in r) + "\n")


def read_tsv(path):
    lines = [l.rstrip("\n").split("\t") for l in open(path)]
    return lines[0], lines[1:]


# ---------------------------------------------------------------- steps
def step_sample():
    log(f"{chain}: top tokens by distinct senders in the last 24 h ({TDB})")
    cand = ch(f"SELECT log_address, uniqExact(`from`) AS senders FROM {T_TRANSFERS} "
              f"WHERE minute >= toRelativeMinuteNum(now()) - 1440 AND transfer_type = 'transfer' "
              f"GROUP BY log_address ORDER BY senders DESC LIMIT {TOP_TOKENS * 5}")
    lst = ",".join(f"'{c}'" for c, _ in cand)
    held = {c: int(n) for c, n in ch(f"SELECT contract, countIf(balance > 0) FROM {T_BAL} WHERE contract IN ({lst}) GROUP BY contract")}
    sym = symbols([c for c, _ in cand])
    # Address-poisoning tokens top the sender ranking with fake transfers and no balances: keep the first TOP_TOKENS
    # candidates the database holds rows for, and record the skipped ones with their on-chain supply, so that a
    # real token missing from the database still shows.
    top = [(c, s) for c, s in cand if held.get(c, 0) > 0][:TOP_TOKENS]
    last = int(top[-1][1]) if top else 0
    skipped = [(c, s) for c, s in cand if held.get(c, 0) == 0 and int(s) >= last]
    sup = calls([(c, SEL_SUPPLY, "latest") for c, _ in skipped], batch=50, workers=2) if skipped else []
    write_tsv(f"{OUT}/top_tokens_skipped.tsv", ["contract", "symbol", "senders_24h", "total_supply_latest"],
              [(c, sym[c], s, "" if v is None else v) for (c, s), v in zip(skipped, sup)])
    tops = [c for c, _ in top]
    write_tsv(f"{OUT}/top_tokens.tsv", ["contract", "symbol", "senders_24h"], [(c, sym[c], s) for (c, s) in top])
    log(f"  {len(top)} tokens kept, {len(skipped)} higher-ranked candidates skipped (no rows in the database)")
    rows = []
    for c in tops:
        got = ch(f"SELECT contract, address, block_num, toString(balance) FROM {T_BAL} FINAL WHERE contract = '{c}' AND balance > 0 "
                 f"ORDER BY balance DESC LIMIT {TOP_HOLDERS}", timeout=900)
        rows += [g + [f"top:{sym[c]}"] for g in got]
        log(f"  top holders {sym[c]:12} {len(got)}")
    log(f"{chain}: random pairs from the address slice {SLICE[0]}..{SLICE[1]}")
    base = (f"SELECT contract, address, max(block_num) AS b, argMax(balance, block_num) AS v FROM {T_BAL} "
            f"WHERE address >= '{SLICE[0]}' AND address < '{SLICE[1]}' GROUP BY contract, address")
    for name, lo, hi in BANDS:
        cond = "v = 0" if name == "zero" else " AND ".join(x for x in (f"v >= toUInt256('{lo}')" if lo else "", f"v < toUInt256('{hi}')" if hi else "") if x)
        got = ch(f"SELECT contract, address, b, toString(v) FROM ({base}) WHERE {cond} ORDER BY cityHash64(contract, address) LIMIT {PER_BAND}")
        rows += [g + [f"rand:{name}"] for g in got]
        log(f"  band {name:10} {len(got)}")
    hb = int(ch(f"SELECT max(block_num) FROM {T_BLOCKS}")[0][0])
    recent_from = int(ch(f"SELECT min(block_num) FROM {T_BLOCKS} WHERE timestamp >= now() - INTERVAL 1 DAY")[0][0])
    got = ch(f"SELECT contract, address, b, toString(v) FROM ({base}) WHERE b >= {recent_from} ORDER BY cityHash64(contract, address) LIMIT {RECENT}")
    rows += [g + ["recent"] for g in got]
    log(f"  recent (last 24 h, from block {recent_from}) {len(got)}; db head {hb}")
    write_tsv(f"{OUT}/sample.tsv", ["contract", "address", "block_num", "balance_wei", "stratum"], rows)
    log(f"sample.tsv: {len(rows)} rows")


def step_check():
    _, rows = read_tsv(f"{OUT}/sample.tsv")
    log(f"{chain}: precision, balanceOf at each row's block for {len(rows)} rows")
    at_row = calls([(c, bal_data(a), hex(int(b))) for c, a, b, _, _ in rows])
    H = cursor_block()
    cur = ch_pairs({(c, a) for c, a, _, _, _ in rows})
    log(f"freshness at block {H} (db cursor)")
    at_h = calls([(c, bal_data(a), hex(H)) for c, a, _, _, _ in rows])
    out = []
    for (c, a, b, w, s), vr, vh in zip(rows, at_row, at_h):
        w = int(w)
        prec = "unknown" if vr is None else ("equal" if vr == w else "mismatch")
        cb, cw = cur.get((c, a), (None, None))
        if cb is None:
            fresh = "row_gone"
        elif cb > H:
            fresh = "moved_after_H"
        elif vh is None:
            fresh = "unknown"
        else:
            fresh = "equal" if vh == cw else "differs"
        out.append((c, a, s, b, w, "" if vr is None else vr, prec, H, "" if cb is None else cb, "" if cw is None else cw, "" if vh is None else vh, fresh))
    write_tsv(f"{OUT}/check.tsv", ["contract", "address", "stratum", "row_block", "row_wei", "chain_at_row", "precision",
                                   "H", "current_block", "current_wei", "chain_at_H", "freshness"], out)
    summarize_check(out)


def summarize_check(out):
    import collections
    by = collections.defaultdict(collections.Counter)
    for r in out:
        st = r[2] if not r[2].startswith("top:") else "top"
        by[st]["rows"] += 1; by[st]["p_" + r[6]] += 1; by[st]["f_" + r[11]] += 1
    log("stratum       rows  prec_equal  prec_mismatch  prec_unknown | fresh_equal  differs  moved  unknown")
    for st, c in sorted(by.items()):
        log(f"{st:12} {c['rows']:5} {c['p_equal']:11} {c['p_mismatch']:14} {c['p_unknown']:13} | {c['f_equal']:11} {c['f_differs']:8} {c['f_moved_after_H']:6} {c['f_unknown']:8}")


def step_coverage():
    log(f"{chain}: coverage, recipients of ERC-20 transfers in the last 24 h ({TDB})")
    _, tt = read_tsv(f"{OUT}/top_tokens.tsv")
    tops = ",".join(f"'{r[0]}'" for r in tt)
    base = (f"SELECT DISTINCT log_address AS contract, to AS address FROM {T_TRANSFERS} WHERE minute >= toRelativeMinuteNum(now()) - 1440 "
            f"AND transfer_type IN ('transfer', 'mint', 'deposit', 'issue') AND to != '0x0000000000000000000000000000000000000000'")
    rnd = ch(f"SELECT contract, address FROM ({base}) ORDER BY cityHash64(contract, address) LIMIT {COV_RANDOM}")
    top = ch(f"SELECT contract, address FROM ({base} AND log_address IN ({tops})) ORDER BY cityHash64(contract, address) LIMIT {COV_TOP}")
    pairs = [(c, a, "random") for c, a in rnd] + [(c, a, "top") for c, a in top]
    H = cursor_block()
    cur = ch_pairs({(c, a) for c, a, _ in pairs})
    vals = calls([(c, bal_data(a), hex(H)) for c, a, _ in pairs])
    # rows newer than H: settle at their own block
    later = [(i, c, a, cur[(c, a)]) for i, (c, a, _) in enumerate(pairs) if (c, a) in cur and cur[(c, a)][0] > H]
    later_v = calls([(c, bal_data(a), hex(b)) for _, c, a, (b, _) in later]) if later else []
    later_map = {i: v for (i, _, _, _), v in zip(later, later_v)}
    out = []
    for i, ((c, a, kind), v) in enumerate(zip(pairs, vals)):
        row = cur.get((c, a))
        if v is None:
            verdict = "unknown"
        elif row and row[0] > H:
            lv = later_map.get(i)
            verdict = "served_equal" if lv == row[1] else ("served_differs" if lv is not None else "unknown")
        elif row is None:
            verdict = "empty_no_row" if v == 0 else "missing"
        elif row[1] == v:
            verdict = "served_equal" if v > 0 else "empty_zero_row"
        else:
            verdict = "served_differs"
        out.append((c, a, kind, H, "" if v is None else v, "" if row is None else row[0], "" if row is None else row[1], verdict))
    write_tsv(f"{OUT}/coverage.tsv", ["contract", "address", "kind", "H", "chain_at_H", "row_block", "row_wei", "verdict"], out)
    import collections
    for kind in ("random", "top"):
        c = collections.Counter(r[7] for r in out if r[2] == kind)
        held = sum(v for k, v in c.items() if k in ("served_equal", "missing", "served_differs"))
        log(f"{kind:7} pairs {sum(c.values())}: {dict(c)}; holders with a balance on chain {held}, served exactly {c['served_equal']}")


def step_supply():
    _, tt = read_tsv(f"{OUT}/top_tokens.tsv")
    log(f"{chain}: supply of the {len(tt)} top tokens")
    out = []
    for c, sym, _ in tt:
        res = None
        for attempt in range(4):
            b0 = cursor_block()
            s = ch(f"SELECT toString(sum(balance)), count() FROM {T_BAL} FINAL WHERE contract = '{c}'", timeout=900)
            b1 = cursor_block()
            ts = calls([(c, SEL_SUPPLY, hex(b0)), (c, SEL_SUPPLY, hex(b1))], batch=2, workers=1)
            res = (b0, b1, int(s[0][0]), int(s[0][1]), ts[0], ts[1])
            if ts[0] is not None and ts[0] == ts[1]:
                break
        b0, b1, total, holders, t0, t1 = res
        if t0 is None:
            verdict, diff = "no_totalSupply", ""
        elif t0 == t1:
            diff = total - t0; verdict = "equal" if diff == 0 else "differs"
        else:
            diff = total - t1; verdict = "supply_moving"
        out.append((c, sym, holders, b0, b1, total, "" if t0 is None else t0, "" if t1 is None else t1, diff,
                    "" if (t0 in (None, 0) or diff == "") else f"{diff / t0:.3e}", verdict))
        log(f"  {sym:12} holders {holders:>9} sum-supply {diff} ({out[-1][9]}) {verdict}")
    write_tsv(f"{OUT}/supply.tsv", ["contract", "symbol", "rows", "cursor_before", "cursor_after", "sum_balances",
                                   "supply_before", "supply_after", "sum_minus_supply", "relative", "verdict"], out)


def step_etherscan():
    if not ES_CHAIN:
        log(f"{chain}: Etherscan free tier does not serve this chain; skipped"); return
    if not ES_KEY:
        raise SystemExit("ETHERSCAN_API_KEY is not set")
    _, rows = read_tsv(f"{OUT}/sample.tsv")
    random.seed(7)
    top = [r for r in rows if r[4].startswith("top:")]; rnd = [r for r in rows if r[4].startswith("rand:") and r[4] != "rand:zero"]
    pick = random.sample(top, min(len(top), ES_PAIRS // 2)) + random.sample(rnd, min(len(rnd), ES_PAIRS // 2))
    log(f"{chain}: Etherscan tokenbalance for {len(pick)} pairs (chain id {ES_CHAIN})")
    es = []
    for c, a, *_ in pick:
        q = urllib.parse.urlencode({"chainid": ES_CHAIN, "module": "account", "action": "tokenbalance", "contractaddress": c,
                                    "address": a, "tag": "latest", "apikey": ES_KEY})
        val = None
        for attempt in range(8):
            try:
                with urllib.request.urlopen("https://api.etherscan.io/v2/api?" + q, timeout=60, context=CTX) as r:
                    d = json.loads(r.read())
            except Exception:  # noqa: BLE001
                time.sleep(2 * (attempt + 1)); continue
            if d.get("status") == "1":
                val = int(d["result"]); break
            msg = f"{d.get('message')} {d.get('result')}".lower()
            if "rate limit" in msg or "max calls" in msg or "busy" in msg:
                time.sleep(1.5 * (attempt + 1)); continue
            break
        es.append(val)
        time.sleep(0.22)
    node = calls([(c, bal_data(a), "latest") for c, a, *_ in pick])
    cur = ch_pairs({(r[0], r[1]) for r in pick})
    out = []
    import collections
    cnt = collections.Counter()
    for (c, a, *_), e, n in zip(pick, es, node):
        row = cur.get((c, a))
        v_node = "es_unavailable" if e is None else ("es_eq_node" if e == n else "es_ne_node")
        v_db = "no_row" if row is None else ("db_eq_es" if e is not None and row[1] == e else "db_ne_es")
        cnt[v_node] += 1; cnt[v_db] += 1
        out.append((c, a, "" if e is None else e, "" if n is None else n, "" if row is None else row[0], "" if row is None else row[1], v_node, v_db))
    write_tsv(f"{OUT}/etherscan.tsv", ["contract", "address", "etherscan_latest", "node_latest", "row_block", "row_wei", "etherscan_vs_node", "db_vs_etherscan"], out)
    log(f"etherscan: {dict(cnt)}")


steps = {"sample": step_sample, "check": step_check, "coverage": step_coverage, "supply": step_supply, "etherscan": step_etherscan}
for name in (["sample", "check", "coverage", "supply", "etherscan"] if STEP == "all" else [STEP]):
    steps[name]()
