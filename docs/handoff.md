# Handoff: current state

Where the repository stands on 2026-10-11, for whoever picks it up next. Keep
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

The committed spkgs are CI's canonical Linux builds (Rust 1.99.0, substreams
CLI v1.22.0), taken from the "Package hashes" job's `packages` artifact.
Identify a build by its module hash, never by the spkg sha256. None is
live-qualified.

| Package | Version and spkg | Module | Module hash | WASM sha256 |
| --- | --- | --- | --- | --- |
| [`native/balances`](../native/balances/README.md) | v0.2.2, `spkg/native-balances-v0.2.2.spkg` | `map_events` | `378d27a630f0f2c8589f4396b308edff9c981ffd` | `8fc5a84901b498570efdf8cd54f5a077dc040b07ddc5bffa923016e2e909c235` |
| [`erc20/balances`](../erc20/balances/README.md) | v0.4.0, `spkg/erc20-balances-v0.4.0.spkg` | `map_events` | `d8a9db86240ca6466270f140798fae0b8db05f7b` | `17a847aa77346058a160553c1fe3f79ab42cc4c0c5de1e0f3997e3a7b28e05d1` |
| [`evm-balances`](../evm-balances/README.md) | v0.6.1, `spkg/evm-balances-v0.6.1.spkg` | `db_out` | `b3313b50a193a78e295350fc6e83b77d3b0fd5c8` | `1e6520fab2764d445754140aa45081307aa624cc9b86750c8c12ddfb7490eba7` |

- **`native/balances`** takes only the block, with no params. Not qualified.
  The last qualified build is historical: the prost build was live-qualified
  on BSC on 2026-09-23 (WASM `48d89d28…`, module `5a2a2e0c…`), for the
  accounts it emitted in the stated windows
  ([evidence](../native/balances/README.md#live-qualification-bsc-2026-09-23)).
  No build since then is qualified.
- **`erc20/balances`** is inference only. Not qualified, and it has not run in
  a Substreams engine. Offline, on five captured BSC intervals scored against
  every row the RPC reference package emitted for them, it reproduces
  58.5–62.8% of those rows with 99.92–100% value precision
  ([measured](../erc20/balances/README.md#measured) gives each interval and
  its scored holders). The earlier layout package v0.1.0, another source, was
  checked on live BSC on 2026-09-23
  ([report at `9b41c7f`](https://github.com/pinax-network/substreams-evm-balances/blob/9b41c7f/erc20/balances/docs/live-package-bsc-2026-09-23.md)).
- **`evm-balances`** imports both maps with the module hashes above and
  embeds the schema `ed1c3bff…`. Not qualified; it has not run in a
  Substreams engine or a sink.

Older versions under `spkg/` (native v0.1.0 and v0.2.0, erc20 v0.1.0 and
v0.2.0, evm-balances v0.4.0 and v0.5.0) are historical builds that the package
READMEs describe. `spkg/erc20-balances-v0.3.4.spkg` is the immutable RPC
reference ([migration](migration.md)); versions skip v0.3.x so that no build
can overwrite it.

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
`substreams pack` also embeds the three balance packages' READMEs, so a README
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

## Open work

Open work is tracked in
[GitHub issues](https://github.com/pinax-network/substreams-evm-balances/issues),
under the roadmap
[#21](https://github.com/pinax-network/substreams-evm-balances/issues/21). On
the path to production:

- [#124](https://github.com/pinax-network/substreams-evm-balances/issues/124):
  qualify the balance packages above on BSC, then cut Token API over to
  `evm-balances`. Every live step needs the owner's OK. The offline
  preconditions are met apart from the BSC reader release (#116).
- [#116](https://github.com/pinax-network/substreams-evm-balances/issues/116):
  BSC producer drift under firehose-tracer 5.5.0. The erc20 read policy (P2)
  and truncation guard landed in PR #137. Open: the reader release and height
  from infra, `Call` fields
  35/36 once a substreams-ethereum release has them (0.12.0 does not), and
  the rows-per-block alert in ops (the query is in the
  [`evm-balances` README](../evm-balances/README.md)).
- [#117](https://github.com/pinax-network/substreams-evm-balances/issues/117):
  per-network start heights, a producer and tracer record per network, and
  the endpoint identity check in the `evm-balances` README. BSC keeps no
  `networks:` entry and starts at 0 (owner, 2026-10-06).
- [#7](https://github.com/pinax-network/substreams-evm-balances/issues/7):
  the [cutover contract](initialization-and-completeness.md) and its offline
  checks are done (PR #144); the cold start per network (#8) and the live
  checks during #124 B2 remain.
- [#123](https://github.com/pinax-network/substreams-evm-balances/issues/123):
  how Token API treats wrong computed balances and stale RPC-era pairs,
  decided before #124 B3.
- [#8](https://github.com/pinax-network/substreams-evm-balances/issues/8):
  the networks epic, with one issue per network.

Done on 2026-10-06: the Rust 1.99.0 hash batch (#118), the "Package hashes"
CI job (#119), the sink pin
[substreams-sink-sql v4.13.0](https://github.com/pinax-network/substreams-sink-sql/releases/tag/v4.13.0),
commit `933a187` (#120, [README](../evm-balances/README.md#build-and-deploy)),
and the release flow (#122). On 2026-10-13 the owner made
[`evm-balances-v0.6.0`](https://github.com/pinax-network/substreams-evm-balances/releases/tag/evm-balances-v0.6.0)
the only release: it now carries `native-balances-v0.2.1.spkg` and
`erc20-balances-v0.4.0.spkg` (the CI builds of tags `native-balances-v0.2.1`
and `erc20-balances-v0.4.0`, commit `313f8d1`, with the module hashes above)
and notes on what is included and how to deploy it, without `HASHES.txt`. The
separate native and erc20 releases were deleted; their tags remain. Later
that day sinks moved to the Token API undo buffer (BSC: 5) instead of
`--final-blocks-only`, accepting that a fork deeper than the buffer leaves
orphaned rows ([README](../evm-balances/README.md#token-api-live-sink-bsc)).
The first prod-c backfill of v0.6.0 stopped on BSC block 127,411,449, which
native refused on the producer's CALLCODE self-transfer pair; v0.6.1 (native
v0.2.2, #149) drops that pair. The owner then made releases plain
`v<version>` tags, named the BSC database `bsc:evm-balances@<version>` and
renamed this repository from `substreams-evm-extended` to
`substreams-evm-balances` (GitHub redirects the old URLs). The DEX code moves
to the new `pinax-network/substreams-evm-dex`, and this repository is pruned
to balances. Releases are not live qualification.

Upstream, open: deterministic spkg packing,
[streamingfast/substreams#978](https://github.com/streamingfast/substreams/pull/978);
a deterministic `db_out` composite-key order,
[anthropics/buffa#574](https://github.com/anthropics/buffa/issues/574) and
[streamingfast/substreams-sink-database-changes#14](https://github.com/streamingfast/substreams-sink-database-changes/issues/14).
