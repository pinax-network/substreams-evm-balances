# Pinned sink descriptors

This descriptor-only package contains no executable modules. It supplies the
schema actually emitted by `substreams-database-change` 5.0.0, preserves
`sf.substreams.sink.sql.v1.Service` for the standalone SQL sink and, since
v0.2.0, supplies `sf.substreams.foundational_store.model.v2.SinkEntries`, the
output of native `map_sink_entries` that a StreamingFast hosted store ingests.

The SQL descriptor uses `sf/substreams/sink/sql/v1/deprecated.proto`, the path
bundled in current Substreams CLIs. Older CLI readers load it from this package;
newer readers merge the same path once. The old `services.proto` path defines
the same messages under a different filename and causes GUI registry conflicts
when both paths are present.

The vendored sources below are unchanged upstream files:

| File | Source revision | SHA-256 |
| --- | --- | --- |
| `sf/substreams/sink/database/v1/database.proto` | [substreams-database-change 5.0.0, `dae3f279f3d93518ac0f81e1ef71b82b4f7ec1b7`](https://github.com/streamingfast/substreams-sink-database-changes/blob/dae3f279f3d93518ac0f81e1ef71b82b4f7ec1b7/proto/sf/substreams/sink/database/v1/database.proto) | `122db4ce295c1e4e24d66a55d5796cdb950599ffeeb820684d61f630721094f7` |
| `sf/substreams/sink/sql/v1/deprecated.proto` | [Substreams `be35ad36f63a52ff49d3e15cf993de4cad6bfbd9`](https://github.com/streamingfast/substreams/blob/be35ad36f63a52ff49d3e15cf993de4cad6bfbd9/proto/sf/substreams/sink/sql/v1/deprecated.proto) | `3156ed87b92c95e842dce9a8138d16baca1b6a929d0ba0602a773da9244b48cb` |
| `sf/substreams/options.proto` | [Substreams `be35ad36f63a52ff49d3e15cf993de4cad6bfbd9`](https://github.com/streamingfast/substreams/blob/be35ad36f63a52ff49d3e15cf993de4cad6bfbd9/proto/sf/substreams/options.proto) | `6a840f477f403427474ff79c75a6f298be99a82e6ed99b3ae997f083c354e20b` |
| `sf/substreams/foundational-store/model/v2/model.proto` | [substreams-foundational-store `cd54d1098609e7f76710ff446e97a7be0875b54e`](https://github.com/streamingfast/substreams-foundational-store/blob/cd54d1098609e7f76710ff446e97a7be0875b54e/proto/sf/substreams/foundational-store/model/v2/model.proto) | `719eb85e2c539bd9c2244ebbf0ac12eae0695080b560b3f601b3433776fa1075` |

The database source is distributed in the pinned 5.0.0 Rust crate; its
`.cargo_vcs_info.json` records the revision above. The SQL descriptor was
introduced at Substreams `4a57e19fed98e1c2fba1fac316c21819f4848ba5` and is
unchanged at the pinned revision. The foundational-store model is that file's
last revision (2025-11-05, which added `if_not_exist`). The Substreams CLI at
the pinned revision bundles the same path, and its field numbers are those of
the `substreams` 0.8.0 crate's generated types.

Rebuild with `make -C proto/sink pack` (Buf and Substreams CLI required).
The recipe compiles the sources and their standard Protobuf dependencies into
an ignored `sink.binpb`, then packs
`spkg/evm-balances-sink-protodefs-v0.2.0.spkg`. Run the recipe from the package
directory, as `make -C` does, because the CLI resolves descriptor-set
`localPath` against its working directory. v0.1.0 was built with Substreams v1.22.0
(commit `be35ad36f63a52ff49d3e15cf993de4cad6bfbd9`); v0.2.0 with the CLI at
that commit and Buf 1.42.0. Different CLI/Buf versions
may embed additional system descriptors; the source hashes above identify the
contract. Normal balance-package builds use the committed descriptor package
and need neither Buf nor a network request.

The two original upstream descriptor packages in `spkg/` are preserved as
historical evidence; the current manifests do not import them. `erc20/balances`
still imports v0.1.0, which has no hosted-store descriptors; `native/balances`
imports v0.2.0.
