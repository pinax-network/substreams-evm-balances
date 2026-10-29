Native account balances from Firehose **Extended** blocks, written to ClickHouse in the native tables of substreams-evm `evm-balances` v0.3.4. No RPC and no params; the default network is BSC. ERC-20 balances are the separate `evm-balances-erc20` package, with their own database.

## Included

| File | Modules | Contents |
| --- | --- | --- |
| [`${SPKG}`](https://github.com/${REPO}/releases/download/${TAG}/${SPKG}) | `map_events`, `db_out` | `map_events`: every account whose balance changed in the block, with its balance after the block, as `evm.balances.v1.NativeEvents`; block 0 carries the genesis allocations. `db_out`: those balances as ClickHouse rows, with the ClickHouse schema embedded. **The sink needs only this file.** |

## Changes

v0.7.11 restores the debit of a self-destruct that Polygon's `Block.ver` 3 producer records without the self-destruct flag, and refuses such a call in any other shape but a self-destruct to itself. Unlike v0.7.9 and v0.7.10, it changes rows of blocks that the earlier releases accept, and it is the first native release since v0.7.8 that can refuse blocks that the release before it accepts:

- **Polygon:** from Napoli (block 54,876,000, which brings EIP-6780), an account not created in its transaction that self-destructs to another account ends at zero, but the producer records only the beneficiary's credit of its balance, in a call without the self-destruct flag, and no record of the account. Earlier releases restore the debit only in a flagged call, so the account kept its balance. v0.7.11 writes it as zero at that block, in the exact captured shape (blocks 55,741,124 and 55,412,000). The 2026-10-19 comparison with the RPC-era database found 61 contracts whose balance v0.7.8's database keeps where the RPC-era database reads zero, from block 55,412,000 to 71,461,200 (1,694.17 POL). 58 of them are located in this shape, which v0.7.11 debits (1,692.97 POL): 56 in the blocks of those zero readings, and 2 more in blocks 56,042,508 and 58,697,458. The last 3 (1.2 POL) lie in blocks not fetched. The rule needs no network, `Block.ver` or fork gate: a self-destruct to another account leaves it at zero under either opcode.
- **Every network:** a persisted transaction call without the self-destruct flag that holds a self-destruct credit (`SUICIDE_REFUND`) in any other shape now stops the stream instead of keeping a balance that may be wrong, for example the self-destruct of an account with an earlier record in its transaction. One shape is kept as recorded, as by earlier releases: a self-destruct to itself recorded as the account's own credit from zero, its only record in the transaction, which ends at the balance EIP-6780 leaves the account. A few refused shapes may keep the right balance, such as a debit that the producer records itself, but no capture shows them without the flag. No such call is in the saved blocks and fixtures of any network. The 2026-10-29 pre-release noops passed on all eight networks, including Polygon ranges before Napoli and after the producer transition at block 73,440,256. Acceptance of those ranges does not establish coverage of every self-destruct shape; Polygon's full-history noop is still required before database writes.

On Polygon this release writes `polygon:${PACKAGE}@${VERSION}`. Every other network gets from it the rows of the release its database runs wherever no call without the self-destruct flag holds a self-destruct credit, and none was found in their saved blocks and fixtures, so a database there can stay on the version it runs: the module hash differs, and the sinks run with `--on-module-hash-mistmatch=error`.

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

- **Bounded pre-release checks passed (2026-10-29).** The exact CI-built package passed 525 historical noop windows across all eight networks, including 116 on Polygon. Same-block RPC checks matched all 15,721 emitted balances in the recorded recent intervals and the two captured Polygon defect blocks, with zero mismatches or RPC errors. [Dated evidence](https://github.com/${REPO}/blob/${TAG}/docs/history/native-v0711-prerelease-2026-10-29.json) records the intervals, module hashes, current reader/client versions, and limits. These are emitted-row samples, not full-history, global-holder, supply or cutover qualification; [#124](https://github.com/${REPO}/issues/124) remains separate.
- **Polygon rollout.** Run a full-history `db_out` noop of this release before any ClickHouse writes, then resync into `polygon:${PACKAGE}@${VERSION}` and repeat the identified-contract and supply comparisons. Continue to ingest `db_out`. The release does not deploy a sink or switch API reads, and other networks do not need a resync for this Polygon fix.
- **Polygon head (open risk).** No self-destruct of Polygon's `Block.ver` 4 and 5 producers (from block 73,440,256), which a live sink reads, is saved. If they leave the self-destruct flag off, as `Block.ver` 3 does for an account not created in its transaction, while they record the account's debit as Ethereum's `Block.ver` 4 and 5 producers do (a `SUICIDE_WITHDRAW`), every such self-destruct of a positive balance there stops the stream. Ethereum's `Block.ver` 4 and 5 producers flag all 55 of their saved self-destructs, which is indirect evidence only.
- **Reorgs.** The sink holds the newest 5 blocks back instead of waiting for finality. A fork deeper than that stops the sink; after a restart it continues on the canonical chain, but the rows it wrote for the orphaned blocks stay. This is an accepted risk ([reorgs](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#build-and-deploy)).
- **Start block.** A sync can start at block 0: it writes the genesis allocations. Where a network's Extended blocks begin later, start at the first whole 1,000-block segment of them: Optimism 105,236,000, Arbitrum One 22,208,000 ([start blocks](https://github.com/${REPO}/blob/${TAG}/native/balances/README.md#input-and-networks)). A block with incomplete or ambiguous native data, or a shape no rule pins, stops the stream instead of writing a guess; only this sink stops, not the ERC-20 one.
- **Native rows** cover only accounts with a persisted balance change. The RPC era also wrote unchanged accounts.
- **Cutover** from the RPC-era database: follow the [cutover steps](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#cutover-and-backfill) and the [cutover contract](https://github.com/${REPO}/blob/${TAG}/docs/initialization-and-completeness.md).
