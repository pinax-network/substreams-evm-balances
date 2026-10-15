# erc20/balances: evm-balances-erc20

An RPC-free `map_events(block)` reads Firehose Extended blocks and emits
**`evm.balances.v1.Events`**, and `db_out` writes them into the substreams-evm
ClickHouse tables (`erc20_balances`, `blocks` and the ERC-20 history), whose
schema the spkg embeds. The package is `evm_balances_erc20`: release tag and
spkg `evm-balances-erc20-v<version>`, BSC database
`bsc:evm-balances-erc20@<version>` ([ClickHouse](../../docs/clickhouse.md)).
Native balances are the separate [`native/balances`](../../native/balances/README.md)
package with their own database.

`map_events` uses the exact shared protobuf and Rust types from
[`erc20/balances` in the original repository](https://github.com/pinax-network/substreams-evm/tree/9f723e4b28a3abad755384ff1329950ea1287938/erc20/balances)
and [`proto/v1/balances.proto`](../../proto/v1/balances.proto).
It takes no params, uses no stores and adds no protobufs; its only cache is
the `map_events` output, and `db_out` takes no params either. Balances are
inferred per block for every contract from its `Transfer` flows and persisted
storage. There are no layouts, token lists or RPC calls.

This module requires Firehose Extended blocks. It lives in
`substreams-evm-balances` (formerly `substreams-evm-extended`) so Extended-only
processing stays separate from the original repository. Until v0.6.1 the
combined `evm-balances` package imported this manifest and wrote its rows; since
v0.7.0 this package's own `db_out` writes them into its own database, so an
ERC-20 change never resyncs native balances
([why](../../docs/clickhouse.md)). The earlier CLI native-sink path and its
smoke evidence are at
[`cb62110`](https://github.com/pinax-network/substreams-evm-balances/tree/cb6211007f1f9d0cb5852a6b666da475a8fa6542/erc20/balances/clickhouse).
[Migration provenance](../../docs/migration.md) distinguishes
the preserved historical package from the package built in this repository.

## Rules

A block fails only when it is not a complete Extended block (detail level,
producer version 3–5, block identity, transaction status and calls) or when the
embedded [persistence rules](src/persist.rs) cannot resolve it. Inference itself
never fails a block: any doubt drops that contract's rows for the block. Work
per block is bounded by the block's calls, logs, writes and 64-byte preimages;
longer recorded preimages are never decoded. Per block and per contract
([`src/infer.rs`](src/infer.rs)):

1. **Candidates** are the RPC reference's `(contract, holder)` pairs: logs of
   non-reverted calls in successful transactions matching its 64 event shapes,
   their participants, `tx.from` and the emitting contract. USDT
   `Issue`/`Redeem` holders need an `owner()` call and are excluded.
2. **Votes**: in every such call frame, each holder's signed net `Transfer` flow
   is compared with the net persisted delta of the frame's writes whose key has
   a verified same-frame preimage `pad(holder) || base`. DELEGATECALL frames
   match by the proxy address of logs and writes. Exact votes for flows below
   10^6 do not count for a mapping that also mismatches. `Deposit`,
   `Withdrawal`, `Mint` and `Burn` never vote, because staking pools and lock
   ledgers emit the same shapes; a contract without a nonzero `Transfer` in
   the block is not inferred.
3. **Base**: the mapping every keyed `balanceOf` reads, if it has exact votes;
   otherwise the mapping with the most exact votes, and a tie infers nothing.
   A traced `balanceOf` is keyed when its storage context recorded a
   `pad(holder) || base` preimage; see [producer releases](#producer-releases).
   Inner mappings (with their own 64-byte preimage, such as allowances) and
   mappings with more large mismatches than exact votes (a packed word whose
   other field moves) are never chosen.
4. **Exclusions**: malformed records or event addresses; removed code or a
   self-destruct; discontinuous writes to a key; a traced `balanceOf(holder)`
   that calls another contract (a beacon `implementation()` lookup followed by
   its DELEGATECALL is allowed) or contradicts the stored word at its execution
   ordinal, or a keyed one that reads another holder mapping; a successful
   `balanceOf` call to the contract without a 36-byte input or return data,
   the shape a producer leaves when it truncates a call.
5. **Other holder state**: when other mappings mismatch the event flows at least
   as often as the base matches them, as reward and reflection tokens do, only
   holders with their own keyed `balanceOf` keep rows. With two or more exact
   votes and no other exactly matching mapping, mismatches below a thousandth
   of the flow (counters) are ignored.
6. **Rows**: candidate holders written under the base get their last persisted
   word. If that write's frame has no `Transfer`/`Deposit`/`Withdrawal`/`Mint`/`Burn`
   naming a holder other than the contract itself (a flag such as FiatToken's
   blacklist bit, an admin or a sync write), that holder needs its own keyed
   `balanceOf`. A candidate without a write gets the value of a keyed
   `balanceOf` that read only the base. Everyone else stays unknown; nothing
   becomes zero.

## Producer releases

`Block.ver` alone does not identify producer semantics. Producers on
firehose-tracer 5.5.0 and later, such as StreamingFast reth-bsc
v0.1.2-fh3.1-3 (released 2026-09-29), keep `Block.ver` 5 but change two
things this package reads
([#116](https://github.com/pinax-network/substreams-evm-balances/issues/116)):

- `Call.keccak_preimages` keeps only the preimages that explain a storage
  write of their transaction or system call. A `balanceOf` of a holder whose
  balance the transaction does not write usually records none.
- Once a transaction's internal calls pass 50 MiB of input, or 25 MiB of
  return data, later calls keep only their 4-byte selector and no return data.
  The flags that mark them (`Call` fields 35 and 36) are not in
  substreams-ethereum 0.12.0, so this package sees only the shape.

At the owner's direction (2026-10-06), an unkeyed read still excludes its
contract when it calls out or contradicts the stored word, but it never
chooses the base, vouches for a holder or supplies a value. A truncated-shape
`balanceOf` call drops its contract's rows for the block (rule 4). Each
qualification records the reader release (client and tracer version), not just
`Block.ver`.

Since v0.7.2 (owner, 2026-10-14), a chosen word at or above 2^128 drops its
contract's rows for the block. Such a word holds another field next to the
balance that did not move in the block, such as a packed timestamp or
FiatToken v2.2's blacklist flag in bit 255, so the transfer deltas still match
but the word is not the balance. A live check against `balanceOf`
(`scripts/check-live-balances.ts`, 2026-10-14) found such words in 3 of 768
contracts over 100 Ethereum blocks and 1 of 830 over 300 Base blocks. A token
whose real supply reaches 2^128 base units also loses its rows.

Since v0.7.3 the embedded [persistence rules](src/persist.rs), like
[`common/persist`](../../common/persist), recover the authorization boundary of
a failed SetCode (EIP-7702) transaction whose root call records `begin_ordinal`
0, as Arbitrum One's `Block.ver` 3 producer does for every root call: after the
sender's nonce record, each authorization's nonce and delegation code records
at consecutive ordinals (after the authority's creation when it is new), then
one ordinal that no record or call boundary takes. Any other shape still fails
the block. ERC-20 v0.7.2 failed Arbitrum One blocks 350,048,039, 400,015,655,
400,016,543, 450,044,117, 500,000,077 and 514,791,024 on it (its pre-release
noop failed in the 350M, 400M, 450M, 500M and head windows); v0.7.3 gives
v0.7.2's rows on every block v0.7.2 accepts. The captured transactions are the
`arbitrum-*-setcode.pb` fixtures in [`tests/fixtures/`](tests/fixtures/), with
their provenance in
[`arbitrum-setcode.json`](tests/fixtures/arbitrum-setcode.json); the
[native README](../../native/balances/README.md#failed-transactions) gives the
whole shape.

## Measured

Offline replays of captured Extended BSC blocks. Each interval is scored against
every row the RPC reference package `erc20-balances-v0.3.4.spkg` (`8aaa03b5…`)
emitted for it, that is, its per-block candidate holders whose `balanceOf` call
succeeded. Row recall is exact rows over reference rows; value precision is
exact over exact plus wrong. No interval had an extra row or an error block.

| Blocks | Reference rows (tokens) | Emitted | Exact | Wrong | Row recall | Value precision |
| --- | --- | --- | --- | --- | --- | --- |
| 123,561,000–123,562,023 | 538,758 (3,428) | 338,371 | 338,357 | 14 | 62.80% | 99.996% |
| 122,288,006–122,289,029 | 190,651 (1,696) | 112,708 | 112,705 | 3 | 59.12% | 99.997% |
| 120,607,788–120,608,043 | 122,668 (1,460) | 72,781 | 72,778 | 3 | 59.33% | 99.996% |
| 104,727,168–104,727,231 | 25,854 (708) | 16,210 | 16,197 | 13 | 62.65% | 99.920% |
| 122,264,480–122,264,543 | 19,353 (509) | 11,331 | 11,331 | 0 | 58.55% | 100% |

The rules were tuned on the first interval. The second was scored by earlier
review rounds, so it is not a pristine holdout; the last three are full captures
of the same reference package, scored once after the rules were frozen.

- **Precision by token group.** The 425 tokens that had qualified layouts until
  [9b41c7f](#history) got no wrong row in any interval. On every other token,
  value precision is 99.983% (95% Clopper–Pearson lower bound 99.972%) and
  99.963% (99.893%) in the first two intervals, and 99.938% (99.899%) pooled over
  the last three. The 99.95% target is therefore not established beyond the
  tuning interval. 13 of the 16 wrong rows in the last three intervals are one
  reward token in one block.
- **What the wrong rows are.** All 33 are computed balances in blocks that do not
  show it: tokens whose `balanceOf` returns a nonzero value for an empty stored
  word, and reward tokens whose `balanceOf` adds holder state that the block
  neither moves nor reads.
- **Why recall stops near 60%.** The reference re-emits holders whose balance
  did not change in the block (the token itself, `tx.from`, approvers); these
  stay unknown unless a traced `balanceOf` covers them. Tokens that emit events
  without persisted balance writes (event-only, fake-balance or computed-only)
  are never inferred.

**Simulated firehose-tracer 5.5.0 blocks.** The same captures, with each
transaction's and system call's preimages filtered as that tracer does (a
scratch port of its `keccak_filter.rs` at `70497b7`; no call reached the
truncation limits). The [producer-release](#producer-releases) rules leave the
table above unchanged row for row.

| Blocks | Emitted | Exact | Wrong | Row recall | Value precision |
| --- | --- | --- | --- | --- | --- |
| 123,561,000–123,562,023 | 337,930 | 337,910 | 20 | 62.72% | 99.994% |
| 122,288,006–122,289,029 | 112,615 | 112,610 | 5 | 59.07% | 99.996% |
| 120,607,788–120,608,043 | 72,587 | 72,584 | 3 | 59.17% | 99.996% |
| 104,727,168–104,727,231 | 16,214 | 16,199 | 15 | 62.66% | 99.907% |
| 122,264,480–122,264,543 | 11,320 | 11,320 | 0 | 58.49% | 100% |

- **Lost rows.** 511, 137, 214, 17 and 11 rows that only an unkeyed read
  supplied. While unkeyed reads vetoed their contract, recall fell to 26–40%.
- **Added rows.** 70, 44, 20, 21 and 0 rows of contracts that a second holder
  mapping's preimage used to exclude; 6, 2, 0, 2 and 0 of them are wrong. Their
  `balanceOf` also reads that mapping, but its preimage is filtered, so the read
  looks keyed to the base alone.

These results do not establish any token's semantics outside the tested blocks
and holders.

## Limits

- **No carried state.** Unchanged holders stay unknown unless read in the block.
  Because a later block can drop a contract, a consumer that keeps the latest
  emitted amount can hold a stale value. In the first two intervals, 38 and 14
  reference rows were preceded by a different emitted value for the pair, and
  the final amount differed for 15 of 167,959 and 5 of 70,303 pairs.
- **No persisted write, no rows.** A contract without a persisted write in a
  block gets no rows, however many holders its events name.
- **Computed balances.** Reward, reflection, rebasing, scaled and
  constant-default balances are excluded only when the block shows it. On
  5.5.0 blocks it shows less: a `balanceOf` that also reads a second holder
  mapping usually keeps only the base's preimage. Only a tracer option that
  keeps read preimages would restore that exclusion.
- **Certification by exact votes alone.** When the real balance key is not
  `pad(holder) || base` (struct offsets, Vyper ordering, assembly hashing),
  another mapping that moves exactly, such as a cumulative `received[holder]`,
  can be emitted instead. A packed word is emitted whole when its other fields
  never change, or change on no more of the block's touches than those that move
  the balance alone.
- **Polygon state syncs.** Polygon's `Block.ver` 3 state-sync transaction
  restarts the ordinals in each of its depth-1 calls
  ([native](../../native/balances/README.md#execution-order-v078)). A
  contract with a storage word that the transaction writes in two or more of
  those calls, such as WPOL or WETH, fails the chain check by ordinal and gets
  no rows in that block. On the nine Polygon blocks checked on 2026-10-14, no
  row was wrong (443 of 444 rows match `balanceOf`; the other is a computed
  balance from another transaction), and the dropped rows come back offline
  without the state-sync transaction. A write pattern that chains by ordinal
  to another final value remains possible; none was observed.
- **Guards from synthetic scenarios.** Inner mappings, dust votes,
  self-destruct, non-voting `Deposit`/`Mint`, large own mismatches and
  unexplained writes come from synthetic scenarios and change at most a few rows
  in these intervals.

## Build and Rust tests

```sh
cargo test --locked -p erc20-balances
make -C erc20/balances build schema   # release WASM and clickhouse/schema.sql
```

Committed spkgs come from CI's "Package hashes" artifact, not from a local
`make pack` (a macOS build has other module hashes than CI's Linux build).
Since v0.7.0 the package is `evm_balances_erc20`, with `map_events` and
`db_out`: `spkg/evm-balances-erc20-v0.7.3.spkg`, not live-qualified. v0.7.1
decodes the block with a 1 GiB element budget instead of buffa's 32 MiB
default, which BSC block 43,772,003 (29.9 MB encoded) failed; it gives the
same rows on every block v0.7.0 decodes. v0.7.2 drops a contract's rows for
the block when a chosen word reaches 2^128, and otherwise gives v0.7.1's rows;
it was not released, and its committed spkg embeds the earlier schema. v0.7.3
recovers the SetCode boundary of Arbitrum One's `Block.ver` 3
([producer releases](#producer-releases)) and otherwise gives v0.7.2's rows;
its ClickHouse history keeps only the 1h, 4h, 1d and 1w bars the Token API
serves (owner, 2026-10-14), and the replay check and repair work in 1-hour
windows. Before v0.7.0 it was `erc20_balances`:
`spkg/erc20-balances-v0.4.0.spkg` (package version v0.4.0; versions skip
v0.3.x, so no build can overwrite the v0.3.4 RPC reference). No Buf generation
is needed here; the public schema is already maintained by the shared `proto`
crate. The committed `spkg/erc20-balances-v0.1.0.spkg` (`532b571f…`,
2026-09-23) is the earlier layout package, which takes layouts as params; no
build writes to it. The inference source is packed (v0.2.0 and v0.4.0, below).
v0.4.0 has run in the BSC shadow of the retired combined package since
2026-10-13, and later builds in noops, backfills and the 2026-10-14 live
checks; none of this is live qualification
([#124](https://github.com/pinax-network/substreams-evm-balances/issues/124)).
A build is a new artifact and inherits none of the layout package's checks.
`spkg/erc20-balances-v0.3.4.spkg` is the immutable RPC reference. See
[rename provenance](../../docs/migration.md#module-rename). The committed
`spkg/erc20-balances-v0.2.0.spkg` (module `6539de92…`, Rust 1.88) predates the
[producer-release](#producer-releases) rules; v0.4.0 is the first build with
them
([#118](https://github.com/pinax-network/substreams-evm-balances/issues/118)).
The committed v0.4.0 is CI's canonical Linux build: WASM `17a847aa…`, module
`d8a9db86…`, not live-qualified.

Thin Rust tests call the package functions directly. They cover the rules on
synthetic blocks, the embedded persistence rules, which blocks fail, the
manifest shape, and captured block 122260950 against its saved same-block RPC
values; `tests/db_out.rs` covers the ClickHouse rows, the manifest wiring and
the embedded schema. On block 122260950, all 18 recorded `balanceOf` checks
match, and so do the 101 rows inferred for USDT, WBNB, USDC and BTCB. Without
the 383 preimages a 5.5.0 producer would not record
([`bsc-122260950-5.5.0-dropped-preimages.json`](tests/fixtures/bsc-122260950-5.5.0-dropped-preimages.json)),
13 of its `balanceOf` calls lose their holder's preimage and all 164 rows stay
the same. The tests of the embedded persistence rules also cover the captured
Arbitrum One SetCode transactions.

## History

Until [9b41c7f](https://github.com/pinax-network/substreams-evm-balances/tree/9b41c7f/erc20/balances),
`map_events` took caller-qualified storage layouts as params and failed closed
on any unreviewed write. That commit has the layout code, its tests and
fixtures, and the qualification documents and evidence for 425 BSC tokens. Its
last packed build is the committed v0.1.0 SPKG (2026-09-23), which predates the
typed-path and enumerable-set source at that commit. On 2026-10-05 the owner
replaced layouts with inference, as v0.2.0.

On the five intervals above, the two approaches compare as follows:

- inference gave the same value wherever both emitted a row for those tokens;
- inference added rows that all equal the reference;
- the layouts failed closed in two blocks, where inference failed none;
- inference took about 3 ms instead of about 77 ms per live block (native p50);
- the layouts emitted 3,333 rows in the second interval that inference leaves
  out, 3,299 of them from two tokens without persisted balance writes (22, 5, 1
  and 3 in the other intervals).

The qualification host tools were removed earlier; see
[`6dade89`](https://github.com/pinax-network/substreams-evm-balances/tree/6dade8957887c0c278cfa8da6bef61b9cc22f534/erc20/balances/tools).
