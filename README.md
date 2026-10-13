# substreams-evm-balances

EVM balance Substreams from Firehose Extended blocks: native and ERC-20
balances, the protocol balance-state packages, and the `evm-balances` `db_out`
into the Token API's ClickHouse tables. Formerly `substreams-evm-extended`
(renamed 2026-10-13). Its siblings are
[substreams-evm-dex](https://github.com/pinax-network/substreams-evm-dex) (swaps
and pool state) and
[substreams-evm-transfers](https://github.com/pinax-network/substreams-evm-transfers)
(transfer events); the RPC-based modules stay in
[substreams-evm](https://github.com/pinax-network/substreams-evm).

The first module is [ERC-20 balances](erc20/balances/README.md). One RPC-free
`map_events(block)` infers each block's ERC-20 balances from `Transfer` flows
and persisted storage changes, with no params or layouts, and emits
`evm.balances.v1.Events`, with the exact protobuf used by the RPC balance
implementation. Blocks without the required Extended data are rejected.

[Native balances](native/balances/README.md) is a second one-map package with
the same protobuf: `Balance.contract` is absent and `amount` is the final
persisted native balance of every account changed in the block. It ports the
historical RPC-free native reducer. It was live-qualified on BSC on 2026-09-23
(WASM `48d89d28…`, module `5a2a2e0c…`); no later build is.

[`evm-balances`](evm-balances/README.md) imports both packages and writes the
substreams-evm ClickHouse tables (`blocks`, `erc20_balances`,
`native_balances` and the historical OHLC views) through one `db_out`, with
upstream's SQL byte for byte embedded in its spkg. It is the workspace's only
`db_out` and database-change dependency. The earlier per-package CLI
native-sink paths are at
[`cb62110`](https://github.com/pinax-network/substreams-evm-balances/tree/cb6211007f1f9d0cb5852a6b666da475a8fa6542).

The committed packages are CI's canonical Linux builds (Rust 1.99.0,
substreams CLI v1.22.0; identify a build by these hashes, never by spkg
sha256). None is live-qualified.

| Package | Module | Module hash | WASM sha256 |
| --- | --- | --- | --- |
| `spkg/native-balances-v0.2.2.spkg` | `map_events` | `378d27a630f0f2c8589f4396b308edff9c981ffd` | `8fc5a84901b49857…` |
| `spkg/erc20-balances-v0.4.0.spkg` | `map_events` | `d8a9db86240ca6466270f140798fae0b8db05f7b` | `17a847aa77346058…` |
| `spkg/evm-balances-v0.6.1.spkg` | `db_out` | `b3313b50a193a78e295350fc6e83b77d3b0fd5c8` | `1e6520fab2764d44…` |

`evm-balances` imports the two maps with the same module hashes, and embeds
the schema `ed1c3bff…`.

Picking this up? Start with [`docs/handoff.md`](docs/handoff.md) (package
status, hashes and open work; the dated record is under `docs/history/`) and
the procedures in [`skills/`](skills/README.md).

## Layout

The schemas stay in the repository-root `proto/` crate, so every package
shares one protobuf contract without copying generated types:

- `proto/v1/balances.proto`: canonical balance schema.
- `proto/v1/balance_state.proto`: versioned holder/global balance-state
  companion schema for protocol packages ([contract](docs/balance-state-contract.md)).
- `proto/src/pb/`: shared generated Rust types (buffa, from `buf generate` in `proto/`).
- `proto/v1/executions.proto`, `erc20_events.proto`, `aave_actions.proto`,
  `dex-pool-state.proto` and `dex/`: types of packages that left on
  2026-10-13, kept only so `evm-balances`' `db_out` keeps v0.6.1's hash (see
  [AGENTS.md](AGENTS.md)); removed with the next hash-changing release.

Shared crates and packages:

- `common/persist/`: shared persisted-effect rules for Extended blocks.
- `common/epochs/`: epoch-schedule helper for the protocol balance-state packages.
- `erc20/balances/`: Extended-block ERC-20 balance module.
- `native/balances/`: Extended-block native balance module.
- [`evm-balances/`](evm-balances/README.md): one `db_out` from `native/balances` and `erc20/balances` into the substreams-evm ClickHouse schema, which its spkg embeds.
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
make -C erc20/balances build
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

Only `evm-balances` is released, by tag `v<version>` equal to its manifest's
version. The release publishes `evm-balances-v<version>.spkg` next to the
`native/balances` and `erc20/balances` spkgs it imports; the
[Token API live sink](evm-balances/README.md#token-api-live-sink-bsc) takes
its URL as `SPKG_URL`. One version names one module hash: a change
to a package's WASM bumps its version and commits the spkg that CI's "Package
hashes" job built, and a change to an imported map also changes the `db_out`
hash, so it bumps `evm-balances` too. The tag then runs
[`release.yml`](.github/workflows/release.yml), which:

- builds and packs the three packages on Linux, CI's canonical builder;
- requires `substreams registry verify` to pass with no warning, the module
  hashes of each committed spkg, map packages equal to the maps inside
  `evm-balances`, and the embedded schema of `make schema`;
- records the module hashes, WASM sha256 and schema sha256 in the run's
  summary;
- publishes a GitHub release with the three spkgs and the notes of
  [`.github/release-notes.md`](.github/release-notes.md): what is included and
  how to deploy it.

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

The inference package is packed and committed (`spkg/erc20-balances-v0.4.0.spkg`,
above) but has only been checked offline against saved data: it has not run in
a Substreams engine. Its BSC qualification is
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
