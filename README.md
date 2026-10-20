# substreams-evm-balances

RPC-free EVM balances from Firehose **Extended** blocks, ready for ClickHouse
and, for native, a StreamingFast hosted store. Two packages, native and
ERC-20, each with its own release and database. The tables are the
substreams-evm ClickHouse tables, so the Token API reads them unchanged.

## What's included

| Package | Source | Emits | ClickHouse tables |
| --- | --- | --- | --- |
| **evm-balances-native** | [`native/balances`](native/balances/README.md) | The native balance of every account whose balance changed in the block, from the genesis allocations on | `native_balances`, `blocks`, `historical_native_balances` |
| **evm-balances-erc20** | [`erc20/balances`](erc20/balances/README.md) | ERC-20 balances inferred per block from `Transfer` flows and persisted storage, for every contract | `erc20_balances`, `blocks`, `historical_erc20_balances` |

Each package is one spkg with two modules:

- **The map**: the balances as protobuf. ERC-20's `map_events` emits
  `evm.balances.v1.Events`. Since v0.8.0, native's `map_sink_entries` emits
  `SinkEntries` for a StreamingFast
  [hosted store](native/balances/README.md#hosted-store-v080): one entry per
  changed account, keyed by its address, holding its
  `evm.balances.v1.NativeBalance`. The same projection is a plain Rust function
  (`native_balances::project`, `erc20_balances::run`) for programs that read
  Extended blocks without the Substreams runtime. ERC-20 v0.7.4 keeps `erc20_balances::run` unchanged
  and adds `erc20_balances::run_sized`, which also takes the byte length of
  the delivered block (`Any.value`), as `map_events` does.
- **`db_out`**: the same balances as ClickHouse rows. The spkg embeds the
  schema, so the SQL sink needs nothing else.

No RPC, no params, no token lists. A block without the required Extended data
is rejected; native also stops on a block it cannot explain instead of writing
a guess, while ERC-20 drops the rows it doubts and goes on.

## Use it

Download a package from [Releases](https://github.com/pinax-network/substreams-evm-balances/releases)
(tags `evm-balances-native-v<version>` and `evm-balances-erc20-v<version>`),
then set up a database for it and run the sink:

```sh
substreams-sink-sql setup "$SUBSTREAMS_SINK_DSN" evm-balances-native-v<version>.spkg
substreams-sink-sql run "$SUBSTREAMS_SINK_DSN" evm-balances-native-v<version>.spkg <start-block>: \
  -e bsc.substreams.pinax.network:443 --undo-buffer-size=5
```

Use one database per package and network, named
`<network>:evm-balances-native@<version>` and
`<network>:evm-balances-erc20@<version>`. The supported sink, the Token API
live-sink settings, reorgs and the cutover from the RPC-era database are in
[docs/clickhouse.md](docs/clickhouse.md).

## Status

Neither package is live-qualified yet. BSC comes first
([#124](https://github.com/pinax-network/substreams-evm-balances/issues/124)),
then the other Extended EVM networks
([#8](https://github.com/pinax-network/substreams-evm-balances/issues/8)).
From native v0.7.8 and ERC-20 v0.7.3, both packages support Ethereum, BSC,
Polygon, Arbitrum One, Base, Optimism, Unichain and HyperEVM; Optimism and
Arbitrum One start at the first whole 1,000-block segment of their Extended
blocks ([start blocks](native/balances/README.md#input-and-networks)).
ERC-20 balances are inferred: their coverage and precision are
[measured](erc20/balances/README.md#measured), not guaranteed. The current
state is in [docs/handoff.md](docs/handoff.md), and the plan in the
[roadmap](https://github.com/pinax-network/substreams-evm-balances/issues/21).

## Also in this repository

- Protocol balance-state packages for Aave, Compound v2 and III, Lido and
  ERC-4626 vaults (`*/balance-state`). They emit the companion
  `evm.balance_state.v1` ([contract](docs/balance-state-contract.md)) and are
  not part of the ClickHouse output.
- Shared crates: `proto/` (the protobuf types), `common/persist/` (which state
  changes of an Extended block persisted) and `common/epochs/`.

Related repositories:
[substreams-evm-dex](https://github.com/pinax-network/substreams-evm-dex) (swaps and pool state),
[substreams-evm-transfers](https://github.com/pinax-network/substreams-evm-transfers) (transfer events) and
[substreams-evm](https://github.com/pinax-network/substreams-evm) (the RPC-based modules).
This repository was named `substreams-evm-extended` until 2026-10-13.

## Develop

```sh
cargo test --workspace --locked
make -C native/balances build schema
make -C erc20/balances build schema
```

Committed spkgs come from CI's Linux build, not from a local `make pack`.

To compare a package with the chain, stream recent blocks and check every
emitted balance against `balanceOf` (ERC-20) or `eth_getBalance` (native) at
the same block. It needs [Bun](https://bun.sh), the `substreams` CLI and
`SUBSTREAMS_API_KEY`; the RPC defaults to Pinax's for the chain:

```sh
bun scripts/check-live-balances.ts --chain eth --package erc20/balances/substreams.yaml --blocks 100
bun scripts/check-live-balances.ts --chain bsc --package spkg/evm-balances-native-v0.7.5.spkg --blocks 300 --out report.json
```

It prints the contracts whose balances differ, with an example each, and
exits 1 if any does. Rate-limited calls are retried in up to three slower
passes. Rows the RPC still cannot answer are reported as RPC errors, with the
package's value in the `--out` report, and do not change the exit code.

[AGENTS.md](AGENTS.md) has the repository rules, and
[docs/migration.md](docs/migration.md) the provenance of the packages.
