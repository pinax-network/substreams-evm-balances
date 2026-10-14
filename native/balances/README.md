# native/balances: evm-balances-native

An RPC-free `map_events` reads Firehose Extended blocks and emits
**`evm.balances.v1.NativeEvents`** for native account balances, and `db_out`
writes them into the substreams-evm ClickHouse tables (`native_balances`,
`blocks` and the native history), whose schema the spkg embeds. The package is
`evm_balances_native`: release tag and spkg `evm-balances-native-v<version>`,
BSC database `bsc:evm-balances-native@<version>`
([ClickHouse](../../docs/clickhouse.md)). ERC-20 balances are the separate
[`erc20/balances`](../../erc20/balances/README.md) package with their own database.

`NativeEvents` is the shared [`proto/v1/balances.proto`](../../proto/v1/balances.proto)'s
`Events` without the contract, which native amounts never have: field 1 is
reserved and the other numbers are `Events`' and `Balance`'s, so the bytes
also decode as `Events` with no contract. `NativeBalance.amount` is the
account's balance after the last persisted change in the block, as an exact
decimal `uint256` string.

This package ports the reducer of the historical `evm-balances-storage`
prototype ([source](https://github.com/pinax-network/substreams-evm/blob/311f9005cc8606e9cb1034562b97cf7281f30c99/evm-balances-storage/src/lib.rs))
without its WBNB hardcode, custom protobuf, extra maps, `db_out` or Python
helpers. The [current RPC native module](https://github.com/pinax-network/substreams-evm/blob/970a665e15619de8ad7f686bd89412a1030d46dc/native/balances/src/lib.rs)
discovers candidate addresses and calls `eth_getBalance`; its heuristics are
comparison context, not persistence rules, and are not ported.

## What a row means

Balances are **account state**. Every persisted `BalanceChange` record of the
block is collected by the shared [`evm-persist`](../../common/persist) rules,
sorted by execution ordinal and reduced per account. The row is the final
value; the reducer also retains the value before the account's first change and
the ordinals of its first and last change for host-side checks.

- An account without a persisted change in the block is **not emitted**.
  Absence is no observation; it never initializes an untouched account to zero.
- An account that changes and returns to its starting value **is emitted**
  (a net-zero changing account is different from an individual no-op record,
  which the persistence rules drop).
- Final known zeros are emitted as `"0"`.
- The zero address, precompiles, BSC system contracts and burn-looking
  addresses are ordinary accounts. A credit to `0x…dead` is a balance change,
  not a supply statement.
- An absent `old_value`/`new_value` message **inside an observed record**
  encodes zero under the pinned Ethereum protobuf contract. This does not turn
  a missing record into a zero balance.
- CALLCODE/DELEGATECALL value, value-bearing calls and transfer logs are not
  consulted. Balances are not a sum of inferred transfers, and transfer lists
  do not cover every account-state effect.
- Rows are sorted by account and carry no block hash; the stream's clock and
  cursor identify the block. An empty output block is a block with no
  persisted native change, not a missing block.
- Block 0 emits the genesis allocations (see [Genesis](#genesis-v070)).

## Persistence and fail-closed rules

Which records persist is decided by [`evm-persist`](../../common/persist):
non-reverted frames of succeeded transactions; only `GAS_BUY`, `GAS_REFUND` and
`REWARD_TRANSACTION_FEE` from the root call of a failed or reverted
transaction; non-reverted system calls; every block-level record.

The reducer rejects the whole block when the producer data is ambiguous or
incomplete instead of guessing:

| Condition | Error |
| --- | --- |
| Block is not `DETAILLEVEL_EXTENDED` | `Extended blocks required` |
| Missing header, non-32-byte hash/parent/state root, header number mismatch | `invalid block identity` / `header number mismatch` |
| Block 0 with a balance change that is not a `GENESIS_BALANCE` from zero | `genesis block carries a balance change that is not a genesis allocation` |
| Transaction without status 1–3 or without calls | `incomplete transaction persistence data` |
| Failed/reverted transaction whose root call carries a reason other than the three gas reasons or the reasons known to revert (`TRANSFER`, `TOUCH_ACCOUNT`, `SUICIDE_REFUND`, `SUICIDE_WITHDRAW`, `CALL_BALANCE_OVERRIDE`, `BURN`) | `failed transaction carries a balance-change reason without pinned persistence semantics` |
| Address not 20 bytes | `invalid native account address` |
| Persisted record with ordinal 0 | `persisted native balance change has no execution ordinal` |
| Value longer than 32 bytes | `native balance exceeds uint256` |
| Two records of one account with the same ordinal | `ambiguous native balance execution order` |
| A record's old value differs from the account's previous new value, apart from the self-transfer pair and the fee pool payout below | `discontinuous native balance changes within block` |

**Self-transfer pair (v0.2.2).** A value transfer from an account to itself,
a CALLCODE with value or a CALL whose caller is its address, does not move the
balance, and geth records nothing for it. The BSC producer records it as two
`TRANSFER` records of that account at consecutive ordinals of the call, both
computed from the same prior balance `b`: `b -> b - v` and `b -> b + v`, where
`v` is the call's value. The second record's old value breaks continuity
although the balance never moved. In a succeeded transaction's non-reverted
call, exactly this pair is dropped, so the account's other records must chain
across it. Any other shape, including the same records under another call
type, value or ordinal gap, still fails the block. v0.2.1 refused BSC block
127,411,449 on it (transaction `0x5c664637…`, account `0x1a84c890…`, a
CALLCODE of 0.08 BNB); the captured transaction is the regression fixture
[`tests/fixtures/bsc-127411449-callcode-self-transfer.json`](tests/fixtures/bsc-127411449-callcode-self-transfer.json).

**Fee pool payout (v0.7.0).** BSC collects a block's transaction fees in its
fee pool, `consensus.SystemAddress` `0xff…fe`, and pays them out at the end of
the block: 1/16 of the pool (`pool >> 4`) to the system reward contract
`0x…1002`, the rest to the validator contract `0x…1000`. Each payout moves the
amount from the pool to the block's coinbase, which sends it on in a system
transaction. The producer of `Block.ver` 3 blocks records the first payout's
pool debit with its old value equal to its new one: the client lowered the
pool's balance in place before setting it, so the tracer read the lowered
balance as the old value. That block-level `REWARD_TRANSACTION_FEE` record
reads `n -> n`, the shared rules drop it as a no-op, and the pool's next record
starts from `n`, so continuity broke although the balance moved exactly as the
coinbase's credit records. When the next block-level record, before any
transaction starts, is a fee credit `r` of the block's coinbase with
`r = (n + r) >> 4`, native restores the debit as `n + r -> n`, and the
continuity check verifies `n + r` against the pool's previous balance. Any
other shape still fails the block. The credit is one or two ordinals after the
`n -> n` record: the producer can skip an ordinal between them. Native v0.2.2
refused BSC blocks 363, 12,700,000 and 25,401,003 on this shape, found by the
full-history noop on 2026-10-13 (every range below block 38.1M failed); v0.7.0
required the credit at the very next ordinal and refused blocks 958 and 4104,
where the producer skipped one, which v0.7.1 accepts. The five blocks are the
fixture
[`tests/fixtures/bsc-fee-pool-payout-ver3.json`](tests/fixtures/bsc-fee-pool-payout-ver3.json)
(blocks 363, 958 and 4104 whole, the others from the pool's last transaction
fee on).

### Genesis (v0.7.0)

Block 0 carries the chain's genesis allocations. The BSC producer records them
as one synthetic succeeded transaction (zero hash, from and to the zero
address) whose call holds a `GENESIS_BALANCE` record from zero per funded
account, next to the system contracts' code. Block 0 is accepted when every
balance record in it is such an allocation, and the ordinary rules then emit
one row per funded account: on BSC, TokenHub `0x…1004` with 176,405,560.9 BNB
and `0x446aa6…1f3e` and `0xb00574…c632` with 500 BNB each
([`tests/fixtures/bsc-0-genesis.json`](tests/fixtures/bsc-0-genesis.json)).
Native v0.2.2 refused block 0, and in production mode a stream from block 1
still ran it, because Substreams processes whole 1,000-block segments; a sync
can now start at block 0. All three accounts change later (RPC-era database),
so their current balances do not depend on block 0. BSC block 1 has no
balance change.

**Failed blob transactions (v0.7.2).** BNB Chain credits a blob
transaction's blob fee (`REASON_REWARD_BLOB_FEE`, 17) to the fee pool after
execution, as it does the transaction fee, so a failed blob transaction pays
it too. The shared rules persist only the three gas reasons of a failed
transaction's root call; native also persists the root call's blob fee when
the transaction is a blob transaction (type 3), and the record must chain like
any other. Native v0.7.0 refused BSC block 42,357,369 on it, found by the
full-history noop on 2026-10-14: failed blob transaction `0xbe965c35…`
credits the pool 131,072 wei after its transaction fee, and the pool's next
record, the block-level payout to the coinbase, starts from the credited
balance. The fixture is
[`tests/fixtures/bsc-42357369-failed-blob-fee.json`](tests/fixtures/bsc-42357369-failed-blob-fee.json)
(the failed transaction and every later one, and every block-level record).

The failed-transaction reason guard exists because OP-stack deposit mints
(`INCREASE_MINT`, 18), reverts (`REVERT`, 19) and a blob fee outside a blob
transaction have no saved fixture inside a failed transaction. Silently
dropping such a record could omit a persisted credit; failing the block
surfaces the case for qualification. See the
[persisted-effect matrix](docs/persisted-effects.md).

## Input and networks

`map_events(block)` takes only an Extended block, with no parameters. At the
owner's direction (2026-10-06), the `producer_versions` parameter and its
version gate were removed. `Block.ver` does not control acceptance: the map
checks the block's identity, complete transaction traces, persisted effects,
execution ordinals and balance continuity instead. Version 3, 4, 5 and an
unknown version are covered by synthetic regression tests using the same
valid and ambiguous records.

`network: bsc` remains a CLI endpoint default, not an admission gate. There
are no per-network parameters or network allowlist, and `db_out` takes no
params either. Select the intended network and endpoint when running it;
blocks do not carry a chain ID that this module can use to verify the
endpoint.

Acceptance is not qualification for another network or producer release.
Version-3 system-call ordinals can use a different scale from transaction
ordinals, and network-specific failed-transaction credits remain unqualified
(see the [persisted-effect matrix](docs/persisted-effects.md) and
[#125](https://github.com/pinax-network/substreams-evm-balances/issues/125)).
The existing data checks reject detectable ambiguity; they do not establish
that every chain's persistent effects are represented correctly. The dated
BSC evidence below still refers to its original packages and parameters.

## Build and test

```sh
cargo test --locked -p native-balances -p evm-persist
make -C native/balances build schema   # release WASM and clickhouse/schema.sql; no network
make -C native/balances gui            # or `prod`, MODULE=db_out; streams from ENDPOINT
```

Committed spkgs come from CI's "Package hashes" artifact, not from a local
`make pack` (a macOS build has other module hashes than CI's Linux build).

`make gui` and `make prod` read `SUBSTREAMS_API_KEY` from the environment.
The committed v0.1.0 and v0.2.0 SPKGs are offline builds; the historical live-
qualified build is identified by its WASM (`48d89d28…`) and module hash
(`5a2a2e0c…`) below. `substreams pack` embeds this README, so a pack after a
README edit has a new SPKG digest but the same module. The 2026-10-05
substreams 0.8.0 build had WASM `0bc2e2cf…` and, under `bsc`, module
`868ac202…`; it was not live-qualified. The parameter-free
v0.2.0 source changes the native module and dependent `db_out` hashes again.
It has no new live qualification. v0.2.1 is the same source built with Rust
1.99.0 and reproducible paths (#118): CI's canonical Linux build has WASM
`d3b6a549…` and module `72453949…`, also not live-qualified. v0.2.2 adds the
self-transfer pair exception above; until v0.6.1 the combined `evm-balances`
package imported it and wrote its rows. v0.7.0 (package `evm_balances_native`)
adds `db_out`, `NativeEvents`, the fee pool payout and genesis; v0.7.1 accepts
the payout credit after a skipped ordinal; v0.7.2 persists a failed blob
transaction's blob fee. None is live-qualified.

## Live qualification (BSC, 2026-09-23)

The packed map of 2026-09-23 (`spkg` sha256 `fbb46fc7…`, wasm `48d89d28…`, default
parameters `{"producer_versions":[5]}`) was streamed with
`--final-blocks-only` from `bsc.substreams.pinax.network` and checked with
the host tool `native-balances-tools live-parity`
([source at `6dade89`](https://github.com/pinax-network/substreams-evm-balances/tree/6dade8957887c0c278cfa8da6bef61b9cc22f534/native/balances/tools); removed since). It binds
each block number to its canonical hash and batches
`eth_getBalance(address, {blockHash})`:

- **Saved controls** ([report](docs/evidence/live-parity-bsc-2026-09-23-saved-controls.json)):
  the 1,024 contiguous saved blocks 122,288,006–122,289,029. The packaged
  rows equal the offline Rust replay of the saved `.pb` files row for row
  (76,139 rows, 17,670 accounts, 6,366 known zeros), and every row equals
  `eth_getBalance` at the block hash (76,139/76,139).
- **Live window** ([report](docs/evidence/live-parity-bsc-2026-09-23-live.json)):
  the 5,000 contiguous final blocks 123,548,070–123,553,069, streamed without
  a refused block: 670,873 rows for 102,913 accounts (45,162 known zeros).
  Every row of the last 1,000 blocks and every tenth row of the others was
  checked: 200,344/200,344 equal. Block 123,550,024 has no transaction and
  no output; the miner, fee and system accounts are unchanged across it.
- **Reasons** ([report](docs/evidence/live-reasons-bsc-2026-09-23.json),
  [matrix](docs/persisted-effects.md)): the last 250 blocks fetched from
  Firehose and replayed offline equal the packaged output and carry
  transfers, gas, fee rewards, 37 blob-fee rewards and 500 block-level fee
  resets.

```sh
substreams run -e bsc.substreams.pinax.network:443 <spkg> map_events -s <start> -t <stop> \
  --final-blocks-only -o jsonl --bytes-encoding hex > events.jsonl
```

**Retention from a checkpoint** ([report](docs/evidence/live-retention-checkpoint-bsc-2026-09-23.json)):
the host tool `native-balances-tools retention-check` (removed since) seeded
`eth_getBalance` for 14,172 accounts at block 123,548,069 into the
`common/retention` ledger, applied the
next 300 packaged blocks and compared every retained value with RPC at four
block hashes: 56,688/56,688 equal, including 1,252 accounts that were never
emitted in the window and kept their checkpoint value.

Parity covers the emitted accounts only: an account without a persisted
change is not emitted and not checked, and an RPC candidate list (such as the
current RPC module's) would also contain unchanged accounts, which are not
state-derived updates. Other networks, producer versions and the Ethereum,
OP and Arc rows of the matrix stay under
[#8](https://github.com/pinax-network/substreams-evm-balances/issues/8).

## Offline evidence

Rust regressions (`src/tests.rs`) cover the complete captured BSC block
122260950 against all **82 saved native `eth_getBalance` rows**, the historical
prototype's 82 old/new/ordinal rows, the BSC fee-reset order (transaction
ordinal 3237 credit, block ordinal 3244 reset to zero), the two captured
failed SetCode transactions, net-zero accounts, absent value messages, zero and
burn-looking addresses, `uint256` max, tied/zero ordinals, discontinuity,
reverted frames, failed transactions, reverted system calls, the failed-
transaction reason guard, parameter-free acceptance across block versions,
version-independent ambiguity rejection, the `NativeEvents` wire encoding, the
captured fee pool payouts of blocks 363, 12,700,000 and 25,401,003 with the
exact shape they need, and BSC's captured genesis block 0. `tests/db_out.rs`
covers the ClickHouse rows, the manifest wiring and the embedded schema.

The host replay tool (`native/balances/tools`, [source at `6dade89`](https://github.com/pinax-network/substreams-evm-balances/tree/6dade8957887c0c278cfa8da6bef61b9cc22f534/native/balances/tools);
removed since) reduced directories of captured
`<height>.pb` blocks, linked consecutive block hashes, checked that each
account's `old_amount` equals its previously emitted amount whenever every
intervening block was replayed, and compared against saved oracles. Its
reports over the locally retained BSC captures are in
[`docs/evidence/`](docs/evidence/) and summarized in the
[persisted-effect matrix](docs/persisted-effects.md): 1,439 version-5 blocks,
126,180 rows and 86,564 cross-block continuity checks with zero mismatches,
plus the 82-row same-block RPC oracle. Those captures are the
ERC-20 campaign's block cache, not a native-specific RPC audit: only block
122260950 has a same-block native RPC oracle. The historical 192-block,
37,086-check native audit ([legacy qualification](https://github.com/pinax-network/substreams-evm-balances/blob/9b41c7f/erc20/balances/docs/legacy-qualification.md))
applies to the removed prototype package, not to this one.

## ClickHouse

`db_out` writes `native_balances`, `blocks` and the native history of the
substreams-evm schema into this package's database; the sink, the Token API
live sink, the cutover and the examples in
[`clickhouse/examples/`](clickhouse/examples/) are in
[`docs/clickhouse.md`](../../docs/clickhouse.md). The earlier CLI native-sink
path (bridge view, `Balance` TTL, checks of 2026-10-03) is at
[`cb62110`](https://github.com/pinax-network/substreams-evm-balances/tree/cb6211007f1f9d0cb5852a6b666da475a8fa6542/native/balances/clickhouse).

## Boundaries

No RPC, no candidate discovery, no transfer events, no supply or burn
interpretation, no wallet labels, no ERC-20 tables, no custom sink, no global
holder enumeration. Initialization of untouched accounts, exact checkpoints, reorg
completeness and the complete-block clock contract are shared under
[#7](https://github.com/pinax-network/substreams-evm-balances/issues/7).
Other networks' semantics are separate qualification work under
[#8](https://github.com/pinax-network/substreams-evm-balances/issues/8).
This package does not change `erc20/balances`.
