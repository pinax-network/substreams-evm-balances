# Repair stale holders of a token at a fixed block

Until v0.7.4, `erc20/balances` dropped a whole token's rows for a block when
one holder's word was doubtful. Until v0.7.5 it never read Solady keys,
MKR's burns without a `Transfer`, or a single vote beside a counter or a
packed struct. A holder whose change fell in such a block keeps its earlier
row until it moves again in a block the package accepts
([ERC-20 quality](../../../../../docs/quality-erc20.md#contract-blocks-dropped-by-the-inference)).
Changing the live sinks' package does not heal those rows. This procedure
reads every row of a token from the chain at one block H, and writes a
corrected row at H for each one that differs, with the live sink running.

**Run it only after the live sinks run v0.7.6.** Otherwise the version that
made a row stale can make it stale again after H.

## Steps

For each token, in a directory of its own:

1. **H** is the database's cursor block, which must be after the switch to
   v0.7.6:

   ```sql
   SELECT max(block_num) FROM `<chain>:evm-balances-erc20@v0.7.4`.cursors FINAL
   ```

2. **Export** the token's rows current at or below H into
   `seeded_rows.tsv`:

   ```sql
   SELECT contract, address, block_num, block_hash, toUnixTimestamp(timestamp), toString(balance)
   FROM `<chain>:evm-balances-erc20@v0.7.4`.erc20_balances FINAL
   WHERE contract = '<token>' AND block_num <= <H> ORDER BY address FORMAT TSV
   ```

3. **Read the chain at H** with the seed repair's reader:
   `OUT_PREFIX=at_H RPC_ID=<rpc-id> python3 ../../repair-pre-start-seed/fetch.py . <H> <shard> <shards>`,
   one process per shard. It is resumable, and reads `PINAX_KEY` without
   printing it.
4. **Compare**: `python3 ../compare.py . <rpc-id>` writes
   `corrected_rows.tsv`. It has one row at H, with H's hash and time, for
   every row whose value differs from `balanceOf` at H. Rows whose call
   returned no word are left.
5. **Apply** with admin credentials, one step at a time:
   `CHAIN=<chain> CH_URL=... CH_USER=... CH_PASSWORD=... ../apply.sh create|check|rows|verify|drop`.
   `VERSION` (default `v0.7.4`) names the database. A pair the live sink
   wrote after H keeps that newer row. No history bar is deleted: a stale
   pair has no row in H's windows, so the materialized view only adds the
   corrected bars there.

## Dry run (2026-11-05, read-only)

At the cursors of 2026-11-05 03:38 UTC (Ethereum block 26,345,990, Arbitrum
One block 521,366,877), before the sinks moved off v0.7.4:

| Token | Rows | Stale | Database minus chain |
| --- | ---: | ---: | ---: |
| sUSDe (`0x9d39a5de…`) | 69,226 | 17,173 | +2.46 billion sUSDe |
| AAVE (`0x7fc66500…`) | 785,133 | 16,073 | +2.60 M AAVE |
| MKR (`0x9f8f72aa…`) | 400,152 | 4,445 | +289,441 MKR |
| USDG (`0x004b5068…`, Arbitrum One) | 2,482 | 50 | +1.87 M USDG |

HEX (9.4 million rows) and stkAAVE were not read. Solady tokens such as
pUSD and AERO have no rows at all, so this procedure cannot repair them;
the v0.7.6 resync fills them. After the repair, AAVE's delegating holders
and stkAAVE, and USDG's packed accounts, go stale again when they next move,
because their words pack other fields (see the
[limits](../../../README.md#limits)).
