Native account balances from Firehose **Extended** blocks, written to ClickHouse in the native tables of substreams-evm `evm-balances` v0.3.4. No RPC and no params; the default network is BSC. ERC-20 balances are the separate `evm-balances-erc20` package, with their own database.

## Included

| File | Modules | Contents |
| --- | --- | --- |
| [`${SPKG}`](https://github.com/${REPO}/releases/download/${TAG}/${SPKG}) | `map_events`, `db_out` | `map_events`: every account whose balance changed in the block, with its balance after the block, as `evm.balances.v1.NativeEvents`; block 0 carries the genesis allocations. `db_out`: those balances as ClickHouse rows, with the ClickHouse schema embedded. **The sink needs only this file.** |

## Changes

v0.7.12 restores the debit of a self-destruct that Polygon's `Block.ver` 3 producer records without the self-destruct flag also when the account has records earlier in its transaction, such as the value credit of the call that self-destructs it. v0.7.11 refuses those blocks. v0.7.12 writes the same rows as v0.7.11 for every block v0.7.11 accepts, on every network:

- **Polygon:** v0.7.11's backfill and full-history noop of 2026-10-29 stopped on 9 blocks, from 56,954,084 to 72,387,144 (`Block.ver` 3, after Napoli), each the first refusal of its range. In 8, a transaction sends POL to an existing contract (from 1 wei to 100 POL), which self-destructs in the same call: the contract's `TRANSFER` credit precedes the beneficiary's `SUICIDE_REFUND`. In block 68,559,510 the contract pays 0.0001 POL in a call before it self-destructs. v0.7.11's shape allows no earlier record of the account in its transaction, so neither matches. The RPC-era database reads zero for each contract at its block, where v0.7.8's and v0.7.10's databases keep the balance those records leave (96.16 POL in all). v0.7.12 writes zero at that block. The account's earlier records chain into the restored debit: the continuity check requires the balance they leave to be exactly what the self-destruct moves, so a debit that does not chain still fails the block.
- **Every network:** the other near-shapes of v0.7.11 still stop the stream: the self-destruct of an account created in its transaction, two refunds in the call, a refund that moves nothing, a `SUICIDE_WITHDRAW`, a refund that does not end its frame, and, in a Bor state-sync transaction, whose subtrees restart the ordinals, any other record of the account. The self-destruct to itself recorded as the account's own credit from zero is kept as recorded, as before.

On Polygon this release writes `polygon:${PACKAGE}@${VERSION}`: v0.7.11's Polygon database cannot pass these blocks. Every other network gets from it the rows of v0.7.11, since the change only affects blocks that v0.7.11 refuses, and none of their v0.7.11 backfills had refused one by 2026-10-30, so a database there can stay on v0.7.11: the module hash differs, and the sinks run with `--on-module-hash-mistmatch=error`.

## ClickHouse tables

| Table | Key | Contents |
| --- | --- | --- |
| `native_balances` | `address` | Native balance after a block; the latest with `FINAL` |
| `blocks` | `block_num` | Every block that wrote a native balance row |
| `historical_native_balances` | view | Open/high/low/close balance bars at 1h, 4h, 1d and 1w |

Balances are raw `UInt256` amounts in wei. Addresses and block hashes are `0x` + lowercase hex. The tables and rows match the RPC-era database's, so a new database can [copy its history](https://github.com/${REPO}/blob/${TAG}/native/balances/clickhouse/examples/backfill-from-rpc-database.sql). No table stores a chain id: use one database per network.

## Token API live sink

The BSC database is `bsc:${PACKAGE}@${VERSION}`, next to the RPC-era `bsc:evm-balances@v0.3.x`. The live sink takes `ASSET: evm`, `TYPE: balances-native`, `VERSION: ${VERSION}` and this release's package as `SPKG_URL`:

```
https://github.com/${REPO}/releases/download/${TAG}/${SPKG}
```

The Token API's native routes (`/balances/native`, `/holders/native`, `/tokens/native`, `/balances/historical/native`) read this database.

## Deploy

Use [pinax-network/substreams-sink-sql ${SINK}](https://github.com/pinax-network/substreams-sink-sql/releases/tag/${SINK}) (commit `${SINK_COMMIT}`, image `ghcr.io/pinax-network/substreams-sink-sql:${SINK}`). StreamingFast's own `${SINK}` lacks its fixes: `substreams-sink-sql --version` must name commit `${SINK_COMMIT}`.

On BSC, with the flags of the Token API's BSC balances live sink:

```sh
export SUBSTREAMS_SINK_DSN='clickhouse://<user>:<password>@<host>:9000/<database>'  # create <database> first
export SUBSTREAMS_API_KEY=...
PKG=https://github.com/${REPO}/releases/download/${TAG}/${SPKG}
substreams-sink-sql setup "$SUBSTREAMS_SINK_DSN" "$PKG"
substreams-sink-sql run "$SUBSTREAMS_SINK_DSN" "$PKG" <start-block>: \
  -e bsc.substreams.pinax.network:443 \
  --undo-buffer-size=5 \
  --live-block-flush-interval=20 \
  --batch-block-flush-interval=1000 \
  --batch-row-flush-interval=50000 \
  --live-block-time-delta=3m0s \
  --idle-timeout=5m \
  --on-module-hash-mistmatch=error
```

`setup` is idempotent and creates the tables, the state table, the materialized view, the view and the sink's `cursors` table. Other chains use other buffer and flush values; see [Token API live sinks](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#token-api-live-sinks-bsc) and [sink behaviour](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#sink-behaviour).

## Before you deploy

- **Not live-qualified.** No dated live evidence exists for this build yet ([#124](https://github.com/${REPO}/issues/124)). Its evidence is offline: the 9 refused blocks, fetched from Firehose on 2026-10-30 and replayed whole, all pass with each contract at zero, and their other 1,092 rows equal v0.7.10's; the RPC-era rows read zero for the 9 contracts at their blocks. The pre-release noops run before the tag.
- **Polygon rollout.** Run a full-history `db_out` noop of this release before any ClickHouse writes, at least over `Block.ver` 3 from Napoli (blocks 54,876,000 to 73,440,255), where the stopped ranges of v0.7.11 lie, then resync into `polygon:${PACKAGE}@${VERSION}` and repeat the identified-contract and supply comparisons. Continue to ingest `db_out`. The release does not deploy a sink or switch API reads, and other networks do not need a resync for this Polygon fix.
- **Polygon head (open risk).** No self-destruct of Polygon's `Block.ver` 4 and 5 producers (from block 73,440,256), which a live sink reads, is saved. If they leave the self-destruct flag off, as `Block.ver` 3 does for an account not created in its transaction, while they record the account's debit as Ethereum's `Block.ver` 4 and 5 producers do (a `SUICIDE_WITHDRAW`), every such self-destruct of a positive balance there stops the stream. Ethereum's `Block.ver` 4 and 5 producers flag all 55 of their saved self-destructs, which is indirect evidence only.
- **Reorgs.** The sink holds the newest 5 blocks back instead of waiting for finality. A fork deeper than that stops the sink; after a restart it continues on the canonical chain, but the rows it wrote for the orphaned blocks stay. This is an accepted risk ([reorgs](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#build-and-deploy)).
- **Start block.** A sync can start at block 0: it writes the genesis allocations. Where a network's Extended blocks begin later, start at the first whole 1,000-block segment of them: Optimism 105,236,000, Arbitrum One 22,208,000 ([start blocks](https://github.com/${REPO}/blob/${TAG}/native/balances/README.md#input-and-networks)). A block with incomplete or ambiguous native data, or a shape no rule pins, stops the stream instead of writing a guess; only this sink stops, not the ERC-20 one.
- **Native rows** cover only accounts with a persisted balance change. The RPC era also wrote unchanged accounts.
- **Cutover** from the RPC-era database: follow the [cutover steps](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#cutover-and-backfill) and the [cutover contract](https://github.com/${REPO}/blob/${TAG}/docs/initialization-and-completeness.md).
