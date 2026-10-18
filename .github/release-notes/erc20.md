ERC-20 balances from Firehose **Extended** blocks, inferred per block from `Transfer` flows and persisted storage, and written to ClickHouse in the ERC-20 tables of substreams-evm `evm-balances` v0.3.4. No RPC, no params and no token list; the default network is BSC. Native balances are the separate `evm-balances-native` package, with their own database.

## This release

v0.7.4 (owner, 2026-10-17) changes which rows are inferred, for past blocks as well as new ones, so it is written to a **new database, `<chain>:evm-balances-erc20@v0.7.4`, filled by a full resync** from each network's start block; a v0.7.3 database is not reused. On 131 saved Ethereum, BSC and Polygon blocks it keeps every v0.7.3 row with the same value except two wrong rows of Liquorice share tokens, which it drops, and adds 16,363 rows; on two blocks of Origin's OUSD and OETH it drops their rows for the block ([history](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#build-and-rust-tests)):

- A `balanceOf` input longer than 36 bytes (bots reuse a 68-byte `transfer` buffer) is read as `balanceOf(input[16..36])` instead of dropping the token for the block, and the cut shape (a 4-byte input or no return data) drops it only where the tracer could have cut the call.
- In a block without a `Transfer` of the contract, a native-backed WETH9 deposit or withdrawal and a TetherToken `destroyBlackFunds` vote.
- Tether `Issue`/`Redeem` name the sender of the emitting frame, and they and `DestroyedBlackFunds` explain the writes whose net change equals their amount.
- A `balanceOf` vouches for an unexplained write only when it was traced after it.
- A FiatToken v2.2 (USDC) `balanceOf` that returns a holder's word without the blacklist bit, which a write in a frame whose `Blacklisted` or `UnBlacklisted` names the holder sets or clears, no longer drops the token for the block; only that holder gets no row.
- Polygon's state-sync transaction is ordered by its depth-1 calls.
- Vyper's `HashMap` key order is read (crvUSD, CRV, Curve pools and gauges, Yearn vaults); under it, a contract's own row is never emitted, because a Yearn v3 vault computes its own balance.
- A holder's word whose next or second next storage key is also written in the block is a struct field (Curve BoostV2's `Point`), and drops its contract for the block unless the holder's own `balanceOf` traced after its last write shows the word.
- A traced `scaledBalanceOf(holder)` that reads the chosen mapping, keyed to it or returning the holder's stored word, drops its contract for the block: the word is a scaled balance that `balanceOf` converts. On the 131 saved blocks this removes exactly the two rows of Liquorice's LLCUSDC and LDWETH at Ethereum block 25,852,210, whose share words are about 1000 times their `balanceOf`.
- A holder whose word under the chosen mapping moves by a nonzero amount a thousand times more or less than its `Transfer` flow drops its contract for the block: the word is in another unit for that holder, as Origin's OUSD and OETH keep a rebasing holder's credits. On two Ethereum blocks of OUSD and OETH it removes the rebasing holders' wrong rows and the rows of the tokens' other holders in those blocks; on the 131 saved blocks it removes none.
- A block that the persistence rules refuse is retried once, as native v0.7.9 does, with each failed SetCode (EIP-7702) transaction's self-authorization that cannot apply (the sender's own, with the transaction's nonce, which the producer leaves unmarked) marked discarded; any other refusal stands. v0.7.3 refused Arbitrum One blocks 358,023,229, 360,021,045, 382,234,350, 420,239,048 and 472,773,590 on it ([producer releases](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#producer-releases)).
- For programs that call the Rust crate: `erc20_balances::run(block)` is unchanged. The new `erc20_balances::run_sized(block, block_len)`, which `map_events` calls, also takes the byte length of the delivered block (`Any.value`), one bound on where the tracer could have cut call data; `run` bounds by the block's re-encoding, which drops unknown fields.

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

- **Not live-qualified.** No dated live evidence exists for this build yet ([#124](https://github.com/${REPO}/issues/124)). v0.7.4's rules were checked offline, not on a live stream: of the 48,106 rows v0.7.4 gives on 131 saved Ethereum, BSC and Polygon blocks, the 10,572 that have an RPC-era 1-minute bar at their block all equal it; the other 37,534 were not compared, among them 31,555 of the 31,743 rows that v0.7.3 also gives and 360 of the 481 Vyper rows.
- **Reorgs.** The sink holds the newest 5 blocks back instead of waiting for finality. A fork deeper than that stops the sink; after a restart it continues on the canonical chain, but the rows it wrote for the orphaned blocks stay. This is an accepted risk ([reorgs](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#build-and-deploy)).
- **Rows are measured, not qualified.** On five captured BSC intervals, the inference source of 2026-10-05 (package v0.2.0) reproduced 58.5–62.8% of the RPC reference rows with 99.92–100% value precision ([measured](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#measured)); that measurement was not re-run for later versions, v0.7.4 included. Pairs it does not emit keep their previous value, and computed balances (reward, reflection, rebasing, shares converted by a rate) can be missed: in a block without a traced `balanceOf` or `scaledBalanceOf` of such a token, its stored word is emitted ([computed balances](https://github.com/${REPO}/blob/${TAG}/erc20/balances/README.md#limits)). [Monitor](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#monitoring) ERC-20 rows per block.
- **Own rows of Vyper vaults.** A Yearn v3 vault's `balanceOf(vault)` falls with time while its stored word stays, so v0.7.4 never emits a contract's own row under Vyper's key order, neither from a write nor from a traced `balanceOf`; the RPC era re-read it in nearly every block that touched the vault. A traced `balanceOf(vault)` that returns less than the word after the vault's last write drops all of that vault's rows for the block.
- **Inference never fails a block.** A native refusal no longer stops these rows: native is its own sink.
- **Start block.** A block that is not Extended stops the stream. Where a network's Extended blocks begin after genesis, start at the first whole 1,000-block segment of them: Optimism 105,236,000, Arbitrum One 22,208,000 ([start blocks](https://github.com/${REPO}/blob/${TAG}/native/balances/README.md#input-and-networks)).
- **Cutover** from the RPC-era database: follow the [cutover steps](https://github.com/${REPO}/blob/${TAG}/docs/clickhouse.md#cutover-and-backfill) and the [cutover contract](https://github.com/${REPO}/blob/${TAG}/docs/initialization-and-completeness.md).
