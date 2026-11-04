# Check an ERC-20 balances database against the chain

Owner's exception to the no-host-tools rule (2026-11-03), like
[`check-db-native-balances`](../check-db-native-balances/). It checks a
`<chain>:evm-balances-erc20@<version>` database on ClickHouse against the
chain, in four read-only steps. An ERC-20 row can be wrong at its own block,
stale (right at its block, then the balance changed without a row), or missing
(a holder with a balance and no row), so each step measures one of these.

| Step | Question | How |
| --- | --- | --- |
| `sample` | what to check | the top 1,000 holders of the 20 tokens with the most distinct senders in the last 24 h that the database holds rows for, 900 random (contract, holder) pairs per raw-balance band (zero, under 1e6, 1e6 to 1e12, 1e12 to 1e18, 1e18 and up) and up to 900 rows written in the last 24 h, from the 0x1a address slice |
| `check` | wrong at its block, and stale now | `balanceOf` at each row's block (precision), then the current row against `balanceOf` at a fixed head block, the database cursor (freshness) |
| `coverage` | missing holders | recipients of the last 24 h of ERC-20 transfers in `<chain>:evm-transfers@<version>`, a different package: for each, is there a row, and does it equal `balanceOf` at the head block |
| `supply` | missing or stale holders in aggregate | each top token's sum of rows against `totalSupply()` at the cursor block, retried until the supply is the same before and after the sum |
| `etherscan` | an independent source | on the chains Etherscan's free tier serves (Ethereum, Arbitrum One, Polygon, Unichain, HyperEVM), 1,000 pairs against Etherscan `tokenbalance` and the Pinax node |

The top-token ranking skips higher-ranked candidates with no rows at all and
records them in `top_tokens_skipped.tsv` with their supply. On most chains
they are address-poisoning tokens, whose fake transfers top every sender
ranking; a real token missing entirely would show there too.

Reading the results:

- A supply gap equal to the token's zero-address balance is by design: the
  package never emits holder `0x0`.
- Some contracts' `totalSupply()` is not the sum of balances (QNT's is a fixed
  cap), so compare with the mint and burn events before calling a gap a
  database error.
- A coverage miss in a token whose `balanceOf` returns a balance for an
  address that never touched it is a fake-airdrop token, not a gap.
- A stale holder with a transfer after its row's block is a dropped
  contract-block; one without any is a computed balance.

## Environment (never printed)

| Variable | Use |
| --- | --- |
| `CH_URL` and `CH_CURL_CONFIG` (a `curl -K` file) or `CH_USER`, `CH_PASSWORD` | ClickHouse HTTP, read-only queries |
| `PINAX_KEY` (or `RPC_URL`) | Pinax RPC, `https://<rpc-id>.rpc.pinax.network/v1/<key>/` |
| `ETHERSCAN_API_KEY` | the `etherscan` step |
| `ERC20_DB`, `TRANSFERS_DB` | override the database names (default `<chain>:evm-balances-erc20@v0.7.4`, `<chain>:evm-transfers@v0.3.3`) |

## Example

```sh
export CH_URL=http://ch-node:8123 CH_USER=... CH_PASSWORD=... PINAX_KEY=... ETHERSCAN_API_KEY=...
python3 erc20check.py mainnet mainnet          # all steps into ./mainnet/
python3 erc20check.py bsc bsc check            # one step
```

## Results

The 2026-11-03 run on all eight chains is summarized in
[ERC-20 quality](../../docs/quality-erc20.md#checks-against-the-chain-and-etherscan-2026-11-03),
with the evidence in
[`erc20/balances/docs/evidence/chain-check-2026-11-03/`](../../erc20/balances/docs/evidence/chain-check-2026-11-03/).
