ERC-20 balances from Firehose **Extended** blocks, inferred per block from `Transfer` flows and persisted storage, and written to ClickHouse in the ERC-20 tables of substreams-evm `evm-balances` v0.3.4. No RPC, no params and no token list; the default network is BSC. Native balances are the separate `evm-balances-native` package, with their own database.

## Included

| File | Modules | Contents |
| --- | --- | --- |
| [`${SPKG}`](https://github.com/${REPO}/releases/download/${TAG}/${SPKG}) | `map_events`, `db_out` | `map_events`: the ERC-20 balances of every contract inferred in the block, as `evm.balances.v1.Events`. `db_out`: those balances as ClickHouse rows, with the ClickHouse schema embedded. **The sink needs only this file.** |

## ClickHouse tables

| Table | Key | Contents |
| --- | --- | --- |
| `erc20_balances` | `(contract, address)` | ERC-20 balance after a block; the latest with `FINAL` |
| `blocks` | `block_num` | Every block that wrote an ERC-20 balance row |
| `historical_erc20_balances` | view | Open/high/low/close balance bars at 1h, 4h, 1d and 1w |

Balances are raw `UInt256` amounts, with no token decimals applied. Addresses and block hashes are `0x` + lowercase hex. The tables and rows match the RPC-era database's, so a new database can [copy its history](https://github.com/${REPO}/blob/${TAG}/erc20/balances/clickhouse/examples/backfill-from-rpc-database.sql). No table stores a chain id: use one database per network.

## Token API live sink

The BSC database is `bsc:${PACKAGE}@${VERSION}`, next to the RPC-era `bsc:evm-balances@v0.3.x`. The live sink takes `ASSET: evm`, `TYPE: balances-erc20`, `VERSION: ${VERSION}` and this release's package as `SPKG_URL`:

```
https://github.com/${REPO}/releases/download/${TAG}/${SPKG}
```

The Token API's ERC-20 routes (`/balances`, `/holders`, `/tokens`, `/balances/historical`) read this database.

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

- **Not live-qualified.** No dated live evidence exists for this build yet ([#124](https://github.com/${REPO}/issues/124)).
- **Reorgs.** The sink holds the newest 5 blocks back instead of waiting for finality. A fork deeper than that stops the sink; after a restart it continues on the canonical chain, but the rows it wrote for the orphaned blocks stay. This is an accepted risk ([reorgs](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#build-and-deploy)).
- **Rows are measured, not qualified.** On five captured BSC intervals, the inference reproduced 58.5–62.8% of the RPC reference rows with 99.92–100% value precision ([measured](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#measured)). Pairs it does not emit keep their previous value, and computed balances (reward, reflection, rebasing) can be missed. [Monitor](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#monitoring) ERC-20 rows per block.
- **Inference never fails a block.** A native refusal no longer stops these rows: native is its own sink.
- **Start block.** A block that is not Extended stops the stream. Where a network's Extended blocks begin after genesis, start at the first whole 1,000-block segment of them: Optimism 105,236,000, Arbitrum One 22,208,000 ([start blocks](https://github.com/${REPO}/blob/${TAG}/native/balances/README.md#input-and-networks)).
- **Cutover** from the RPC-era database: follow the [cutover steps](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#cutover-and-backfill) and the [cutover contract](https://github.com/${REPO}/blob/${TAG}/docs/initialization-and-completeness.md).
