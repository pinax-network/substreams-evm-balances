Native account balances from Firehose **Extended** blocks, for a StreamingFast hosted store and for ClickHouse, in the native tables of substreams-evm `evm-balances` v0.3.4. No RPC and no params; the default network is BSC. ERC-20 balances are the separate `evm-balances-erc20` package, with their own database.

## Included

| File | Modules | Contents |
| --- | --- | --- |
| [`${SPKG}`](https://github.com/${REPO}/releases/download/${TAG}/${SPKG}) | `map_sink_entries`, `db_out` | `map_sink_entries`: every account whose balance changed in the block, with its balance after the block, as hosted-store `SinkEntries` (key: the 20-byte address; value: `evm.balances.v1.NativeBalance`); block 0 carries the genesis allocations. `db_out`: those entries as ClickHouse rows, with the ClickHouse schema embedded. **The sink and the hosted store need only this file.** |

## Changes

v0.8.0 changes the map's output for a StreamingFast hosted store (owner, 2026-10-20). The balances and the ClickHouse rows are v0.7.10's for every block, on every network:

- **`map_sink_entries` replaces `map_events`.** It emits `sf.substreams.foundational_store.model.v2.SinkEntries`, the only shape a hosted store ingests: one entry per account whose balance changed in the block, keyed by its 20-byte address, its value an `Any` of `evm.balances.v1.NativeBalance` (the address and its balance after the block), with `if_not_exist` false so every block overwrites the stored balance. `map_events` and its `evm.balances.v1.NativeEvents` are gone: a consumer of them reads the entries' values instead.
- **`db_out` reads the entries** and writes the same rows. The block is reduced once for both outputs, and a hosted store caches only `map_sink_entries`.

A database can stay on the version it runs: the rows are the same, the module hashes differ, and the sinks run with `--on-module-hash-mistmatch=error`.

## Hosted store

On The Graph Market, under Hosted Services, create a **Hosted Store** with a Substreams feed: this release's package URL, the network, Type URL `evm.balances.v1.NativeBalance`, output module `map_sink_entries`, and start block `0` for a complete store. StreamingFast hosts the store; it answers `Get` per address at a block number over gRPC. See [hosted store](https://github.com/${REPO}/blob/${TAG}/native/balances/README.md#hosted-store-v080) for reading and testing it.

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

- **Not live-qualified.** No dated live evidence exists for this build yet ([#124](https://github.com/${REPO}/issues/124)). Its evidence is offline: captured fixtures, RPC-era rows at the refused block and a comparison with v0.7.9 on saved blocks.
- **Reorgs.** The sink holds the newest 5 blocks back instead of waiting for finality. A fork deeper than that stops the sink; after a restart it continues on the canonical chain, but the rows it wrote for the orphaned blocks stay. This is an accepted risk ([reorgs](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#build-and-deploy)).
- **Start block.** A sync can start at block 0: it writes the genesis allocations. Where a network's Extended blocks begin later, start at the first whole 1,000-block segment of them: Optimism 105,236,000, Arbitrum One 22,208,000 ([start blocks](https://github.com/${REPO}/blob/${TAG}/native/balances/README.md#input-and-networks)). A block with incomplete or ambiguous native data, or a shape no rule pins, stops the stream instead of writing a guess; only this sink stops, not the ERC-20 one.
- **Native rows** cover only accounts with a persisted balance change. The RPC era also wrote unchanged accounts.
- **Cutover** from the RPC-era database: follow the [cutover steps](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#cutover-and-backfill) and the [cutover contract](https://github.com/${REPO}/blob/${TAG}/docs/initialization-and-completeness.md).
