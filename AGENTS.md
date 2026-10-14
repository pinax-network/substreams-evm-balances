# Repository guidance

This repository, `pinax-network/substreams-evm-balances` (renamed from
`substreams-evm-extended` on 2026-10-13), contains the EVM balance Substreams
that consume Firehose **Extended** blocks. The Rust workspace members are the
shared protobuf crate, the shared `common/persist` persisted-effect rules, the
`common/epochs` schedule helper, and the packages `erc20/balances`,
`native/balances`, `aave/balance-state`, `compound-v2/balance-state`,
`compound-v3/balance-state`, `lido/balance-state` and `erc4626/balance-state`.
At the owner's direction (2026-10-13) DEX code lives in
`pinax-network/substreams-evm-dex` and transfer events in
`pinax-network/substreams-evm-transfers`: `dex/pool-state` and `erc20/events`
moved there with their history, and `evm/executions` and `aave/actions` were
removed. All four remain in this repository's history at `ca00aba`.

The repository holds Substreams package code only: map crates (the two
balance packages with their `db_out`), manifests, SQL, Makefiles, regression
tests and fixtures. Host-side diagnostic tools, reference models and harnesses were
removed on 2026-10-03. They remain in git history at `6dade89`, which the
dated evidence documents refer to. Do not add new tool crates or CLI harnesses.

## Production boundary

- Keep one RPC-free `map_events` module per map package and the shared
  `evm.balances.v1` output for balance packages: `Events` for ERC-20 and, at
  the owner's direction (2026-10-13), `NativeEvents` for native, which is
  `Events` without the always-absent contract (field 1 reserved, the same
  bytes). Preserve the canonical balances protobuf field numbers and wire
  types; do not introduce a custom storage protobuf or map cache. Protocol balance-state packages emit the
  companion `evm.balance_state.v1.Events` instead, never a replacement for
  `Balance.amount`. `erc20/balances` keeps its embedded copy of the
  persistence rules; `common/persist` must not diverge from it.
- Require Extended block data. The protocol balance-state packages also
  require explicit, independently qualified layouts and
  bindings (markets, epochs, vaults, pools). Except in `erc20/balances` (next
  item), unknown writes and unreviewed runtime or dependency changes must fail
  closed.
- `erc20/balances` is inference only, at the owner's direction (2026-10-05):
  `map_events(block)` takes no params or layouts and infers transfer-guided
  balances for every contract from the block's persisted storage. It stays
  RPC-free with bounded per-block work. A block fails only when it is not a
  complete Extended block or the persistence rules cannot resolve it; inference
  never fails a block and drops a contract's rows for the block on any doubt.
  Undetected computed balances remain possible: its rows are measured, not
  qualified. The former layout path, with its tests, fixtures and qualification
  evidence, is in git history at `9b41c7f`. Its last packed build is the
  committed `spkg/erc20-balances-v0.1.0.spkg` (2026-09-23), which predates the
  typed-path and enumerable-set source at `9b41c7f`. The inference package was
  `erc20_balances` v0.4.0 (v0.2.0 until the Rust 1.99.0 batch, #118); versions
  skipped v0.3.x so that no build can overwrite the RPC reference
  `spkg/erc20-balances-v0.3.4.spkg`. Since v0.7.0 it is `evm_balances_erc20`.
- `Block.ver` alone does not identify producer semantics. Producers on
  firehose-tracer 5.5.0 and later keep `Block.ver` 5 but record only the Keccak
  preimages that explain a storage write, and cut call data past
  per-transaction limits (#116). At the owner's direction (2026-10-06),
  `erc20/balances` lets a `balanceOf` without a `pad(holder) || base` preimage
  only veto (on a call out or a contradicted stored word), and drops a
  contract's rows for the block on a successful `balanceOf` call without a
  36-byte input or return data. Each qualification records the reader release
  (client and tracer version), not just `Block.ver`.
- The two balance packages each ship their `map_events` and a `db_out` in one
  spkg (owner, 2026-10-13): `native/balances` is `evm_balances_native`, written
  to `<chain>:evm-balances-native@<version>`, and `erc20/balances` is
  `evm_balances_erc20`, written to `<chain>:evm-balances-erc20@<version>`.
  Each version and database moves on its own, so an ERC-20 change never
  resyncs native, and a native refusal never stops ERC-20. The combined
  `evm-balances` package (one `db_out` for both, until v0.6.1, 2026-10-05 to
  2026-10-13) is retired; its source is at `5101e32`. Each `db_out` writes its
  half of the substreams-evm ClickHouse tables with the row format of upstream
  `db_out` at substreams-evm@`cb8607f`: tables, keys, columns and value
  encoding. It encodes them with `substreams-database-change` 5.0.0, where
  every field also carries `update_op = UPDATE_OP_SET`; upstream's 3.0.0 has no
  such field, and the SQL sink reads only a CREATE field's name and value. Each
  package's `clickhouse/schema.*.sql` are upstream's files byte for byte,
  except its half of `schema.1.table.balances.sql` (the two halves rejoin it);
  its manifest's `sink:` section embeds their concatenation. Each package keeps
  its own copy of the row helpers. A `db_out` adds no store, params or RPC. Do
  not add other `db_out` or database-change modules, or custom sinks. The map
  outputs remain the contract for external native sinks; local sink state
  belongs under `out/`. The sinks run with `--undo-buffer-size`, as the Token
  API live sinks do (BSC: 5), not `--final-blocks-only` (owner, 2026-10-13): a
  fork deeper than the buffer stops the sink and leaves the rows it wrote for
  the orphaned blocks, an accepted risk. See `docs/clickhouse.md`.
- `native/balances` is params-free at the owner's direction (2026-10-06):
  `map_events(block)` has no producer-version gate or manifest network allowlist.
  Keep its complete Extended-block, failed-transaction reason and persisted
  balance continuity checks. Its only continuity exceptions are recognized
  producer shapes with a captured fixture: the self-transfer pair (v0.2.2),
  the `Block.ver` 3 fee pool payout (v0.7.0, v0.7.1), the Ethereum
  EIP-6780 self-destruct debit (v0.7.3) and self-destruct to itself
  (v0.7.5). v0.7.2 also persists
  a failed blob transaction's blob fee, which the shared rules drop (BNB
  Chain pays it whatever the outcome; captured fixture). Since v0.7.0 block 0
  emits the genesis allocations when every balance record in it is a
  `GENESIS_BALANCE` from zero. Accepting a block is not network qualification; network-specific
  persistence work remains separate. Both packages default to `network: bsc`
  for routing.
- Host-side qualification can use RPC. Production balance processing cannot.

## Evidence

- **Live use** (owner's rule, #121). Every live Substreams, Firehose, RPC or
  sink step needs the owner's explicit OK for that step. Endpoints that are
  available (BSC resumed on 2026-09-22), an earlier OK or a historical report
  authorize nothing new. Keep credentials (`SUBSTREAMS_API_KEY`, RPC URLs,
  sink DSNs) in environment variables only, and never print access-bearing
  endpoint URLs or secrets into logs or evidence. Other documents point here
  instead of restating this rule.
- Verify new work ad hoc (for example in clickhouse-local or with scratch
  queries), and report the method and results in the pull request. Keep
  regression tests in Rust inside the package crates.
- Preserve captured fixtures, source/runtime bindings, failed attempts and
  historical evidence: in the tree, or at a cited commit when the owner removes
  a package path (the `erc20/balances` layout path: `9b41c7f`; the per-package
  CLI native-sink ClickHouse paths: `cb62110`; `dex/pool-state`,
  `erc20/events`, `evm/executions` and `aave/actions`: `ca00aba`). Use fresh
  output directories for new live checks.
- Compare with the immutable canonical RPC reference package. Distinguish newly
  built packages from historical package digests rather than rewriting evidence.
- Coverage claims must state the tested interval and initialized observed-holder
  set. Matching samples do not establish universal token or global-holder support.

## Validation

Use the pinned Rust toolchain and lockfile. `rust-toolchain.toml` pins Rust
1.99.0 exactly, with rustfmt, Clippy and the wasm32 target, and CI installs
from it, so there is no second pin. Pin a point release, never a minor
version: the WASM embeds `/rustc/<commit>`, so a new point release changes
every module hash. Offline validation consists of formatting, workspace
library/binary tests, Clippy for all targets on the host and wasm32 (only
wasm32 compiles the handlers), a WASM workspace check and rustdoc, all with
warnings denied. Live RPC, stream and holder checks are separate from offline
CI.

CI's "Package hashes" job builds `native/balances` and `erc20/balances` on
Linux through their Makefiles, with the checksum-pinned substreams CLI
v1.22.0. It fails unless each committed `spkg/<name>-<version>.spkg` carries
the module hashes of its source, and each embedded schema equals its
`make -C <package> schema`. A PR that changes one of these WASMs therefore
bumps its version and commits the repacked spkg in the same PR. That job's Linux build is canonical (2026-10-06): a macOS build
of the same source gives other module hashes, because cargo's symbol
metadata includes the host and LTO then merges functions differently. Take
committed spkgs from the job's `packages` artifact, not from a local pack.
Compare module hashes, never spkg sha256: an edited README changes the spkg
bytes, and with substreams CLI v1.22.0 so does every pack of a manifest with
several `networks:` entries (streamingfast/substreams#978). Today's
single-network packages do pack byte-identically across CI runs. Each balance
package is released on its own (owner, 2026-10-13), by a tag equal to its spkg
name, `evm-balances-native-v<version>` or `evm-balances-erc20-v<version>`, cut
after the spkg is committed. `release.yml` publishes the CI-built spkg with the
notes of `.github/release-notes/<native|erc20>.md`: what is included and how to
deploy it, no hash file (README, Releases). The plain `v<version>` tags and the
`evm-balances` releases before v0.7.0 belong to the retired combined package. A
release states that it is not live-qualified unless dated evidence names its
module hashes.

The workspace is on substreams 0.8.0, substreams-ethereum 0.12.0 and buffa
0.9.2 (2026-10-05). `proto/src/pb` is generated from `proto/v1` by `buf
generate` in `proto/` (`buf.gen.yaml`: the `buf.build/anthropics/buffa` plugin
v0.9.2, the version and options the SDK crates use); regenerate it rather than
editing it. Only `pb/mod.rs` is written by hand. A `db_out` links every
message of the `proto` crate into its WASM through buffa's `json` feature, so
`evm.balance_state.v1` sits behind the crate's default `balance-state`
feature, which the two balance packages turn off: an edit to the balance-state
messages then cannot change their hashes. The types of the packages that left
on 2026-10-13 were removed with v0.7.0. The SQL sink's descriptors are the
committed `spkg/evm-balances-sink-protodefs-v0.1.0.spkg`, built from
`proto/sink`.

Package Makefiles build WASM with `--locked` and the `RUSTFLAGS` of the
root `build.mk`, which remap the checkout, `CARGO_HOME` and the Rust sources
(`--remap-path-prefix`). WASM and module hashes then depend only on the source
and the pinned toolchain, not on the checkout directory, the user, or whether
rust-src is installed, and RUSTFLAGS set in the shell are ignored. Keep this
in every WASM build. Handlers
decode their input with buffa's default limits
(32 MiB of repeated elements per block); the measured headroom is in the
dated [substreams 0.8.0 section](docs/history/handoff-2026-09-21-to-2026-10-06.md#substreams-080-2026-10-05)
of the 2026-09-21 to 2026-10-06 handoff. The exception is `map_events` in
`native/balances` (since v0.7.4) and `erc20/balances` (since v0.7.1): it
decodes its block itself with a 1 GiB element budget (`decode_block`),
because BSC block 43,772,003 (29.9 MB encoded) failed the default decode.
