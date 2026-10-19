# Balance quality: methods

How the [native](quality-native.md), [ERC-20](quality-erc20.md) and
[top-token](quality-top-tokens.md) pages compare the new databases with the
RPC-era ones, with the SQL to rerun each comparison. The databases are listed
in [Deployment status](deployment-status.md).

## What is compared

- **New**: `<chain>:evm-balances-native@<version>` and
  `<chain>:evm-balances-erc20@<version>`, written by this repository's
  packages from Firehose Extended blocks, without RPC. Each database is
  backfilled up to a fixed end block. The **cutoff** is the last block before
  that end block.
- **RPC-era**: `<chain>:evm-balances@v0.3.3`, which the Token API serves
  today. It reads a balance by RPC (`eth_getBalance`, `balanceOf`) when it
  sees an event that may change it. It was backfilled up to its live start
  block and is live since then, so it runs past the cutoff.

Neither side is ground truth. The RPC-era database misses a change when it
sees no event for it, and it can lag or skip blocks in its live part. The new
native database follows every persisted balance change, while the new ERC-20
database infers balances and drops a contract's rows for a block when in
doubt ([measured, not qualified](../erc20/balances/README.md#measured)). So
the pages classify each difference and say which side the evidence shows to
be wrong. Where only an RPC call at the block could settle it, they say
"needs RPC". These comparisons made no RPC or Firehose call: running one is a
live step that needs the owner's OK ([AGENTS.md](../AGENTS.md#evidence)).

## Holder comparison at the cutoff

`native_balances` and `erc20_balances` hold one row per address (per contract
and address for ERC-20). The row carries the latest balance and the block of
its last change. Both are `ReplacingMergeTree(block_num)` tables, so they are
read with `FINAL`. A full outer join on the address gives:

| Count | Meaning |
| --- | --- |
| comparable | address in both databases, and the RPC-era row is at or before the cutoff, so both rows give the balance at the cutoff |
| match / mismatch | comparable rows with equal / different balances |
| mismatch, new older | the new row is older than the RPC-era row: the new database missed a later change, or the RPC-era database recorded one that did not happen |
| mismatch, same block | both rows are at the same block, but the balances differ |
| mismatch, new newer | the new row is newer: the RPC-era database missed a change (for example a fee, a reward or a gap in its live sink), or the new database recorded one that did not happen |
| after | address in both, but the RPC-era row is after the cutoff, so this join cannot compare it (the top-holder comparison can) |
| only RPC-era | address only in the RPC-era database, with its row at or before the cutoff |
| only new | address only in the new database |
| value match | the share of the comparable RPC-era balance sum held by matching rows |

Native, sharded by address hash (repeat for `k` in `0..N-1` and add the
counts). Set `<min-block>` to the new database's first block on chains that
start after genesis (Optimism: 105,236,000), and 0 elsewhere:

```sql
SELECT
  countIf(c) AS comparable,
  countIf(c AND nb = ob) AS match,
  countIf(c AND nb != ob AND nblk < oblk) AS mismatch_new_older,
  countIf(c AND nb != ob AND nblk = oblk) AS mismatch_same_block,
  countIf(c AND nb != ob AND nblk > oblk) AS mismatch_new_newer,
  countIf(isNotNull(nb) AND isNotNull(ob) AND oblk > <cutoff>) AS after,
  countIf(isNull(nb) AND oblk <= <cutoff>) AS only_rpc,
  countIf(isNull(nb) AND oblk <= <cutoff> AND ob = 0) AS only_rpc_zero,
  countIf(isNull(ob)) AS only_new,
  countIf(isNull(ob) AND nb = 0) AS only_new_zero,
  sumIf(toFloat64(ob), c AND nb = ob) AS value_matched,
  sumIf(toFloat64(ob), c) AS value_total
FROM
(
  SELECT n.*, o.*,
    isNotNull(nb) AND isNotNull(ob) AND oblk <= <cutoff> AND oblk >= <min-block> AS c
  FROM (SELECT address, balance AS nb, block_num AS nblk
        FROM `<new>`.native_balances FINAL
        WHERE cityHash64(address) % <N> = <k>) AS n
  FULL OUTER JOIN
       (SELECT address, balance AS ob, block_num AS oblk
        FROM `<chain>:evm-balances@v0.3.3`.native_balances FINAL
        WHERE cityHash64(address) % <N> = <k>) AS o
  ON n.address = o.address
)
SETTINGS join_use_nulls = 1, max_execution_time = 1800, max_threads = 8
FORMAT JSONEachRow
```

ERC-20 is the same query on `erc20_balances` with `WHERE contract = '<contract>'`
on both sides. That is cheap, because the table is ordered by
`(contract, address)`. USDT and USDC have tens of millions of rows, so shard
them as well.

## Top holders at the cutoff hour

Rows changed after the cutoff cannot be compared by the join. So the top-n
check reads both databases' hourly history instead: the
`historical_*_balances_state` bars with `interval_min = 60`. The holder set
is the union of both databases' current top n. Each holder's balance is
`argMaxMerge(close)` over the bars before the start of the hour that contains
the cutoff block. A holder ranks by its RPC-era balance (`rank_old`), and
also by its new balance (`rank_new`), which catches phantom holders that
exist only in the new database.

```sql
WITH holders AS (
  SELECT address FROM (SELECT address FROM `<new>`.native_balances FINAL ORDER BY balance DESC LIMIT 1000)
  UNION DISTINCT
  SELECT address FROM (SELECT address FROM `<old>`.native_balances FINAL ORDER BY balance DESC LIMIT 1000)
),
hour AS (SELECT toStartOfHour(max(timestamp)) FROM `<new>`.blocks WHERE block_num <= <cutoff>)
SELECT address, ifNull(nb, 0) AS nb, ifNull(ob, 0) AS ob,
  row_number() OVER (ORDER BY ifNull(ob, 0) DESC) AS rank_old,
  row_number() OVER (ORDER BY ifNull(nb, 0) DESC) AS rank_new
FROM (SELECT address, argMaxMerge(close) AS nb FROM `<new>`.historical_native_balances_state
      WHERE interval_min = 60 AND timestamp < (SELECT * FROM hour) AND address IN holders GROUP BY address) AS n
FULL OUTER JOIN
     (SELECT address, argMaxMerge(close) AS ob FROM `<old>`.historical_native_balances_state
      WHERE interval_min = 60 AND timestamp < (SELECT * FROM hour) AND address IN holders GROUP BY address) AS o
USING address
SETTINGS join_use_nulls = 1, max_execution_time = 900
```

For ERC-20, use `erc20_balances` and `historical_erc20_balances_state`, and
add `AND contract = '<contract>'` everywhere. The same bars, read at earlier
hours, give the historical checks on the [top-token page](quality-top-tokens.md).
The history views keep the intervals the Token API serves: 1h, 4h, 1d and 1w
(`interval_min` 60, 240, 1440 and 10080).

## Supply checks

Some tokens give a total that does not depend on either database:

- **WETH9-style wrapped native tokens** (WETH, WBNB, WPOL, WHYPE):
  `totalSupply()` is the contract's own native balance. So the sum of all
  holder token balances is at most the contract's native balance in the
  native database. The gap is native currency sent to the contract without a
  `Deposit`, through a self-destruct or a block reward, and is usually tiny.
- **USDT and USDC**: the total supply follows from the issue, redeem, mint
  and burn events (and USDT's `DestroyedBlackFunds`) in
  `mainnet:evm-transfers@v0.3.3`, wherever those tables cover the full
  history.

## Running the queries

- The new databases do not change once backfilled, but the RPC-era databases
  do. So rerunning a comparison later moves holders from "comparable" to
  "after", and the counts shift a little.
- The full joins scan both tables completely. On a shared cluster, shard them
  by `cityHash64(address) % N`, as above: 16 to 48 shards for the large
  native tables (Ethereum, BSC, Base, Polygon), and 4 to 8 for USDT and USDC.
  Run few at a time, with `max_threads` and `max_execution_time` set. The
  default limit of 60 seconds is too short for the larger joins.
- A new database's `blocks` table has a row only for blocks with a balance
  change, so its absent blocks are not gaps. Coverage is in
  `backfill_processed_ranges`.
