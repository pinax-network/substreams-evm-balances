# Token API balances: deployment status

A snapshot taken on 2026-10-19 at about 16:00 UTC. For each chain, it lists
the final package release, the ClickHouse database it writes, what is
backfilled, and what is still running. The quality comparisons with the
RPC-era databases are on separate pages:

- [Native balance quality](quality-native.md), for the seven chains whose
  native database is fully backfilled.
- [ERC-20 balance quality](quality-erc20.md), for Ethereum, the one chain
  whose ERC-20 database is fully backfilled.
- [Top tokens](quality-top-tokens.md): USDT, USDC and WETH on Ethereum, and
  the wrapped native token of every chain (WETH, WBNB, WPOL, WHYPE).
- [Methods](quality-methods.md): how the comparisons work, with the SQL to
  rerun them.

The rows here change as backfills finish. [Handoff](handoff.md) keeps the
current state.

## Final releases

| Package | Release tag | `map_events` | `db_out` | Chains |
| --- | --- | --- | --- | --- |
| `evm_balances_native` v0.7.8 | [`evm-balances-native-v0.7.8`](https://github.com/pinax-network/substreams-evm-balances/releases/tag/evm-balances-native-v0.7.8) | `6ed2464534915064dd4f475e80d453b709fd7727` | `40f294cb126dae6dbbd874f9e291e85b634d04d1` | Ethereum, BSC, Polygon, Optimism, Unichain, HyperEVM |
| `evm_balances_native` v0.7.9 | [`evm-balances-native-v0.7.9`](https://github.com/pinax-network/substreams-evm-balances/releases/tag/evm-balances-native-v0.7.9) | `424ce160682cf8c429d18322fbc23bf91fdd8a5c` | `9cf8c500a73dd95f91112d1c71fe4fe7e964334f` | Base |
| `evm_balances_native` v0.7.10 | [`evm-balances-native-v0.7.10`](https://github.com/pinax-network/substreams-evm-balances/releases/tag/evm-balances-native-v0.7.10) | `ee871813ce0ebe99dc94d57d3975cb0b45e75095` | `5feb4830c806c568cc9df46b8f280b618cc2b51b` | Arbitrum One |
| `evm_balances_erc20` v0.7.4 | [`evm-balances-erc20-v0.7.4`](https://github.com/pinax-network/substreams-evm-balances/releases/tag/evm-balances-erc20-v0.7.4) | `e611c63aab92c691fbb32550fa008faab113f194` | `0786f2dffd4c82b9fa011988e6b9b6a18ca4b51b` | Ethereum, BSC |

Each spkg is the release asset
`https://github.com/pinax-network/substreams-evm-balances/releases/download/<tag>/<tag>.spkg`.
The hashes above were read with `substreams info` from the published assets.

The native versions differ only in which producer shapes they accept. v0.7.9
and v0.7.10 accept blocks that v0.7.8 refused on Arbitrum One and Base, and
give v0.7.8's output for every block v0.7.8 accepts. So one native database
per chain is enough, and a chain moves to a newer version only when its
full-history noop needs it.
No release is live-qualified yet (see [Handoff](handoff.md)).

## Databases per chain

Each new database is backfilled from the chain's first Extended block to a
fixed end block. That end block is the start block of its future live sink.
"Blocks" counts blocks with at least one balance change; blocks without one
have no row. "Holders" counts distinct addresses for native and distinct
(contract, holder) pairs for ERC-20, including rows whose balance is now
zero.

| Chain | Native database | Backfilled range | Blocks | Holders | ERC-20 database | Backfilled range | Blocks | Holders |
| --- | --- | --- | ---: | ---: | --- | --- | ---: | ---: |
| Ethereum | `mainnet:evm-balances-native@v0.7.8` | [0, 26,198,000) | 26,195,986 | 345,811,875 | `mainnet:evm-balances-erc20@v0.7.4` | [0, 26,215,000) | 22,268,024 | 443,936,986 in 1,428,012 contracts |
| BSC | `bsc:evm-balances-native@v0.7.8` | [0, 127,810,000) | 127,199,156 | 529,074,089 | `bsc:evm-balances-erc20@v0.7.4` | backfilling to 128,355,000 ([below](#in-progress)) | | |
| Base | `base:evm-balances-native@v0.7.9` | [0, 52,720,000) | 50,867,566 | 311,651,644 | none | | | |
| Optimism | `optimism:evm-balances-native@v0.7.8` | [105,236,000, 158,320,000) | 52,714,811 | 25,933,707 | none | | | |
| Polygon | `polygon:evm-balances-native@v0.7.8` | [0, 95,690,000) | 86,915,716 | 197,432,321 | none | | | |
| Unichain | `unichain:evm-balances-native@v0.7.8` | [0, 61,490,000) | 52,934,210 | 8,385,130 | none | | | |
| HyperEVM | `hyperevm:evm-balances-native@v0.7.8` | [0, 48,780,000) | 43,413,402 | 1,254,132 | none | | | |
| Arbitrum One | `arbitrum-one:evm-balances-native@v0.7.10` | to be backfilled over [22,208,000, 515,680,000) ([below](#in-progress)) | | | none | | | |

Every database listed as backfilled was checked the same way. Its
`backfill_processed_ranges` cover the range with no break and no duplicate,
and its `blocks` table has one row per block. A block has no row when nothing
in it changed a balance. The native databases were also checked against the
RPC-era transfer databases, `<chain>:evm-transfers@v0.3.3`: no block that
carries a native value transfer there (`native_transfers.amount > 0`) is
missing from the native database.

| Chain | Blocks in range | Blocks without a row | Blocks with a native value transfer | Of those, missing from native |
| --- | ---: | ---: | ---: | ---: |
| Ethereum | 26,198,000 | 2,014 | 26,190,111 | 0 |
| BSC | 127,810,000 | 610,844 | 127,199,155 | 0 |
| Base | 52,720,000 | 1,852,434 | 50,227,257 | 0 |
| Optimism | 53,084,000 | 369,189 | 45,006,232 | 0 |
| Polygon | 95,690,000 | 8,774,284 | 80,933,100 | 0 |
| Unichain | 61,490,000 | 8,555,790 | 23,414,041 | 0 |
| HyperEVM | 48,780,000 | 5,366,598 | 14,378,421 | 0 |

On Ethereum, the blocks without a row are empty blocks between the Merge and
Shanghai, which carry no reward. On Base, 1.72M of them are below block
2,000,000, before Base opened to users. On the L2s and Polygon, a block can
hold only system transactions, which pay no fee.

Optimism starts at Bedrock and Arbitrum One at Nitro, at the first whole
1,000-block segment of Extended blocks. The legacy chains' balances appear in
no block's records, so an account that has not changed since then has no
row ([start blocks](../native/balances/README.md#input-and-networks)).

### What the Token API serves today

The Token API still reads the RPC-era databases, `<chain>:evm-balances@v0.3.3`.
Each was backfilled up to the block below and is kept at the chain head by a
live sink. All comparisons on the quality pages are against these databases.

| Chain | RPC-era database | Backfilled to, then live | Head on 2026-10-19 ~15:20 UTC |
| --- | --- | ---: | ---: |
| Ethereum | `mainnet:evm-balances@v0.3.3` | 24,420,000 | 26,227,474 |
| BSC | `bsc:evm-balances@v0.3.3` | 81,390,000 | 128,575,312 |
| Base | `base:evm-balances@v0.3.3` | 42,090,000 | 52,816,917 |
| Optimism | `optimism:evm-balances@v0.3.3` | 147,920,000 | 158,412,211 |
| Polygon | `polygon:evm-balances@v0.3.3` | 83,060,000 | 95,811,900 |
| Unichain | `unichain:evm-balances@v0.3.3` | 40,180,000 | 61,674,842 |
| HyperEVM | `hyperevm:evm-balances@v0.3.3` | 35,000,000 | 48,969,310 |
| Arbitrum One | `arbitrum-one:evm-balances@v0.3.3` | 433,100,000 | 516,324,009 |
| Avalanche | `avalanche:evm-balances@v0.3.4` | not compared | |

Avalanche stays on the RPC-era package. Its endpoint serves no Extended
blocks, so both new packages refuse every block there.

## Live sinks

None of the new databases has a live sink yet. Each one stops at its
backfill's end block, which is the start block of its future live sink. When
a sink starts, it catches up from that block to the head. The
[cutover](clickhouse.md) then points the Token API's `balances-native` and
`balances-erc20` keys at the new databases, one chain and one side at a time.

| Chain | Native live sink start | ERC-20 live sink start | Live sink deployment |
| --- | ---: | ---: | --- |
| Ethereum | 26,198,000 | 26,215,000 | native proposed (awaiting the owner); ERC-20 not proposed yet |
| BSC | 127,810,000 | 128,355,000 | native proposed (awaiting the owner); ERC-20 after its backfill |
| Base | 52,720,000 | | not proposed yet |
| Optimism | 158,320,000 | | not proposed yet |
| Polygon | 95,690,000 | | not proposed yet |
| Unichain | 61,490,000 | | not proposed yet |
| HyperEVM | 48,780,000 | | not proposed yet |
| Arbitrum One | 515,680,000 | | after its backfill |

## In progress

### Arbitrum One native v0.7.10

The full-history noop of native v0.7.9 refused block 500,689,520 on
2026-10-18. v0.7.10 accepts it, and at the owner's direction Arbitrum One
resyncs into `arbitrum-one:evm-balances-native@v0.7.10`.

- Noops: 10 full-history noops over [22,208,000, 515,680,000), in ranges of
  about 50M blocks, all running since 2026-10-19 11:22 UTC with no refusal so
  far. The same ranges took 10.5 to 14 hours for v0.7.9, so they should finish
  between about 22:00 UTC on 2026-10-19 and 01:30 UTC on 2026-10-20.
- Backfills: each range is backfilled once its noop passes, two at a time.
  v0.7.9 took about 3.5 hours per 50M-block range. At that rate the database
  would be complete late on 2026-10-20 UTC (an estimate).
- The partial `arbitrum-one:evm-balances-native@v0.7.9`, [22,208,000,
  220,000,000), is superseded. Dropping it needs the owner's OK.

### BSC ERC-20 v0.7.4

All 10 full-history noops over [0, 128,355,000) passed. The backfills run in
ascending ranges of 12,781,000 blocks, two at a time.

| Range | State |
| --- | --- |
| [0, 12,781,000) | done |
| [12,781,000, 25,562,000) | done |
| [25,562,000, 38,343,000) | done |
| [38,343,000, 51,124,000) | running |
| [51,124,000, 63,905,000) | running |
| [63,905,000, 76,686,000) to [115,029,000, 128,355,000) | five ranges queued |

A range has taken 3.5 to 4.5 hours, so the backfill should be complete around
midday UTC on 2026-10-20 (an estimate; later BSC blocks are denser). The
ERC-20 comparisons for BSC (USDT, USDC, WBNB and the top tokens) follow then.
The [top-tokens page](quality-top-tokens.md) already checks WBNB against the
native database.

### Superseded databases

These databases are replaced, and dropping each needs the owner's OK:

| Database | Replaced by |
| --- | --- |
| `mainnet:evm-balances-erc20@v0.7.3` | `mainnet:evm-balances-erc20@v0.7.4` |
| `bsc:evm-balances-erc20@v0.7.3` | `bsc:evm-balances-erc20@v0.7.4` |
| `arbitrum-one:evm-balances-native@v0.7.9` (partial) | `arbitrum-one:evm-balances-native@v0.7.10` |

### Not scheduled

- ERC-20 on Base, Optimism, Polygon, Unichain, HyperEVM and Arbitrum One. The
  owner extended only the native package to these chains (2026-10-17), so the
  Token API keeps the RPC-era ERC-20 balances there.
- Avalanche ([above](#what-the-token-api-serves-today)).
