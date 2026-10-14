# Handoff: current state

Where the repository stands on 2026-10-13, for whoever picks it up next. Keep
this page short and current: when a package's version, hashes or
qualification change, update its row here, and put the dated evidence in the
pull request and the package's evidence files. The rules are in
[`AGENTS.md`](../AGENTS.md), which wins where the two disagree; the procedures
are in [`skills/`](../skills/README.md).

## Live use

Each live Substreams, Firehose, RPC or sink step needs the owner's explicit OK
for that step. The rule, with the credential handling, is in
[`AGENTS.md` (Evidence)](../AGENTS.md#evidence); nothing on this page
authorizes a live step.

## Balance packages

Since v0.7.0 (owner, 2026-10-13) each balance package ships its `map_events`
and its own `db_out` in one spkg, is released by its own tag and writes its
own ClickHouse database; [ClickHouse](clickhouse.md) has the tables, the
deployment and the reasons. Full-history BSC noops of both packages are
running; no sink runs them yet.

| Package | spkg and release tag | BSC database | `map_events` output |
| --- | --- | --- | --- |
| [`native/balances`](../native/balances/README.md) | `evm-balances-native-v<version>` | `bsc:evm-balances-native@<version>` | `evm.balances.v1.NativeEvents` |
| [`erc20/balances`](../erc20/balances/README.md) | `evm-balances-erc20-v<version>` | `bsc:evm-balances-erc20@<version>` | `evm.balances.v1.Events` |

Both are on the [Releases](https://github.com/pinax-network/substreams-evm-balances/releases) page; each release's run summary records
its module, WASM and schema hashes. The committed spkgs are CI's canonical
Linux builds (Rust 1.99.0, substreams CLI v1.22.0), taken from the "Package
hashes" job's `packages` artifact. Identify a build by its module hash
(`substreams info`), never by the spkg sha256. None is live-qualified.

- **`native/balances`** v0.7.0 adds `db_out` and emits `NativeEvents`,
  `Events` without the contract, which native amounts never have (field 1
  reserved, the same bytes). It restores the BSC `Block.ver` 3 fee pool
  payout, on which native v0.2.2 refused blocks 363, 12,700,000 and
  25,401,003 (v0.7.1 also accepts the payout credit after a skipped ordinal,
  as in blocks 958 and 4104), and accepts block 0, where it emits the genesis
  allocations ([README](../native/balances/README.md#genesis-v070)). v0.7.2
  persists a failed blob transaction's blob fee, on which v0.7.0 refused
  block 42,357,369, and v0.7.3 restores the Ethereum EIP-6780 self-destruct
  debit, on which v0.7.2 refused Ethereum block 20,249,174
  ([matrix](../native/balances/docs/persisted-effects.md#reason-matrix)).
  v0.7.4 decodes the block with a 1 GiB element budget instead of buffa's
  32 MiB default, which BSC block 43,772,003 (29.9 MB encoded) failed, and
  v0.7.5 keeps the balance of an account that self-destructs to itself after
  Cancun, on which v0.7.3 refused Ethereum block 19,481,732, and v0.7.6
  carries a self-destruct credit from an account created in the transaction
  into the beneficiary's later records in it, on which v0.7.5 refused
  Ethereum block 25,741,008. Not qualified. The
  last qualified build is historical: the prost build was live-qualified on
  BSC on 2026-09-23 (WASM `48d89d28…`, module `5a2a2e0c…`), for the accounts
  it emitted in the stated windows
  ([evidence](../native/balances/README.md#live-qualification-bsc-2026-09-23)).
  No build since then is qualified.
- **`erc20/balances`** v0.7.0 adds `db_out`; its inference is v0.4.0's.
  v0.7.1 decodes the block with the same 1 GiB element budget as native
  v0.7.4; the ERC-20 v0.7.0 noop failed to decode BSC block 43,772,003. Not
  qualified. Offline, on five captured BSC intervals scored against every row
  the RPC reference package emitted for them, it reproduces 58.5–62.8% of
  those rows with 99.92–100% value precision
  ([measured](../erc20/balances/README.md#measured) gives each interval and
  its scored holders). v0.4.0 runs in the BSC shadow below. The earlier
  layout package v0.1.0, another source, was checked on live BSC on
  2026-09-23
  ([report at `9b41c7f`](https://github.com/pinax-network/substreams-evm-balances/blob/9b41c7f/erc20/balances/docs/live-package-bsc-2026-09-23.md)).

The combined `evm-balances` package (v0.4.0 to v0.6.1: one `db_out` for both
tables in one database, released by plain `v<version>` tags) is retired.
v0.6.1 is its last release, and its source is at
[`5101e32`](https://github.com/pinax-network/substreams-evm-balances/tree/5101e327b47d63c1b1740c88a79e4e6841fb7650/evm-balances).
Older versions under `spkg/` are historical builds: native v0.1.0 to v0.2.2
and erc20 v0.1.0, v0.2.0 and v0.4.0, which the package READMEs describe, and
evm-balances v0.4.0 to v0.6.1, described at that commit.
`spkg/erc20-balances-v0.3.4.spkg` is the immutable RPC reference
([migration](migration.md)); versions skip v0.3.x so that no build can
overwrite it.

## Current deployment (BSC)

The retired combined package v0.6.1 has run on BSC as a shadow since
2026-10-13:
release `v0.6.1`, database `bsc:evm-balances@v0.6.1`, filled by a backfill
run of the sink from block 127,000,000 and by a live sink from block
127,455,000 that runs with the Token API's BSC flags, undo buffer 5
([Token API live sinks](clickhouse.md#token-api-live-sinks-bsc)). The Token
API still reads the RPC-era `bsc:evm-balances@v0.3.x`. The shadow runs these
CI builds:

| Package | Version and spkg | Module | Module hash | WASM sha256 |
| --- | --- | --- | --- | --- |
| `native/balances` | v0.2.2, `spkg/native-balances-v0.2.2.spkg` | `map_events` | `378d27a630f0f2c8589f4396b308edff9c981ffd` | `8fc5a84901b498570efdf8cd54f5a077dc040b07ddc5bffa923016e2e909c235` |
| `erc20/balances` | v0.4.0, `spkg/erc20-balances-v0.4.0.spkg` | `map_events` | `d8a9db86240ca6466270f140798fae0b8db05f7b` | `17a847aa77346058a160553c1fe3f79ab42cc4c0c5de1e0f3997e3a7b28e05d1` |
| `evm-balances` | v0.6.1, `spkg/evm-balances-v0.6.1.spkg` | `db_out` | `b3313b50a193a78e295350fc6e83b77d3b0fd5c8` | `1e6520fab2764d445754140aa45081307aa624cc9b86750c8c12ddfb7490eba7` |

Its first FINAL comparison with the RPC-era database
([contract, section 8](initialization-and-completeness.md#8-final-comparison-with-the-rpc-era-database)),
over blocks 127,000,000–127,460,000, found native exact and 1,592 wrong ERC-20
balances among 10,223,501 same-block pairs (precision 0.999844)
([#124](https://github.com/pinax-network/substreams-evm-balances/issues/124)).
That is shadow evidence for v0.6.1, not live qualification. The two v0.7.0
sinks replace this shadow after their noops pass.

A noop of v0.6.1 over the full BSC history (2026-10-13) failed in every range
below block 38.1M; the ranges from 38.1M to 127M ran clean as far as they had
run that day. The captured refusals, blocks 363, 12,700,000 and 25,401,003,
are the `Block.ver` 3 fee pool payout that native v0.7.0 restores. A
production-mode stream from block 1 also runs block 0, because Substreams
processes whole 1,000-block segments; native v0.2.2 refused block 0, and
v0.7.0 accepts it. The v0.7.0 noops (2026-10-13) passed those blocks, but
native refused blocks 958 and 4104, where the producer skipped an ordinal
between the payout records; v0.7.1 accepts them. On 2026-10-14 native v0.7.0
refused block 42,357,369, whose failed blob transaction pays its blob fee to
the fee pool; v0.7.2 keeps that record. Native v0.7.2 refused Ethereum block
20,249,174, an EIP-6780 self-destruct that v0.7.3 debits, and the ERC-20
v0.7.0 noop failed to decode BSC block 43,772,003 (29.9 MB encoded) under the
default buffa limits of the SDK's handler macro, which native v0.7.3 shares;
native v0.7.4 and ERC-20 v0.7.1 decode it with a 1 GiB element budget.
Native v0.7.3 then refused Ethereum block 19,481,732, an EIP-6780
self-destruct to itself whose refund and withdraw v0.7.5 drops. Native v0.7.5
refused Ethereum block 25,741,008, where an account created in the
transaction self-destructs to the sender and the sender's gas refund starts
from the balance before the credit; v0.7.6 adds the credit to it.

## Next steps (owner)

Each step needs the owner's OK. Nothing is written to ClickHouse before the
package's noop over that range passes (owner, 2026-10-13).

1. Finish the full-history noops: ERC-20 v0.7.1, and native v0.7.5 from
   block 0 (the native versions up to v0.7.2 each accept every block the
   earlier ones accept, with the same rows: v0.7.1 adds the payout shape of
   blocks 958 and 4104, v0.7.2 failed blob transactions; v0.7.3 adds the
   EIP-6780 self-destruct debit; native v0.7.4 and ERC-20 v0.7.1 change only
   the decode budget, so they give the same rows on every block the previous
   version decodes; native v0.7.5 drops the self-destruct-to-itself pair,
   which v0.7.4 refuses, or, when it is the account's first record in the
   block, keeps as a wrong zero; native v0.7.6 shifts only records that
   v0.7.5 refuses, so it gives the same rows on every block v0.7.5 accepts).
   BSC continues native v0.7.5, which has refused no BSC block; Ethereum
   native moves to v0.7.6. Ethereum runs the same noops from genesis since 2026-10-14
   (owner), a faster full-history test of about 26M blocks; its databases
   would be `mainnet:evm-balances-native@<version>` and
   `mainnet:evm-balances-erc20@<version>`.
2. Per package, run the backfill (the sink over past blocks) and the live
   sink into its own database, `bsc:evm-balances-native@<version>` or
   `bsc:evm-balances-erc20@<version>`
   ([Token API live sinks](clickhouse.md#token-api-live-sinks-bsc)). They
   replace the v0.6.1 shadow.
3. Token API (pinax-api): set `balances-native` and `balances-erc20` in the
   network's `dbs-config.yaml` entry
   ([pinax-api#601](https://github.com/pinax-network/pinax-api/pull/601)),
   so that its native routes read the native database and its other routes
   the ERC-20 database; a side without its key keeps reading `balances`.
4. Cut over each database on its own
   ([cutover and backfill](clickhouse.md#cutover-and-backfill),
   [contract](initialization-and-completeness.md)).

## Other packages

None has a committed spkg or a recorded module hash for its current build,
and none is qualified at that build. Every package moved to substreams 0.8.0
on 2026-10-05, so the dated live evidence below covers earlier builds only;
each README states what its evidence covers.

| Package | Version | Default network | Evidence |
| --- | --- | --- | --- |
| `aave/balance-state` | v0.2.0 | `bsc` | [live, 2026-09-22](../aave/balance-state/README.md#live-qualification-bsc-2026-09-22) |
| `erc4626/balance-state` | v0.2.0 | `bsc` | [live static aToken, 2026-09-23](../erc4626/balance-state/README.md#live-qualification-bsc-static-atoken-2026-09-23) |
| `compound-v2/balance-state` | v0.2.0 | `mainnet` | synthetic tests and compiled layouts ([README](../compound-v2/balance-state/README.md)) |
| `compound-v3/balance-state` | v0.2.0 | `mainnet` | synthetic tests and compiled layouts ([README](../compound-v3/balance-state/README.md)) |
| `lido/balance-state` | v0.2.0 | `mainnet` | synthetic tests and compiled layouts ([README](../lido/balance-state/README.md)) |

`dex/pool-state` and `erc20/events` moved with their history to
[substreams-evm-dex](https://github.com/pinax-network/substreams-evm-dex) and
[substreams-evm-transfers](https://github.com/pinax-network/substreams-evm-transfers)
on 2026-10-13, and `evm/executions` and `aave/actions` were removed; their
last state here, with their evidence, is at `ca00aba`.

## Validation

[`AGENTS.md` (Validation)](../AGENTS.md#validation) describes the offline
checks, the toolchain pin and the "Package hashes" job. Run the commands of
the "Rust checks" job in [`ci.yml`](../.github/workflows/ci.yml) before a pull
request. A change to a balance package's WASM bumps its version and commits
the spkg from the job's `packages` artifact in the same pull request.
`substreams pack` also embeds the two balance packages' READMEs, so a README
edit changes their spkgs but not their module hashes. `make pack` writes
straight into `spkg/`; on a host other than CI's Linux it overwrites the
committed spkg with other module hashes, so restore it with
`git checkout -- spkg/`.

## History

- [`history/handoff-2026-09-21-to-2026-10-06.md`](history/handoff-2026-09-21-to-2026-10-06.md):
  this page's previous content, verbatim: the 2026-09-21 plan, the review and
  qualification record, and every dated section through 2026-10-06, including
  the measured buffa decode headroom. Its
  [section 3](history/handoff-2026-09-21-to-2026-10-06.md#3-where-the-artifacts-live)
  lists where the captured Extended blocks were kept; they are not in git.
- [`history/review-findings-2026-09-21.md`](history/review-findings-2026-09-21.md)
  and [`history/audit-remediation-2026-09-21.md`](history/audit-remediation-2026-09-21.md):
  the 2026-09-21 hardening review and the offline issue-audit remediation.
- [`6dade89`](https://github.com/pinax-network/substreams-evm-balances/tree/6dade8957887c0c278cfa8da6bef61b9cc22f534):
  the host tools, reference models and harnesses removed on 2026-10-03; the
  dated evidence refers to them.
- [`9b41c7f`](https://github.com/pinax-network/substreams-evm-balances/tree/9b41c7f/erc20/balances):
  the `erc20/balances` layout path with its tests, fixtures and qualification
  evidence, removed on 2026-10-05.
- [`cb62110`](https://github.com/pinax-network/substreams-evm-balances/tree/cb6211007f1f9d0cb5852a6b666da475a8fa6542):
  the per-package CLI native-sink ClickHouse paths, replaced by `evm-balances`
  on 2026-10-05.
- [`5101e32`](https://github.com/pinax-network/substreams-evm-balances/tree/5101e327b47d63c1b1740c88a79e4e6841fb7650/evm-balances):
  the combined `evm-balances` package (v0.4.0 to v0.6.1) with its README and
  example SQL, retired on 2026-10-13 when each balance package got its own
  `db_out` (v0.7.0).

## Open work

Open work is tracked in
[GitHub issues](https://github.com/pinax-network/substreams-evm-balances/issues),
under the roadmap
[#21](https://github.com/pinax-network/substreams-evm-balances/issues/21). On
the path to production:

- [#124](https://github.com/pinax-network/substreams-evm-balances/issues/124):
  qualify the balance packages above on BSC, then cut Token API over to their
  two databases. Every live step needs the owner's OK. The offline
  preconditions are met apart from the BSC reader release (#116).
- [#124](https://github.com/pinax-network/substreams-evm-balances/issues/124):
  BSC, per package: the full-history noop, the sync from genesis and the
  live sink, the shadow comparison, the Token API `balances-native` and
  `balances-erc20` settings,
  then the cutover. It also holds the BSC reader release and freeze from
  infra, and the rows-per-block alert in ops (the query is in
  [ClickHouse](clickhouse.md#monitoring)).
- [#123](https://github.com/pinax-network/substreams-evm-balances/issues/123):
  how Token API treats wrong, stale and missing inferred ERC-20 balances,
  decided before the ERC-20 cutover.
- [#116](https://github.com/pinax-network/substreams-evm-balances/issues/116):
  read `Call` fields 35/36 once a substreams-ethereum release has them
  (0.12.0 does not); the erc20 read policy (P2) and truncation guard landed
  in PR #137.
- [#8](https://github.com/pinax-network/substreams-evm-balances/issues/8):
  the networks epic: per-network start heights, the producer record, the
  endpoint identity check, the cold start and the failed-root rules, with
  one issue per network family. BSC keeps no `networks:` entry and starts at
  0 (owner, 2026-10-06), which native accepts since v0.7.0.
- The [cutover contract](initialization-and-completeness.md) and its offline
  checks are done (#7, PR #144), per package since v0.7.0.

Done on 2026-10-06: the Rust 1.99.0 hash batch (#118), the "Package hashes"
CI job (#119), the sink pin
[substreams-sink-sql v4.13.0](https://github.com/pinax-network/substreams-sink-sql/releases/tag/v4.13.0),
commit `933a187` (#120, [deploy](clickhouse.md#build-and-deploy)),
and the release flow (#122). On 2026-10-13 the owner made
[`evm-balances-v0.6.0`](https://github.com/pinax-network/substreams-evm-balances/releases/tag/evm-balances-v0.6.0)
the only release: it now carries `native-balances-v0.2.1.spkg` and
`erc20-balances-v0.4.0.spkg` (the CI builds of tags `native-balances-v0.2.1`
and `erc20-balances-v0.4.0`, commit `313f8d1`)
and notes on what is included and how to deploy it, without `HASHES.txt`. The
separate native and erc20 releases were deleted; their tags remain. Later
that day sinks moved to the Token API undo buffer (BSC: 5) instead of
`--final-blocks-only`, accepting that a fork deeper than the buffer leaves
orphaned rows ([Token API live sinks](clickhouse.md#token-api-live-sinks-bsc)).
The first production backfill of v0.6.0 stopped on BSC block 127,411,449,
which native refused on the producer's CALLCODE self-transfer pair; v0.6.1
(native v0.2.2, #149) drops that pair. The owner then made releases plain
`v<version>` tags, named the BSC database `bsc:evm-balances@<version>` and
renamed this repository from `substreams-evm-extended` to
`substreams-evm-balances` (GitHub redirects the old URLs). The DEX code moved
to the new `pinax-network/substreams-evm-dex`, and this repository was pruned
to balances. That evening the owner split the balance packages (v0.7.0): each
is released by its own tag (`evm-balances-native-v<version>`,
`evm-balances-erc20-v<version>`) into its own database, and the combined
package with its plain `v<version>` tags is retired. Releases are not live
qualification.

Upstream, open: deterministic spkg packing,
[streamingfast/substreams#978](https://github.com/streamingfast/substreams/pull/978);
a deterministic `db_out` composite-key order,
[anthropics/buffa#574](https://github.com/anthropics/buffa/issues/574) and
[streamingfast/substreams-sink-database-changes#14](https://github.com/streamingfast/substreams-sink-database-changes/issues/14).
