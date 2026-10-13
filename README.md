# substreams-evm-balances

EVM balance Substreams from Firehose Extended blocks: native and ERC-20
balances, each with a `db_out` into the Token API's ClickHouse tables, and the
protocol balance-state packages. Formerly `substreams-evm-extended`
(renamed 2026-10-13). Its siblings are
[substreams-evm-dex](https://github.com/pinax-network/substreams-evm-dex) (swaps
and pool state) and
[substreams-evm-transfers](https://github.com/pinax-network/substreams-evm-transfers)
(transfer events); the RPC-based modules stay in
[substreams-evm](https://github.com/pinax-network/substreams-evm).

Two packages carry the balances, each with an RPC-free `map_events(block)`
and a `db_out` in one spkg, released and synced on their own
([ClickHouse](docs/clickhouse.md)):

- [`evm-balances-native`](native/balances/README.md) (`native/balances`):
  `map_events` emits `evm.balances.v1.NativeEvents`, the final persisted
  native balance of every account changed in the block, from the genesis
  allocations at block 0 on. It ports the historical RPC-free native reducer;
  an earlier build was live-qualified on BSC on 2026-09-23 (WASM `48d89d28…`,
  module `5a2a2e0c…`), no later build is. `db_out` writes `native_balances`,
  `blocks` and the native history into `<chain>:evm-balances-native@<version>`.
- [`evm-balances-erc20`](erc20/balances/README.md) (`erc20/balances`):
  `map_events` infers each block's ERC-20 balances from `Transfer` flows and
  persisted storage changes, with no params or layouts, and emits
  `evm.balances.v1.Events`, the exact protobuf of the RPC balance
  implementation. `db_out` writes `erc20_balances`, `blocks` and the ERC-20
  history into `<chain>:evm-balances-erc20@<version>`.

Blocks without the required Extended data are rejected. The tables are the
substreams-evm ClickHouse tables, with upstream's SQL embedded in each spkg, so
the Token API reads them unchanged. Until v0.6.1 one combined `evm-balances`
package wrote both into one database (source at `5101e32`); the earlier
per-package CLI native-sink paths are at
[`cb62110`](https://github.com/pinax-network/substreams-evm-balances/tree/cb6211007f1f9d0cb5852a6b666da475a8fa6542).

The committed packages are CI's canonical Linux builds (Rust 1.99.0,
substreams CLI v1.22.0; identify a build by these hashes, never by spkg
sha256). None is live-qualified.

| Package | Module | Module hash | WASM sha256 |
| --- | --- | --- | --- |
| `spkg/evm-balances-native-v0.7.0.spkg` | `map_events` | `0759418fb023485e9ca06bfe7a0c1c5c0d2c4c63` | `c448342379378455…` |
| | `db_out` | `29f7d1596fc6454ce5df7bcdf53636e11a533cf8` | same WASM |
| `spkg/evm-balances-erc20-v0.7.0.spkg` | `map_events` | `65629a0baac9167970079510489900648822fce5` | `a2c12a1284ad5713…` |
| | `db_out` | `73627870ed1c72a9b76ad69f46fa94f9979afbb4` | same WASM |

Each spkg embeds its half of the schema (native `0566d859…`, ERC-20
`ad492406…`). The last builds before the split are native v0.2.2 (`378d27a6…`),
erc20 v0.4.0 (`d8a9db86…`) and the combined `evm-balances` v0.6.1 (`db_out`
`b3313b50…`).

Picking this up? Start with [`docs/handoff.md`](docs/handoff.md) (package
status, hashes and open work; the dated record is under `docs/history/`) and
the procedures in [`skills/`](skills/README.md).

## Layout

The schemas stay in the repository-root `proto/` crate, so every package
shares one protobuf contract without copying generated types:

- `proto/v1/balances.proto`: canonical balance schema.
- `proto/v1/balance_state.proto`: versioned holder/global balance-state
  companion schema for protocol packages ([contract](docs/balance-state-contract.md)).
- `proto/src/pb/`: shared generated Rust types (buffa, from `buf generate` in
  `proto/`). `evm.balance_state.v1` sits behind the default `balance-state`
  feature, which the two balance packages turn off ([AGENTS.md](AGENTS.md)).
- `proto/sink/`: sources of the SQL sink's pinned descriptors,
  `spkg/evm-balances-sink-protodefs-v0.1.0.spkg`.

Shared crates and packages:

- `common/persist/`: shared persisted-effect rules for Extended blocks.
- `common/epochs/`: epoch-schedule helper for the protocol balance-state packages.
- `erc20/balances/`: Extended-block ERC-20 balances and their `db_out` (`evm-balances-erc20`).
- `native/balances/`: Extended-block native balances and their `db_out` (`evm-balances-native`).
- `aave/balance-state/`: Aave V3 aToken holder basis and reserve state module.
- `compound-v2/balance-state/`: Compound v2 cToken shares, market words, cash and rate-model dependency module.
- `erc4626/balance-state/`: ERC-4626 vault shares, total supply and source-bound conversion inputs (Aave static aToken, Savings DAI, OpenZeppelin).
- `lido/balance-state/`: Lido stETH holder shares, packed global words, derived pooled ether and report evidence module.
- `compound-v3/balance-state/`: Compound III (Comet) signed principal and market index module.

Each map's handler wraps a plain Rust function (for example
`native_balances::project` and `erc20_balances::run`), so host programs such
as firehose-parquet can run the same projection on decoded Extended blocks
without the Substreams runtime.

`dex/pool-state` and `erc20/events` moved with their history to
[substreams-evm-dex](https://github.com/pinax-network/substreams-evm-dex) and
[substreams-evm-transfers](https://github.com/pinax-network/substreams-evm-transfers);
`evm/executions` and `aave/actions` were removed. All four remain here at
[`ca00aba`](https://github.com/pinax-network/substreams-evm-balances/tree/ca00aba).

## Build and test

The pinned Rust toolchain includes the WASM target. The packages use
substreams 0.8.0, substreams-ethereum 0.12.0 and buffa 0.9.2. Building
packages also requires the Substreams CLI.

```sh
cargo test --workspace --lib --bins --tests --locked
make -C native/balances build schema
make -C erc20/balances build schema
```

`make pack` writes into `spkg/`. Only CI's Linux build gives the committed
module hashes, so a local pack on another host changes the committed spkg;
restore it with `git checkout -- spkg/` and take spkgs from CI
([`AGENTS.md` (Validation)](AGENTS.md#validation)).

The repository holds package code only; regression tests are Rust, inside the
package crates. The host tools that produced the qualification evidence
(packaged-output comparison with hash-pinned RPC results, initialized-holder
checks, offline replays, reference models) were removed on 2026-10-03. The
dated evidence documents refer to them as they were at
[`6dade89`](https://github.com/pinax-network/substreams-evm-balances/tree/6dade8957887c0c278cfa8da6bef61b9cc22f534).
RPC is used for qualification, not inside the maps.

## Releases

Each balance package is released on its own, by a tag equal to its spkg name:
`evm-balances-native-v<version>` or `evm-balances-erc20-v<version>`. The
release publishes that spkg; the
[Token API live sinks](docs/clickhouse.md#token-api-live-sinks-bsc) take its
URL as `SPKG_URL`. One version names one module hash: a change to a package's
WASM bumps its version and commits the spkg that CI's "Package hashes" job
built. The tag then runs [`release.yml`](.github/workflows/release.yml), which:

- builds and packs the package on Linux, CI's canonical builder;
- requires `substreams registry verify` to pass with no warning, the module
  hashes of the committed spkg and the embedded schema of `make schema`;
- records the module hashes, WASM sha256 and schema sha256 in the run's
  summary;
- publishes a GitHub release with the spkg and the notes of
  [`.github/release-notes/`](.github/release-notes/): what is included and how
  to deploy it.

The plain `v<version>` tags (`v0.6.1`) are the retired combined `evm-balances`
package's.

Identify a release by its module hashes, never by the spkg sha256: an edited
README changes the spkg bytes, and with substreams CLI v1.22.0 so does every
pack of a manifest with several `networks:` entries
([streamingfast/substreams#978](https://github.com/streamingfast/substreams/pull/978)).
A release is not live qualification;
its notes say so unless dated evidence names its module hashes.

## Coverage

ERC-20 balances are inferred, not configured. On five captured BSC intervals,
scored against every row the RPC reference package emitted for them, the
package reproduces 58.5–62.8% of the reference rows with 99.92–100% value
precision; the [measured results](erc20/balances/README.md#measured) give each
interval, its scored holder set and the limits. Matching intervals do not
establish universal ERC-20 support or global holder enumeration. The former
layout path covered 431 explicitly qualified profiles among the first 450 BSC
candidates ranked by activity; its code, coverage reports and evidence remain at
[`9b41c7f`](https://github.com/pinax-network/substreams-evm-balances/tree/9b41c7f/erc20/balances).

Computed balances (reward, reflection, rebasing, scaled) remain open. Networks
other than BSC need their own qualification after the BSC work; the target
networks are tracked in
[#8](https://github.com/pinax-network/substreams-evm-balances/issues/8). The
repository name is not a claim of verified coverage on every EVM network.

The inference runs in the BSC shadow deployment of 2026-10-13
(`bsc:evm-balances@v0.6.1`, from block 127,000,000): its first FINAL
comparison with the RPC-era database, over blocks 127,000,000–127,460,000,
found 1,592 wrong balances among 10,223,501 same-block pairs (precision
0.999844). Its BSC qualification is
[#124](https://github.com/pinax-network/substreams-evm-balances/issues/124), and
each live step needs the owner's OK ([live use](AGENTS.md#evidence)).

Outstanding implementation, holder coverage, packaging and network work is
tracked in [GitHub issues](https://github.com/pinax-network/substreams-evm-balances/issues),
under the roadmap [#21](https://github.com/pinax-network/substreams-evm-balances/issues/21).
The requested chains, asset scopes, protocol deployments and action-evidence
mapping for the extraction roadmap are recorded in
[extraction coverage](docs/extraction-coverage.md).

See [migration provenance](docs/migration.md) for the original PR, preserved
schema/package digests and the distinction between historical qualification
and the new repository's build and sink checks.
