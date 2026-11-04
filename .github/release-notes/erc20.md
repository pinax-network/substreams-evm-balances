ERC-20 balances from Firehose **Extended** blocks, inferred per block from `Transfer` flows and persisted storage, and written to ClickHouse in the ERC-20 tables of substreams-evm `evm-balances` v0.3.4. No RPC, no params and no token list; the default network is BSC. Native balances are the separate `evm-balances-native` package, with their own database.

## This release

v0.7.5 (owner, 2026-11-04) changes which rows are inferred, for past blocks as well as new ones, so it is written to a **new database, `<chain>:evm-balances-erc20@v0.7.5`, filled by a full resync** from each network's start block; a v0.7.4 database is not reused. It keeps every row of v0.7.4 with the same value and adds the rows of holders that v0.7.4 dropped together with a doubtful holder of the same token: on the 24 blocks of the stale holders that the chain check of 2026-11-03 found, 43 random recent blocks of six chains and every committed fixture, it adds 51 rows, each equal to `balanceOf` at its block ([history](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#build-and-rust-tests), [ERC-20 quality](https://github.com/${REPO}/blob/${TAG}/docs/quality-erc20.md#contract-blocks-dropped-by-the-inference)):

- A holder whose word is at or above 2^128, a packed field next to its balance, gets no row; v0.7.4 dropped the token's rows for the block. AaveTokenV3 keeps a delegating holder's delegation mode and a delegatee's delegated power above a 104-bit balance, so v0.7.4 dropped AAVE in every block that wrote such a word, the stkAAVE contract's included, which left 258 of AAVE's top 1,000 holders stale on 2026-11-03.
- A holder whose word moves a thousand times more or less than its `Transfer` flow (a rebasing holder's credits in OUSD and OETH, USDG's accounts that pack a timestamp and an accumulator above the balance) gets no row; v0.7.4 dropped the token for the block. Such a holder's large mismatch no longer keeps the mapping from being chosen.
- A `balanceOf` that returns the holder's stored word with its high bits cleared (AAVE returns the low 104 bits) leaves only that holder without a row; v0.7.4 took it as a contradiction and dropped the token.
- A FiatToken v2.2 (USDC) holder whose writes after a credit or debit in the same block only set or clear the blacklist bit, in frames whose `Blacklisted` or `UnBlacklisted` names it, gets the balance that write left, which `balanceOf` returns; v0.7.4 left it at its earlier value.
- Holders whose word packs other fields stay at the last value a block emitted, as in v0.7.4: AAVE's delegating holders and delegatees, the stkAAVE contract and USDG's packed accounts ([limits](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#limits)).

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

- **Not live-qualified.** No dated live evidence exists for this build yet ([#124](https://github.com/${REPO}/issues/124)). v0.7.5's changes were checked offline, not on a live stream: every row it adds on 24 blocks of stale holders, 43 random recent blocks and the committed fixtures equals `balanceOf` at its block. v0.7.4's rules were checked offline too: of the 48,106 rows v0.7.4 gives on 131 saved Ethereum, BSC and Polygon blocks, the 10,572 that have an RPC-era 1-minute bar at their block all equal it; the other 37,534 were not compared, among them 31,555 of the 31,743 rows that v0.7.3 also gives and 360 of the 481 Vyper rows.
- **Reorgs.** The sink holds the newest 5 blocks back instead of waiting for finality. A fork deeper than that stops the sink; after a restart it continues on the canonical chain, but the rows it wrote for the orphaned blocks stay. This is an accepted risk ([reorgs](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#build-and-deploy)).
- **Rows are measured, not qualified.** On five captured BSC intervals, the inference source of 2026-10-05 (package v0.2.0) reproduced 58.5–62.8% of the RPC reference rows with 99.92–100% value precision ([measured](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#measured)); that measurement was not re-run for later versions, v0.7.5 included. Pairs it does not emit keep their previous value, and computed balances (reward, reflection, rebasing, shares converted by a rate) can be missed: in a block without a traced `balanceOf` or `scaledBalanceOf` of such a token, its stored word is emitted ([computed balances](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#limits)). [Monitor](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#monitoring) ERC-20 rows per block.
- **Own rows of Vyper vaults.** A Yearn v3 vault's `balanceOf(vault)` falls with time while its stored word stays, so since v0.7.4 no contract's own row is emitted under Vyper's key order, neither from a write nor from a traced `balanceOf`; the RPC era re-read it in nearly every block that touched the vault. A traced `balanceOf(vault)` that returns less than the word after the vault's last write drops all of that vault's rows for the block.
- **Inference never fails a block.** A native refusal no longer stops these rows: native is its own sink.
- **Start block.** A block that is not Extended stops the stream. Where a network's Extended blocks begin after genesis, start at the first whole 1,000-block segment of them: Optimism 105,236,000, Arbitrum One 22,208,000 ([start blocks](https://github.com/${REPO}/blob/${TAG}/native/balances/README.md#input-and-networks)).
- **Cutover** from the RPC-era database: follow the [cutover steps](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#cutover-and-backfill) and the [cutover contract](https://github.com/${REPO}/blob/${TAG}/docs/initialization-and-completeness.md).
