# Repair the seeded ERC-20 state below the Extended start

`<chain>:evm-balances-erc20@v0.7.4` starts Arbitrum One at Nitro and Optimism
at Bedrock. The balances carried across that start S were seeded on
2026-10-29 from the RPC-era database
([deployment status](../../../../../docs/deployment-status.md#legacy-state-below-the-extended-start)).
The chain check of 2026-11-03
([ERC-20 quality](../../../../../docs/quality-erc20.md#checks-against-the-chain-and-etherscan-2026-11-03))
found stale seeded rows: rows that were right at their own block, whose
balance then changed before S without the RPC era recording it. On Arbitrum
One they include a holder of 608 quadrillion USD₮0 and one of 3.4 trillion
bridged USDC, both holding nothing on chain at S-1. This procedure reads every seeded row from the
chain at block S-1 and corrects the database in place, with the live sink
running. It is the procedure of the
[native Optimism repair](../../../../../native/balances/clickhouse/examples/repair-optimism-pre-bedrock-seed/),
for ERC-20 and both chains.

| Chain | S | S-1 hash | S-1 time (UTC) |
| --- | ---: | --- | --- |
| Arbitrum One | 22,208,000 | `0x8a0ab52be8fc926eef13a154dfb0bb06920c21f7980effd9a42d2159f50c7ce7` | 2022-08-31 16:26:49 |
| Optimism | 105,236,000 | `0x33534e1fcc96457cc297e9e04db4f81245cff6b28399d40fe795e2623b366f47` | 2023-06-06 16:59:35 |

The scripts read `PINAX_KEY` and `RPC_ID` (`arbone` or `optimism`) from the
environment and never print them. Each works in a directory per chain.

## Steps

1. **Export** the rows still current below S, with their block hashes, into
   `<chain-dir>/seeded_rows.tsv`:

   ```sql
   SELECT contract, address, block_num, block_hash, toUnixTimestamp(timestamp) AS ts, toString(balance)
   FROM `arbitrum-one:evm-balances-erc20@v0.7.4`.erc20_balances FINAL
   WHERE block_num < 22208000 ORDER BY contract, address FORMAT TSV
   ```

2. **Read the chain** for every pair with `balanceOf`:
   `fetch.py <chain-dir> <block> <shard> <shards> [workers]`, one process
   per shard, resumable. At block S-1 it writes `at_S1.part<shard>.tsv`.
   On Arbitrum One, reads at block 22,207,999 slowed from about 400 pairs a
   second to about 50 after 29% of the pairs, so the rest were read at the
   head block instead (`OUT_PREFIX=at_H`, block 521,009,827, about 4,000 a
   second): a seeded row still current below
   S that equals the chain at head is right, since the package wrote nothing
   for it after S. `compare.py` reads the pairs that differ at head at S-1.
3. **Build the corrected rows**:
   `compare.py <chain-dir> <S> <S-1 hash> <S-1 unix time>`. Rows equal to
   the chain at S-1 are left alone, and so are rows whose call returned no
   word. For the others it reads the chain at the row's own block: if the
   final value was already there, the row keeps its block, hash and
   timestamp (`receipt_block`); otherwise it moves to S-1 (`boundary`). A
   token with more than `ROW_READ_MAX_PER_TOKEN` differing rows (default
   1,000) has a computed balance, such as Optimism's legacy ETH or an Aave
   aToken, whose `balanceOf` moves without a stored change; its rows move to
   S-1 without the second read (`boundary_computed`). `ROW_READ=0` skips the
   second reads not done yet and moves those rows to S-1. A row right at S-1
   that differs at head is a change after S that the package missed; it is
   listed in `changed_after_start.tsv` and not corrected here. Output:
   `corrected_rows.tsv` (contract, address, block, hash, timestamp, balance,
   old balance, case).
4. **Check the file** before writing: `spot.py <chain-dir> <n>` reads a
   random sample of corrected rows, plus three of each case, at their
   blocks.
5. **Apply** with admin credentials, from the chain directory, one step at a
   time:
   `CHAIN=<chain> CH_URL=... CH_USER=... CH_PASSWORD=... ../apply.sh create|check|bars|rows|verify|drop`.
   `create` loads the rows into a replicated staging table created
   `ON CLUSTER`. `check` shows the counts; every staging pair should still
   be current below S. `bars` deletes the history bars in the corrected
   rows' windows below the hour after S-1 with one mutation on the
   replicated state table (`allow_nondeterministic_mutations=1`, since the
   subquery reads the replicated staging table, after checking that both
   replicas hold the same staging rows; `mutations_sync=2`). `rows` inserts
   the corrected rows into `erc20_balances`, and the existing materialized
   view rebuilds their 1h, 4h, 1d and 1w bars. A corrected row carries the
   seeded row's block or S-1, so it supersedes the seeded row in the
   `ReplacingMergeTree`. A pair the live sink writes meanwhile holds a
   newer row and is untouched.
6. **Check the database**: `check_rows.py <chain-dir> <S> <n>` (with
   `CHAIN`, `RPC_ID` and read-only ClickHouse access) samples seeded pairs,
   reads their current rows, and compares each row still current below S
   with `balanceOf` at S-1. Run it before and after the apply.

## The 2026-11-03 run

| | Arbitrum One | Optimism |
| --- | ---: | ---: |
| Seeded rows below S | 2,275,584 | 11,985,028 |
| Equal to the chain at S-1 | 2,196,906 | 11,540,650 |
| No word returned | 122 | 20,368 |
| Right at S-1, changed after S (not corrected) | 15,729 | 0 |
| Corrected, non-zero before | 61,164 | 423,833 |
| Corrected, zero before | 1,663 | 177 |
| Corrected rows (`receipt_block` / `boundary` / `boundary_computed`) | 17 / 62,810 / 0 | 0 / 7,211 / 416,799 |
| Tokens with corrections | 294 | 274 |
| Corrected rows read back at their block (`spot.py`) | 31 of 31 equal | 31 of 31 equal |
| Sampled seeded pairs wrong at S-1, before the apply (`check_rows.py`) | 29 of 1,000 | 70 of 2,000 (4 no word) |

On Arbitrum One, the equal rows include 1,554,729 read only at head. Of
18,308 rows read again at their own block, 17 already held the final
value; the other 44,519 second reads were skipped (`ROW_READ=0`) and those
rows moved to S-1, which is right for their balance and differs only in
when the history shows the change. The 15,729 rows changed after S are in
152 tokens, mostly interest-bearing and rebasing tokens such as Aave v3
aTokens. On Optimism, the 416,799 computed rows are in 23 tokens, led by
the legacy ETH token `0xdeaddeaddeaddeaddeaddeaddeaddeaddead0000`
(150,262 rows).
