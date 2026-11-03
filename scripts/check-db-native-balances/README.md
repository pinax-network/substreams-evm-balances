# Check a native balances database against the chain

Owner's exception to the no-host-tools rule (2026-11-03), like
`check-live-balances.ts`. Where that script checks a package's live output,
this one checks a **database**: sampled rows of
`<chain>:evm-balances-native@<version>`.native_balances on ClickHouse against
`eth_getBalance` at each row's own block, and, where Etherscan's free tier
covers the chain, against Etherscan's current balance as an independent
source. It reads ClickHouse and the chain only; it writes nothing.

## What it does

1. `sample.sh <db> <out.tsv>` takes the top 1,000 holders (at least one
   native unit) plus up to 900 random addresses per balance stratum (zero
   rows, under 0.01, 0.01 to 1, 1 to 100, over 100 units) from the 0x00
   address slice of `native_balances FINAL`, with each row's block.
2. `compare.py <sample.tsv> <out_dir>` runs two passes. Pass 1 asks Etherscan
   (`account/balancemulti`, 20 addresses per call, free tier rate) for the
   current balance of every sampled address; rows that match are done. Pass 2
   re-reads each remaining row from ClickHouse and asks the chain for the
   balance at that row's block: `changed_since` means Etherscan saw a later
   block and the row is right at its own block, `mismatch` means the database
   is wrong there. With `--rpc-only` it skips Etherscan and checks every
   sampled row at its block, which is the stronger check and the only one on
   chains Etherscan's free tier does not serve (BSC, Base, Optimism).
3. `run_chain.sh <label> <db> <rpc-id> <etherscan-chainid>` runs both for one
   chain into `<label>/`.

A sample is not a universal claim: it says the rows it holds agree with the
chain, across every balance magnitude and the whole top 1,000. State the
sample and the head block with any result, as the evidence files do.

## Environment (never printed)

| Variable | Use |
| --- | --- |
| `CH_URL`, `CH_USER`, `CH_PASSWORD` | ClickHouse HTTP, read-only queries |
| `PINAX_KEY` (or `RPC_URL`) | Pinax RPC, `https://<rpc-id>.rpc.pinax.network/v1/<key>/`; keyless calls fall to the throttled free tier |
| `ETHERSCAN_API_KEY` | Etherscan v2, pass 1 only; optional with `--rpc-only` |
| `DB_NAME`, `RPC_ID`, `ES_CHAINID`, `RPC_BATCH` | database, Pinax chain id, Etherscan chain id, addresses per batched call (default 50) |

## Example

```sh
export CH_URL=http://ch-node:8123 CH_USER=... CH_PASSWORD=... PINAX_KEY=... ETHERSCAN_API_KEY=...
./run_chain.sh polygon polygon:evm-balances-native@v0.7.12 polygon 137
# or, block-exact only:
./sample.sh 'bsc:evm-balances-native@v0.7.11' bsc/sample.tsv
DB_NAME='bsc:evm-balances-native@v0.7.11' RPC_ID=bsc python3 compare.py bsc/sample.tsv bsc/out_rpc --rpc-only
```

## Results

The 2026-11-03 run on all eight chains is in
[`native/balances/docs/evidence/chain-check-2026-11-03/`](../../native/balances/docs/evidence/chain-check-2026-11-03/)
and summarized in [native quality](../../docs/quality-native.md#checks-against-the-chain-and-etherscan-2026-11-03).
It found the Optimism pre-Bedrock seed defect that
[`repair-optimism-pre-bedrock-seed`](../../native/balances/clickhouse/examples/repair-optimism-pre-bedrock-seed/)
corrected.
