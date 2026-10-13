ERC-20 and native balances from Firehose **Extended** blocks, written to ClickHouse in the tables of substreams-evm `evm-balances` v0.3.4. No RPC and no params; the default network is BSC.

## Included

| File | Module | Contents |
| --- | --- | --- |
| [`${SPKG}`](https://github.com/pinax-network/substreams-evm-extended/releases/download/${TAG}/${SPKG}) | `db_out` | Both balance maps as ClickHouse rows, with the ClickHouse schema embedded. **The sink needs only this file.** |
| [`${NATIVE_SPKG}`](https://github.com/pinax-network/substreams-evm-extended/releases/download/${TAG}/${NATIVE_SPKG}) | `map_events` | Native balances: every account whose balance changed in the block, with its balance after the block. |
| [`${ERC20_SPKG}`](https://github.com/pinax-network/substreams-evm-extended/releases/download/${TAG}/${ERC20_SPKG}) | `map_events` | ERC-20 balances of every contract, inferred from `Transfer` flows and persisted storage. No token list. |

The two map packages contain the same modules that `${SPKG}` imports, so they share its server cache. Use them to stream one map on its own as `evm.balances.v1.Events`.

## ClickHouse tables

| Table | Key | Contents |
| --- | --- | --- |
| `erc20_balances` | `(contract, address)` | ERC-20 balance after a block; the latest with `FINAL` |
| `native_balances` | `address` | Native balance after a block; the latest with `FINAL` |
| `blocks` | `block_num` | Every block that wrote a balance row |
| `historical_erc20_balances`, `historical_native_balances` | views | Open/high/low/close balance bars at 1m, 5m, 10m, 30m, 1h, 4h, 1d and 1w |

Balances are raw `UInt256` amounts, with no token decimals applied. Addresses and block hashes are `0x` + lowercase hex. The tables and rows match the RPC-era database's, so a new database can [copy its history by partition](https://github.com/pinax-network/substreams-evm-extended/blob/${TAG}/evm-balances/clickhouse/examples/backfill-from-rpc-database.sql). No table stores a chain id: use one database per network.

## Deploy

Use [pinax-network/substreams-sink-sql ${SINK}](https://github.com/pinax-network/substreams-sink-sql/releases/tag/${SINK}) (commit `${SINK_COMMIT}`, image `ghcr.io/pinax-network/substreams-sink-sql:${SINK}`). StreamingFast's own `${SINK}` lacks its fixes: `substreams-sink-sql --version` must name commit `${SINK_COMMIT}`.

```sh
export SUBSTREAMS_SINK_DSN='clickhouse://<user>:<password>@<host>:9000/<database>'  # create <database> first
export SUBSTREAMS_API_KEY=...
PKG=https://github.com/pinax-network/substreams-evm-extended/releases/download/${TAG}/${SPKG}
substreams-sink-sql setup "$SUBSTREAMS_SINK_DSN" "$PKG"
substreams-sink-sql run "$SUBSTREAMS_SINK_DSN" "$PKG" -e bsc.substreams.pinax.network:443 <start-block>: --final-blocks-only
```

`setup` is idempotent and creates the tables, state tables, materialized views, views and the sink's `cursors` table. Keep `--final-blocks-only`: ClickHouse cannot undo. See [sink behaviour](https://github.com/pinax-network/substreams-evm-extended/blob/${TAG}/evm-balances/README.md#sink-behaviour) for replays and flush settings.

## Before you deploy

- **Not live-qualified.** No dated live evidence exists for this build yet ([#124](https://github.com/pinax-network/substreams-evm-extended/issues/124)).
- **Start block.** Start at a height qualified for the network. A block with incomplete or ambiguous native data stops the stream instead of writing a guess.
- **Native rows** cover only accounts with a persisted balance change. The RPC era also wrote unchanged accounts.
- **ERC-20 rows are measured, not qualified.** On five captured BSC intervals, erc20-balances v0.4.0 reproduced 58.5–62.8% of the RPC reference rows with 99.92–100% value precision ([measured](https://github.com/pinax-network/substreams-evm-extended/blob/${TAG}/erc20/balances/README.md#measured)). Pairs it does not emit keep their previous value, and computed balances (reward, reflection, rebasing) can be missed. [Monitor](https://github.com/pinax-network/substreams-evm-extended/blob/${TAG}/evm-balances/README.md#monitoring) ERC-20 rows per block.
- **Cutover** from the RPC-era database: follow the [cutover steps](https://github.com/pinax-network/substreams-evm-extended/blob/${TAG}/evm-balances/README.md#cutover-and-backfill) and the [cutover contract](https://github.com/pinax-network/substreams-evm-extended/blob/${TAG}/docs/initialization-and-completeness.md).
