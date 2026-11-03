# Repair the seeded pre-Bedrock state of Optimism

`optimism:evm-balances-native@v0.7.11` starts at Bedrock, block 105,236,000.
The balances carried across Bedrock were seeded on 2026-10-29 from the
RPC-era database ([deployment status](../../../../../docs/deployment-status.md#legacy-state-below-the-extended-start)).
A block-exact check against the chain on 2026-11-03 found that most of the
seed's zero rows belonged to addresses holding dust: the RPC era had read
their receipts as 0. This procedure read every seeded row from the chain and
corrected the database in place, with the live sink running. Record:
[`optimism-pre-bedrock-seed-fix-2026-11-03.json`](../../../docs/evidence/optimism-pre-bedrock-seed-fix-2026-11-03.json).

## Steps

1. **Export** the rows still current below Bedrock, with their block hashes:

   ```sql
   SELECT address, block_num, block_hash, toUnixTimestamp(timestamp) AS ts, toString(balance) AS balance_wei
   FROM `optimism:evm-balances-native@v0.7.11`.native_balances FINAL
   WHERE block_num < 105236000 ORDER BY address FORMAT TSVWithNames
   ```

   into `seeded_rows_hash.tsv` (and the same without `block_hash` into
   `seeded_rows.tsv`, which `fetch_s1.py` reads).
2. **Read the chain at the last pre-Bedrock block**, 105,235,999, for every
   address: `python3 fetch_s1.py <shard> <shards>`, one process per shard,
   resumable; output `chain_at_S1.part<shard>.tsv`. With `PINAX_KEY` set,
   8 shards read about 2,300 addresses per second.
3. **Build the corrected rows**: `python3 compare_s1.py`. Rows equal to the
   chain at 105,235,999 are left alone. For the others it reads the chain at
   the row's own block: if the final value was already there, the row keeps
   its block, hash and timestamp (`receipt_block`); otherwise the balance
   changed before Bedrock and the row moves to 105,235,999
   (`bedrock_boundary`). Output `corrected_rows.tsv`.
4. **Apply** with admin credentials, one step at a time:
   `CH_URL=... CH_USER=... CH_PASSWORD=... ./apply.sh create|check|bars|rows|verify|drop`.
   `create` loads the rows into a replicated staging table created
   `ON CLUSTER`; `bars` deletes the seeded history bars in the corrected
   rows' windows with one mutation on the replicated state table
   (`allow_nondeterministic_mutations=1`, since the subquery reads the
   replicated staging table, after checking that both replicas hold the same
   staging rows; `mutations_sync=2`); `rows` inserts the corrected rows into
   `native_balances`, and the existing materialized view rebuilds their 1h,
   4h, 1d and 1w bars. A corrected row carries the seeded row's block, so it
   supersedes it in the `ReplacingMergeTree`, the later insert winning among
   equal versions. Addresses that changed after Bedrock already hold a newer
   row and are untouched.
5. **Second pass** for zero rows whose address held a balance at the row's
   block but 0 at Bedrock (the RPC era recorded the receipt as 0 and missed
   the spend): read the chain at each such row's block, then write two rows
   per address, the receipt block with the chain's value and 105,235,999
   with 0, through the same `apply.sh` from a directory holding that
   `corrected_rows.tsv`.
6. **Check** with [`scripts/check-db-native-balances`](../../../../../scripts/check-db-native-balances/)
   on a fresh sample of rows below Bedrock.

## The 2026-11-03 run

| | |
| --- | ---: |
| Seeded rows below Bedrock | 1,349,169 |
| Equal to the chain at 105,235,999 | 613,061 |
| Corrected in pass 1 (726,758 kept their block, 9,350 moved to the boundary) | 736,108 |
| Corrected in pass 2 | 2,035 |
| Zero rows below Bedrock, before and after | 938,786 and 208,023 |
| Balance restored | 645.5 ETH |
| History bars rebuilt | 2,915,208 |
| Fresh sample of 3,000 rows below Bedrock against the chain | 0 mismatches |

Ten pass-1 rows were overtaken by the live sink during the apply and hold a
newer row. Not covered: the seeded history bars of addresses that changed
again later still carry the RPC era's zero readings (12.2 M hourly bars
across 2.3 M addresses below Bedrock); balances are unaffected.

The same shape applies to Arbitrum One's pre-Nitro seed, where the check
found no such defect (67 seeded rows in the sample, all equal).
