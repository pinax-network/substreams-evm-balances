# Persisted native balance effects

This page is the chain / producer / fork / reason matrix required by
[#17](https://github.com/pinax-network/substreams-evm-balances/issues/17). It
records what the saved data and recorded BSC runs establish, what the code
handles by pinned rule without a fixture, and what stays unqualified. The
2026-09-23 live reports retain their bounded scope; this document does not
authorize new Substreams, Firehose, RPC or sink runs.

## Scopes

| Scope | Source in `sf.ethereum.type.v2.Block` | Rule | Evidence |
| --- | --- | --- | --- |
| Succeeded transaction | `transaction_traces[].calls[].balance_changes` with `state_reverted == false` | every non-no-op record persists | block 122260950 oracle; saved replay |
| Reverted child frame | same, `state_reverted == true` | dropped | synthetic tests; saved replay continuity |
| Failed / reverted transaction | root call only | `GAS_BUY`, `GAS_REFUND`, `REWARD_TRANSACTION_FEE` persist; so do, each in its exact shape, a blob transaction's `REWARD_BLOB_FEE` (v0.7.2, BNB Chain), a `BURN` of exactly `gas_used * base_fee` (v0.7.8, Polygon), the `TRANSFER` credits after the execution that, with any `REWARD_TRANSACTION_FEE` there, add up to the `GAS_BUY` debit less the `GAS_REFUND` credit (v0.7.8, Arbitrum One), the `TRANSFER` settlement of a failed `ArbitrumRetryTx` or `ArbitrumSubmitRetryableTx` (v0.7.8, Arbitrum One) and a failed deposit's `INCREASE_MINT` (v0.7.8, OP stack); records of the reasons known to revert are dropped during the execution (or at ordinal 0), and one that moves a balance outside the execution and persists by none of these rules fails the block (v0.7.8); any other reason fails the block, and so does a failed Arbitrum deposit, submission, retry or internal transaction whose balance moves are in no exact shape | 6,655 failed-root gas records in the saved replay; two captured failed SetCode transactions; captured failed transactions `bsc-42357369` (blob fee), `polygon-95537024` (base fee), `arbitrum-150000072` and `arbitrum-514857274` (fee transfers), `arbitrum-52067707`, `arbitrum-314259052` and `arbitrum-511131266` (retries), `arbitrum-71044381` and `arbitrum-333543134` (submissions), and failed deposits of Base, Optimism and Unichain ([rules](../README.md#failed-transactions)) |
| Failed SetCode (EIP-7702) | authorization nonce/code records before root `begin_ordinal`; where the root call records `begin_ordinal` 0 (Arbitrum One's `Block.ver` 3), before the boundary recovered from the exact shape of the authorization records (v0.7.8, ERC-20 v0.7.3); since native v0.7.9, when the shared rules refuse the block, with the sender's own authorization carrying the transaction's nonce, which cannot apply and which the producer leaves unmarked, taken as discarded | handled by `evm-persist`; no native balance effect beyond gas | captured `bsc-121114122` / `bsc-121114153`; Arbitrum One `arbitrum-350048039`, `-400015655`, `-450003654` and `-514791024` (failed) and `-500001384` (a new authority), under [`erc20/balances/tests/fixtures/`](../../../erc20/balances/tests/fixtures/); native v0.7.9: `arbitrum-358023229`, `-360021045`, `-382234350`, `-420239048` and `-472773590` under [`tests/fixtures/`](../tests/fixtures/) ([rule](../README.md#failed-transactions)) |
| System call | `system_calls[].balance_changes` with `state_reverted == false` | persists | rule only; the saved BSC captures carry no system-call balance record |
| Block level | `Block.balance_changes` | always persists | 2,878 fee-reward records including the fee reset at 122260950 |

Reduction is by execution ordinal across all scopes, except in a recognized
Polygon `Block.ver` 3 state-sync transaction, whose depth-1 calls restart the
ordinals: its records are ordered by depth-1 call, then ordinal (v0.7.8,
[execution order](../README.md#execution-order-v078)). Array order is not
execution order: block-level records are serialized after the transactions but
the fee reset at block 122260950 (ordinal 3244) must follow the last
transaction credit (ordinal 3237). Per-account continuity (`old == previous
new`) and a strictly increasing execution order are required; ties and gaps
fail the block. The only exceptions are nine recognized producer shapes, each
with a captured fixture ([rules](../README.md#continuity-exceptions)):

- The self-transfer pair (v0.2.2; HyperEVM's shape since v0.7.8), whose two
  records are dropped.
- The BSC `Block.ver` 3 fee pool payout (v0.7.0, v0.7.1): the producer
  recorded the 1/16 system-reward debit of the fee pool `0xff…fe` as
  `n -> n`, and native restores it as `n + r -> n` from the coinbase credit
  `r`, the next block-level record before any transaction starts (one or two
  ordinals later), where `r = (n + r) >> 4`, then checks `n + r` for
  continuity like any record.
- The EIP-6780 self-destruct debit (v0.7.3): the Ethereum `Block.ver` 3
  producer records only the beneficiary's `SUICIDE_REFUND` credit `d` of the
  destroyed account's whole balance, and native restores the account's debit
  `d -> 0` at the credit's ordinal, then checks `d` for continuity. Since
  v0.7.8 the self-destructing call may hold other records (one
  `SUICIDE_REFUND`, no `SUICIDE_WITHDRAW`, no record of the destroyed account
  after the credit, none of its withdraws in the transaction): Polygon's
  `Block.ver` 3 producer records no withdraw at all, and Ethereum's
  `Block.ver` 3 producer none for an account created in the transaction,
  whose CREATE root call also holds the gas records.
- The EIP-6780 self-destruct to itself of an account not created in the
  transaction (v0.7.5): the producer records `SUICIDE_REFUND` `0 -> d` and
  `SUICIDE_WITHDRAW` `d -> 0`, although the account keeps `d`, and native
  drops both.
- The Ethereum EIP-6780 self-destruct of an account created in the
  transaction (v0.7.6, v0.7.7): the producer records `SUICIDE_WITHDRAW`
  `d -> 0` and the beneficiary's `SUICIDE_REFUND` `b -> b + d`; the
  beneficiary's records after the execution (gas refund, fee credit) leave
  out the sum `c` of such credits, and native adds `c` to them when each moves
  exactly the gas refund or fee the transaction implies (otherwise they are
  not shifted and must chain as recorded).
- Arbitrum One's repeated self-destruct withdraw (v0.7.8, v0.7.9): the
  `Block.ver` 3 producer records the destroyed account's `SUICIDE_WITHDRAW`
  `d -> 0` again after the account reached zero, in one of three exact orders
  (since v0.7.9 also with the refund two ordinals after the debit of an
  account not created in the transaction, when the call creates the
  beneficiary's account between them), and native drops the repeat.
- Arbitrum One's repeated withdraw of an account that self-destructs to itself
  since ArbOS 20 (v0.7.9): the producer records `SUICIDE_WITHDRAW` `d -> 0`,
  `SUICIDE_REFUND` `0 -> d` of the account itself and `SUICIDE_WITHDRAW`
  `d -> 0` at consecutive ordinals, the account keeps `d` (EIP-6780), and
  native drops the repeat, in an Arbitrum block since ArbOS 20 only.
- Arbitrum One's legacy self-destruct to itself before ArbOS 20 (v0.7.9): the
  producer records `SUICIDE_REFUND` `d -> 2d` and then `SUICIDE_WITHDRAW`
  `d -> 0` from the balance before the refund, the opcode burns the balance,
  and native rewrites the withdraw to start from `2d`, in an Arbitrum block
  before ArbOS 20 only.
- Base's fee vault credit after the vault's transfers during the execution
  (v0.7.8, v0.7.9): the `Block.ver` 5 producer records the BaseFeeVault's or
  L1FeeVault's fee credit, after the vault's `withdraw()` or, since v0.7.9, a
  transfer into the vault in the same transaction, from the vault's balance
  before the transaction, and native rewrites it to start from the vault's
  balance after the execution (zero after a withdrawal; otherwise a later
  record of the vault in the block must check it).

Native reads an Arbitrum block's ArbOS version from the block itself: Nitro's
`ArbitrumInternalTx` (type 106) from `0x…0a4b05` first, and the version in
bytes 16 to 24 of the header's mix digest (v0.7.9). A self-destruct to itself
of an account not created in the transaction, recorded as
`SUICIDE_WITHDRAW` `d -> 0`, `SUICIDE_REFUND` `0 -> d` and
`SUICIDE_WITHDRAW` `d -> 0`, fails the block in any other shape or on any
other network (v0.7.8): those records chain to zero while the account keeps
`d`.

Ordinal 0 is rejected; there is no fixture for a legitimate zero-ordinal
persisted record. BSC's genesis allocations do not need one: block 0 records
them as `GENESIS_BALANCE` records from zero at positive ordinals (11, 24 and
26). Since v0.7.0 block 0 is accepted when every balance record in it is such
an allocation, and emits one row per funded account
([genesis](../README.md#genesis-v070)); native v0.2.2 refused block 0.

## Reason matrix

Reason numbers follow `sf.ethereum.type.v2.BalanceChange.Reason` in
[firehose-ethereum `type.proto`](https://github.com/streamingfast/firehose-ethereum/blob/develop/proto/sf/ethereum/type/v2/type.proto).
The pinned `substreams-ethereum` 0.12.0 bindings name reasons 0–20 (0.11.1
named 0–16); the reducer applies records regardless of whether it can name the
reason, because the reason only governs the failed-transaction policy. Counts are from the
saved-data replay of 1,439 BSC producer-version-5 blocks.

| Reason | Number | Succeeded / system / block | Failed transaction root call | Saved records |
| --- | --- | --- | --- | --- |
| `TRANSFER` | 5 | persists | reverts during the execution; outside it fails the block (v0.7.8) unless pinned: on Arbitrum One the fee credits after the execution that add up to the gas paid (v0.7.8, [block 150,000,072](../tests/fixtures/arbitrum-150000072-failed-fee-transfers.json), [block 514,857,274](../tests/fixtures/arbitrum-514857274-failed-fee-tip.json)) and the ArbOS settlement of a failed `ArbitrumRetryTx` (v0.7.8, [52,067,707](../tests/fixtures/arbitrum-52067707-failed-retry.json), [314,259,052](../tests/fixtures/arbitrum-314259052-failed-retry-value.json), [511,131,266](../tests/fixtures/arbitrum-511131266-failed-retry-out-of-gas.json)) or `ArbitrumSubmitRetryableTx` (v0.7.8, [71,044,381](../tests/fixtures/arbitrum-71044381-failed-submit-retryable-mint.json), [333,543,134](../tests/fixtures/arbitrum-333543134-failed-submit-retryable-call-value.json)); any other balance move of a failed Arbitrum deposit, submission, retry or internal transaction fails the block | 127,866 (Tx); Arbitrum One block 150,000,072: 8 failed fee credits in 4 reverted transactions; block 333,543,134: 5 in a failed submission |
| `GAS_BUY` | 7 | persists | persists | 95,532 (Tx), 3,344 (failed) |
| `REWARD_TRANSACTION_FEE` | 8 | persists; Base's `Block.ver` 5 producer records a fee vault's credit after the vault's withdrawal in the same transaction from the vault's balance before it, which native v0.7.8 rewrites to start from zero ([block 50,024,530](../tests/fixtures/base-50024530-fee-vault-stale-credit.json), [block 50,326,929](../tests/fixtures/base-50326929-fee-vault-stale-credits.json)), and after a transfer into the vault, which native v0.7.9 rewrites to start from the transferred balance ([block 47,424,842](../tests/fixtures/base-47424842-fee-vault-credit-after-transfer.json)) | persists | 95,532 (Tx), 3,344 (failed), 2,878 (block) |
| `GAS_REFUND` | 9 | persists | persists | 85,350 (Tx), 3,311 (failed) |
| `REWARD_BLOB_FEE` | 17 | persists (BNB Chain blob fee, credited to the system fee account after execution) | persists in a blob transaction (v0.7.2, [block 42,357,369](../tests/fixtures/bsc-42357369-failed-blob-fee.json)); fails the block in any other transaction | 218 (Tx); live 2026-09-23: 37 (Tx) in 250 parity-checked blocks; failed: 1 (block 42,357,369) |
| `TOUCH_ACCOUNT` | 10 | persists (no-op records are dropped) | reverts during the execution; outside it fails the block (v0.7.8) | none observed |
| `SUICIDE_REFUND`, `SUICIDE_WITHDRAW` | 11, 13 | persist; for an Ethereum EIP-6780 self-destruct of an account not created in the transaction the `Block.ver` 3 producer omits the destroyed account's debit, which native v0.7.3 restores from the credit ([block 20,249,174](../tests/fixtures/eth-20249174-selfdestruct-6780.json)), and the `Block.ver` 5 producer records it (block 25,741,008, transaction 509; block 25,747,303, transaction 546); for one to itself the `Block.ver` 3 producer records a refund from zero and a withdraw to zero, which native v0.7.5 drops ([block 19,481,732](../tests/fixtures/eth-19481732-selfdestruct-to-self.json)); for an account created in the transaction the beneficiary's records during the execution include the credit, but its records after the execution (gas refund, fee credit) omit the sum of the transaction's credits, which native v0.7.7 adds to them ([blocks 25,741,008](../tests/fixtures/eth-25741008-selfdestruct-created-credit.json), [25,747,303](../tests/fixtures/eth-25747303-selfdestruct-creation-tx.json), [25,869,452](../tests/fixtures/eth-25869452-selfdestruct-four-credits.json) and [25,920,498](../tests/fixtures/eth-25920498-selfdestruct-transfer-during-execution.json)); Polygon's `Block.ver` 3 producer records no `SUICIDE_WITHDRAW`, and Ethereum's `Block.ver` 3 producer none for an account created in the transaction, which native v0.7.8 restores also in a call with other records ([Polygon block 70,000,124](../tests/fixtures/polygon-70000124-selfdestruct-created.json), [Ethereum block 19,500,178](../tests/fixtures/eth-19500178-selfdestruct-created-root.json)); Arbitrum One's `Block.ver` 3 producer repeats the destroyed account's `SUICIDE_WITHDRAW` after the account reached zero, which native v0.7.8 drops ([blocks 250,005,039](../tests/fixtures/arbitrum-250005039-stale-selfdestruct-withdraw.json), [200,029,376](../tests/fixtures/arbitrum-200029376-stale-selfdestruct-withdraw.json), [450,014,091](../tests/fixtures/arbitrum-450014091-stale-selfdestruct-withdraw.json), [450,028,504](../tests/fixtures/arbitrum-450028504-stale-selfdestruct-withdraw.json) and [187,009,069](../tests/fixtures/arbitrum-187009069-stale-selfdestruct-withdraw.json); of accounts not created in the transaction [514,819,388](../tests/fixtures/arbitrum-514819388-stale-selfdestruct-withdraw-existing.json), [367,502,338](../tests/fixtures/arbitrum-367502338-stale-selfdestruct-withdraw-existing.json) and [176,115,455](../tests/fixtures/arbitrum-176115455-stale-selfdestruct-withdraw-existing.json), and since v0.7.9 to a beneficiary the call creates, [235,153,999](../tests/fixtures/arbitrum-235153999-stale-selfdestruct-withdraw-new-beneficiary.json), [223,944,083](../tests/fixtures/arbitrum-223944083-stale-selfdestruct-withdraw-new-beneficiary.json) and [344,105,344](../tests/fixtures/arbitrum-344105344-stale-selfdestruct-withdraw-new-beneficiary.json)); a self-destruct to itself of an account not created in the transaction recorded as a withdraw to zero, a refund from zero and a withdraw to zero fails the block (v0.7.8), except that native v0.7.9 drops the repeat on Arbitrum since ArbOS 20 ([block 326,011,151](../tests/fixtures/arbitrum-326011151-selfdestruct-to-self.json)); before ArbOS 20 Arbitrum One's producer records a self-destruct to itself as a refund `d -> 2d` and a withdraw from `d`, which native v0.7.9 rewrites to start from `2d` ([blocks 161,626,224](../tests/fixtures/arbitrum-161626224-legacy-selfdestruct-to-self-root.json) and [136,808,179](../tests/fixtures/arbitrum-136808179-legacy-selfdestruct-to-self.json)) | revert during the execution; outside it fail the block (v0.7.8) | Ethereum block 20,249,174: 1 `SUICIDE_REFUND` (Tx), no `SUICIDE_WITHDRAW`; block 19,481,732: 1 `SUICIDE_REFUND`, 1 `SUICIDE_WITHDRAW` (Tx); block 25,741,008: 2 `SUICIDE_WITHDRAW`, 2 `SUICIDE_REFUND` (Tx); block 25,747,303: 2 `SUICIDE_WITHDRAW`, 2 `SUICIDE_REFUND` (Tx); block 25,869,452: 6 `SUICIDE_WITHDRAW`, 6 `SUICIDE_REFUND` (Tx); block 25,920,498: 5 `SUICIDE_WITHDRAW`, 5 `SUICIDE_REFUND` (Tx); block 19,500,178: 1 `SUICIDE_REFUND`, no `SUICIDE_WITHDRAW` (Tx); Polygon block 70,000,124: 2 `SUICIDE_REFUND`, no `SUICIDE_WITHDRAW` (Tx); Arbitrum One: 1 `SUICIDE_REFUND` and 2 or 3 `SUICIDE_WITHDRAW` in each of the eight captured transactions |
| `CALL_BALANCE_OVERRIDE` | 12 | persists | reverts during the execution; outside it fails the block (v0.7.8) | none observed |
| `BURN` | 15 | persists (Polygon's base-fee credit to `0x3ef57def…`) | persists when it moves exactly `gas_used * base_fee` (v0.7.8, Polygon [block 95,537,024](../tests/fixtures/polygon-95537024-failed-base-fee.json)); otherwise reverts during the execution and fails the block outside it (v0.7.8) | Polygon block 95,537,024: 1 failed (transaction 4 of the fixture) |
| `REWARD_FEE_RESET` | 14 | persists | fails the block | none observed; the BSC v5 reset is recorded as `REWARD_TRANSACTION_FEE` at block level |
| `REWARD_MINE_UNCLE`, `REWARD_MINE_BLOCK` | 1, 2 | persist (block level) | fails the block | none observed (BSC has no mining rewards) |
| `DAO_REFUND_CONTRACT`, `DAO_ADJUST_BALANCE` | 3, 4 | persist (Ethereum DAO fork block) | fails the block | none observed; Ethereum fixture required |
| `GENESIS_BALANCE` | 6 | persists; block 0 is accepted only when every balance record in it is one, from zero (v0.7.0; v0.2.2 rejected block 0) | fails the block | none in the replay; BSC block 0: 3 (Tx) ([fixture](../tests/fixtures/bsc-0-genesis.json)) |
| `WITHDRAWAL` | 16 | persists (Ethereum execution-layer withdrawals, block level) | fails the block | none observed; Ethereum fixture required |
| `INCREASE_MINT` | 18 | persists (OP-stack deposit mint, in the root call before the execution) | persists for a failed deposit's mint in its exact shape (v0.7.8: Base [5,044,695](../tests/fixtures/base-5044695-failed-deposit-mint.json) and [7,092,606](../tests/fixtures/base-7092606-failed-deposit-mint-reverted-call.json); Optimism [106,973,772](../tests/fixtures/optimism-106973772-failed-deposit-mint.json), [107,494,341](../tests/fixtures/optimism-107494341-failed-deposit-mint-reverted-call.json) and [156,295,824](../tests/fixtures/optimism-156295824-failed-deposit-mint.json); Unichain [744,856](../tests/fixtures/unichain-744856-failed-deposit-mint-reverted-call.json)); **fails the block** otherwise | failed: 1 in each of the six fixtures |
| `REVERT` | 19 | persists if observed | **fails the block** – unqualified | none observed |
| `MONAD_TX_POST_STATE` | 20 | persists if observed | fails the block | out of scope |
| `UNKNOWN` / future | 0, >20 | persists if observed | fails the block | none observed |

"Persists if observed" means the producer emitted a persisted record and the
reducer applies its old/new values; it does not mean the chain semantics were
reviewed. Producer fixtures are required before any of these networks or
reasons are claimed.

### Live reason matrix (BSC, 2026-09-23)

250 contiguous final blocks 123,552,820–123,553,069 fetched from
`bsc.firehose.pinax.network` and replayed offline
([report](evidence/live-reasons-bsc-2026-09-23.json)); the packaged output of
the same blocks equals the replay, and every emitted row of these blocks was
checked against `eth_getBalance` at the block hash:

| Scope | Reasons (records) |
| --- | --- |
| Succeeded transaction | `TRANSFER` 67,428, `GAS_BUY` 32,923, `REWARD_TRANSACTION_FEE` 32,923, `GAS_REFUND` 30,266, `REWARD_BLOB_FEE` 37 |
| Failed transaction root call (persisted) | `GAS_BUY` 2,264, `GAS_REFUND` 2,259, `REWARD_TRANSACTION_FEE` 2,264 |
| Block level (fee reset) | `REWARD_TRANSACTION_FEE` 500 |

No system-call balance record, no `BURN`, `SUICIDE_*` or `TOUCH_ACCOUNT`
record, and no failed-transaction reason outside the pinned three occurred;
over the 5,000-block live window the reason guard refused no block.

## Chain / producer / fork matrix

As of 2026-10-06, native `map_events(block)` has no parameters or
`Block.ver` gate. The matrix records evidence and limitations, not a runtime
allowlist. Extended-block and concrete persistence checks still apply;
acceptance alone does not qualify a chain or producer release. The reports
below are historical and retain the parameter lists used at capture time.
The spot checks of 2026-10-14 streamed local builds of the first v0.7.8
source (`cfc2923`, with four of the v0.7.8 rules; its CI package, committed
in `c07f06d`, has `map_events` `e44375c0…`) over the stated windows and
compared every emitted row with `eth_getBalance` at the same block; each rule
added later that changes rows was checked live on the blocks it changes. They
cover the emitted rows of those windows only.

| Network | Producer versions in saved data | Status |
| --- | --- | --- |
| BSC (chain id 56) | 5 | Replayed: 1,439 blocks, 0 projection errors, 0 clock or continuity mismatches, 82/82 same-block RPC rows at 122260950. Fee-reset and blob-fee-reward semantics observed. **Live-qualified on 2026-09-23** (package `fbb46fc7…`): 1,024 saved control blocks equal to the offline replay and to `eth_getBalance` (76,139/76,139), and 5,000 contiguous recent blocks without a refused block, 200,344/200,344 same-block `eth_getBalance` checks (see the README). Spot checks on 2026-10-14 (head, 120M, 100M, 75.2M, 50M and 38M; 200 blocks each): 176,196 rows, none differ. |
| BSC | 4 | 71 saved blocks at heights 51,995,162–104,975,334 reduce with 0 projection errors and 0 continuity mismatches when version 4 was explicitly enabled (4,461 additional continuity checks); no native RPC oracle. The current map no longer gates this version. |
| BSC | 3 | Accepted by the historical prototype. Captured on 2026-10-13: block 0 ([genesis](../tests/fixtures/bsc-0-genesis.json)) and blocks 363, 12,700,000 and 25,401,003 ([fee pool payouts](../tests/fixtures/bsc-fee-pool-payout-ver3.json); 363 whole, the others in part). The former parameter gate excluded it because the version-3 tracer recorded system-call ordinals on a different scale from transaction ordinals and set every root call's `begin_ordinal` to 0 ([geth Firehose tracer](https://github.com/streamingfast/go-ethereum/blob/70f5118d6443624792f49501627a1cd80f51e8e9/eth/tracers/firehose.go), [firehose-ethereum CHANGELOG v2.10.0](https://github.com/streamingfast/firehose-ethereum/blob/9485efe2e6290e525fd4978b50462516ec752672/CHANGELOG.md)). Global ordinal reduction across scopes is not trustworthy on version 3. The current map accepts this version but retains ordinal and continuity checks; those checks do not establish complete detection of ordering defects. This producer records the fee pool's 1/16 system-reward debit as `n -> n`: native v0.2.2 refused the three captured blocks on it, found by a full-history noop on 2026-10-13 that failed in every range below block 38.1M, and v0.7.0 restores the debit (above). |
| Ethereum (1) | 3, 5 | Captured on 2026-10-14, post-[EIP-6780](https://eips.ethereum.org/EIPS/eip-6780) SELFDESTRUCT shapes only: blocks 20,249,174, 19,481,732 and 19,500,178 (version 3; native v0.7.3, v0.7.5 and v0.7.8) and 25,741,008, 25,747,303, 25,869,452 and 25,920,498 (version 5; native v0.7.6, v0.7.7), one transaction each. Spot checks (head, 25.5M, 23M, 20M and 17M, 100 blocks each, and 19,500,170–19,500,189): 179,972 rows, none differ. Genesis, uncle/block rewards, DAO redistribution, execution-layer withdrawals, blob fee debits, pre-EIP-6780 SELFDESTRUCT and the consensus-layer boundary need fixtures under #8. |
| Polygon PoS (137) | 3 (to block 73,440,255), 4 (from 73,440,256); the endpoint served block 95,537,024 as 5 and, later the same day, as 4 with the same records at other ordinals | Captured on 2026-10-14, all native v0.7.8: block 95,537,024 (failed transactions' base-fee `BURN`), block 70,000,124 (version 3: no `SUICIDE_WITHDRAW` for an account created and self-destructed in one transaction) and blocks 70,044,000, 20,007,040 and 20,025,792 (version 3 state-sync transactions, whose depth-1 calls restart the ordinals; the last before a span commit). Spot checks (head, 85M, 70M, 62M and 56M; 300 blocks each): 190,138 rows, none differ. Limits: the output of releases before v0.7.8 for version 3 state-sync blocks is unaudited, since records of different depth-1 calls that happened to chain in ordinal order were accepted; a state-sync layout outside the recognized shape fails the block. |
| Arbitrum One (42161) | 3; Extended from block 22,207,818, after Nitro, so streams start at the first whole 1,000-block segment, 22,208,000 | Captured on 2026-10-14, all native v0.7.8: blocks 150,000,072 and 514,857,274 (failed transactions' fee `TRANSFER` credits after the execution); 52,067,707, 314,259,052 and 511,131,266 (failed `ArbitrumRetryTx` settlements); 71,044,381 and 333,543,134 (failed `ArbitrumSubmitRetryableTx` settlements), 22,207,957 (a failed submission without a record) and 514,523,401, 514,497,707 and 514,709,255 (a deposit, a submission and a batch posting report, which the tests mark failed); 250,005,039, 200,029,376, 450,014,091, 450,028,504, 187,009,069, 514,819,388, 367,502,338 and 176,115,455 (repeated `SUICIDE_WITHDRAW`); and five SetCode transactions from block 350,048,039 to 514,791,024 (the shared rules' boundary for a root `begin_ordinal` of 0). Captured on 2026-10-17, native v0.7.9, from the blocks v0.7.8's full-history noops refused: 235,153,999, 223,944,083 and 344,105,344 (a repeated `SUICIDE_WITHDRAW` with the beneficiary's creation between the debit and the refund; 282,198,061 has it too), 161,626,224 and 136,808,179 (a legacy self-destruct to itself, ArbOS 10), 326,011,151 (a self-destruct to itself since Cancun, ArbOS 32) and 358,023,229, 360,021,045, 382,234,350, 420,239,048 and 472,773,590 (a failed SetCode transaction with the sender's unmarked self-authorization of the transaction's nonce). Spot checks (head, 450M, 300M and 150M; 300 blocks each): 9,307 rows, none differ. Limits, each failing the block: a failed retry's multi-dimensional gas refund (ArbOS 60, or ArbOS 61 and later under `MultiGasConstraints`), a failed submission in the later depth-1 layout or with its excess or withheld refunds, a filtered deposit or submission (Nitro v3.10.0 and later), a repeated `SUICIDE_WITHDRAW` in any other order, a self-destruct to itself of an account not created in the transaction in the producer's debit, refund and repeat order outside the exact shape native v0.7.9 drops, a legacy self-destruct to itself with the repeat (between about block 169.4M and ArbOS 20) and a self-destruct to itself since ArbOS 20 of an account created in the transaction (neither captured). The balances carried over at Nitro appear in no block's records. |
| Avalanche C-Chain (43114) | none | The endpoint serves no Extended blocks (2026-10-14): native refuses every block. |
| Base (8453) | 4 (to block 45,903,999), 5 (from 45,904,000) | Captured on 2026-10-14, all native v0.7.8: blocks 5,044,695 and 7,092,606 (version 4: a failed deposit's `INCREASE_MINT`, which the [OP deposit specification](https://specs.optimism.io/protocol/deposits.html#execution) keeps) and 50,024,530 and 50,326,929 (version 5: the BaseFeeVault's and L1FeeVault's credit after their withdrawal starts from the vault's balance before it); on 2026-10-17, native v0.7.9: 47,424,842 (version 5: the L1FeeVault's credit after a transfer into it starts from its balance before the transaction). Spot checks (head, 40M and 25M; 300 blocks each): 235,099 rows, none differ. Limits: no failed deposit of version 5 is captured; the OperatorFeeVault `0x4200…001b` has no captured credit, and a non-zero operator fee would change the L1 fee that the vault rule checks, so an L1FeeVault withdrawal block would then fail. |
| Optimism (10) | 4 (seen from block 105,712,236 to 140,002,047), 5 (seen from 155,196,870); Extended from block 105,235,064, after Bedrock, so streams start at the first whole 1,000-block segment, 105,236,000 | Captured on 2026-10-14, all native v0.7.8: failed deposits' `INCREASE_MINT` in blocks 106,973,772 and 107,494,341 (version 4) and 156,295,824 (version 5). The version 5 producer records a fee vault's credit after its withdrawal from zero (block 155,196,870), which chains. Spot checks (head, 150M, 130M and 110M; 300 blocks each): 34,151 rows, all equal apart from the zero address at block 110,000,181, where `eth_getBalance` answers inconsistently (two values on repeated calls) and neither equals the producer's; not explained. The balances carried over at Bedrock appear in no block's records. |
| Unichain (130) | 4 (to at least block 50,000,000), 5 (from at most 52,500,000) | Captured on 2026-10-14, native v0.7.8: block 744,856 (version 4: a failed deposit's `INCREASE_MINT`, whose call reverts). The version 5 producer records a fee vault's credit after its withdrawal from zero (block 61,187,535). No failed deposit of version 5 was found: all 505 deposits with a mint from block 49,901,783 to 61,264,100 succeeded (version 4 had 141 failed deposits in 128 blocks from 744,856 to 13,422,768). Spot checks (head, 40M and 20M; 300 blocks each): 12,855 rows, none differ. |
| HyperEVM (999) | 5 | Captured on 2026-10-14: block 44,000,001 (self-transfer recorded as four `TRANSFER` records, native v0.7.8). Spot checks (head and eleven heights from 2M to 46M; 300 blocks each): 23,231 rows, all equal apart from 253 zero-address rows of the 30M window. The zero address is the coinbase and receives the priority fees. From about block 29,999,900 to 30,000,500 `eth_getBalance` returns one constant zero-address balance while the producer records a fee credit to it in every block; before and after, both agree, and every other account in the window matches: an RPC history anomaly of that account. HYPE account changes and the EVM side of HyperCore↔HyperEVM transfers need fixtures; HyperCore spot/perp/staked balances are a separate domain not derivable from EVM state. |
| Arc | none | Native USDC at 18 decimals with an ERC-20 interface at 6 decimals over the same balance. Both raw observations must be preserved with an explicit alias identity; this package would emit the native side only. Chain id, producer and precision binding under #8/#12. |

Accepting a protobuf version does not qualify every state transition of a
network. SELFDESTRUCT records are applied as balance movements only; code or
account deletion is not inferred from them. Version 5 producers also stop
recording no-op state changes and gas changes; the persistence rules already
drop no-op balance records, so version 4 and 5 blocks reduce identically.

## Saved-data replay

Reports produced by `native-balances-tools replay` (a host tool, removed on
2026-10-03; at `6dade89`) over the locally retained
Extended block captures (the ERC-20 campaign's cache under
`erc20/balances/out/`, ignored by git) plus the committed block 122260950:

| Report | Producer versions | Blocks | Rows | Continuity checks | Result |
| --- | --- | --- | --- | --- | --- |
| [`replay-bsc-v5.json`](evidence/replay-bsc-v5.json) | 5 | 1,439 replayed; 71 version-4 blocks refused as unqualified | 126,180 (10,226 final zeros; 30,374 accounts) | 86,564 | 0 projection errors, 0 clock or continuity mismatches; oracle 82/82; prototype 82/82 |
| [`replay-bsc-v4-v5.json`](evidence/replay-bsc-v4-v5.json) | 4, 5 | 1,510 replayed | 136,640 (10,753 final zeros; 34,972 accounts) | 91,025 | 0 projection errors, 0 clock or continuity mismatches; oracle 82/82; prototype 82/82 |

Continuity checks compare an account's `old_amount` with its last emitted
amount across consecutive replayed blocks; the 1,024-block window
[122288006, 122289030) contributes most of them. Blocks separated by gaps are
linked only by hash where consecutive. The replay executes the Rust reducer,
not packaged WASM, and reads no network. It does not establish global holder
coverage: 30,374 distinct accounts changed in the replayed blocks, and
accounts without a change are unknown.

## Historical evidence retained

- Block 122260950 with 82 native and 18 WBNB same-block RPC rows:
  [`bsc-122260950.json`](../../../erc20/balances/tests/fixtures/bsc-122260950.json).
- The prototype's 82 native old/new/ordinal rows for that block:
  [`bsc-122260950-prototype-native-rows.json`](../tests/fixtures/bsc-122260950-prototype-native-rows.json),
  extracted from the retained 64-block run output whose SHA-256 is recorded in
  the fixture.
- 192 blocks, 37,086 native before/after RPC checks with zero mismatches for
  the prototype package: [legacy qualification](https://github.com/pinax-network/substreams-evm-balances/blob/9b41c7f/erc20/balances/docs/legacy-qualification.md),
  [64-block](https://github.com/pinax-network/substreams-evm-balances/blob/9b41c7f/erc20/balances/docs/evidence/rpc-exhaustive-64.json) and
  [128-block](https://github.com/pinax-network/substreams-evm-balances/blob/9b41c7f/erc20/balances/docs/evidence/rpc-exhaustive-128.json)
  summaries. The Extended blocks of those windows are not retained locally,
  so they cannot be replayed offline; the checks qualify the removed prototype
  artifact, not this package.
- The failed v0.3.3 reference comparison that overwrote the fee reset
  ([64-audited](https://github.com/pinax-network/substreams-evm-balances/blob/9b41c7f/erc20/balances/docs/evidence/64-audited.json)). This
  is a finding about that artifact's ordering, not about the current
  `substreams-evm` native module.

## Remaining gates

The BSC version-5 package `fbb46fc7…` completed packaged/same-block-hash
qualification in [#55](https://github.com/pinax-network/substreams-evm-balances/pull/55):
[saved controls](evidence/live-parity-bsc-2026-09-23-saved-controls.json)
cover [122288006, 122289030), and the
[live report](evidence/live-parity-bsc-2026-09-23-live.json) covers emitted
accounts in [123548070, 123553070), with 200,344 matching RPC comparisons.
The interval includes one empty-output block; emitted rows do not enumerate
all native accounts or establish their initialization.

The [exact checkpoint report](evidence/live-retention-checkpoint-bsc-2026-09-23.json)
then initializes 14,172 tracked accounts at block 123548069 and applies
[123548070, 123548370), with 56,688 matching comparisons across four
checkpoints. Accounts outside that tracked set remain unknown.

- Sink publication is this package's own `db_out` into
  `<chain>:evm-balances-native@<version>`
  ([ClickHouse](../../../docs/clickhouse.md)); until v0.6.1 it was the
  combined `evm-balances` package. Its completeness checks are in the
  [cutover contract](../../../docs/initialization-and-completeness.md) (#7),
  and run live during
  [#124](https://github.com/pinax-network/substreams-evm-balances/issues/124)
  B2. The sink runs with an undo buffer (BSC: 5 blocks), not final blocks
  only (owner, 2026-10-13): a fork deeper than the buffer stops it and leaves
  the rows of the orphaned blocks, an accepted risk, and no live reorg
  handling is qualified.
- Qualify the unobserved reward, DAO, withdrawal and lifecycle cases, the
  chain-specific shapes the matrices above list without a fixture (such as
  Base's and Unichain's version 5 failed deposits and Arbitrum One's later
  failed submissions), and genesis outside BSC: on BSC through the
  full-history noops of
  [#124](https://github.com/pinax-network/substreams-evm-balances/issues/124),
  elsewhere per network under
  [#8](https://github.com/pinax-network/substreams-evm-balances/issues/8),
  which also holds the failed-root rules.
- Add per-network producer/fork fixtures, package controls and asset identity
  under [#8](https://github.com/pinax-network/substreams-evm-balances/issues/8).
  The recorded BSC results do not qualify other networks or package digests.
