# native/balances: evm-balances-native

An RPC-free `map_events` reads Firehose Extended blocks and emits
**`evm.balances.v1.NativeEvents`** for native account balances, and `db_out`
writes them into the substreams-evm ClickHouse tables (`native_balances`,
`blocks` and the native history), whose schema the spkg embeds. The package is
`evm_balances_native`: release tag and spkg `evm-balances-native-v<version>`,
BSC database `bsc:evm-balances-native@<version>`
([ClickHouse](../../docs/clickhouse.md)). ERC-20 balances are the separate
[`erc20/balances`](../../erc20/balances/README.md) package with their own database.

`NativeEvents` is the shared [`proto/v1/balances.proto`](../../proto/v1/balances.proto)'s
`Events` without the contract, which native amounts never have: field 1 is
reserved and the other numbers are `Events`' and `Balance`'s, so the bytes
also decode as `Events` with no contract. `NativeBalance.amount` is the
account's balance after the last persisted change in the block, as an exact
decimal `uint256` string.

This package ports the reducer of the historical `evm-balances-storage`
prototype ([source](https://github.com/pinax-network/substreams-evm/blob/311f9005cc8606e9cb1034562b97cf7281f30c99/evm-balances-storage/src/lib.rs))
without its WBNB hardcode, custom protobuf, extra maps, `db_out` or Python
helpers. The [current RPC native module](https://github.com/pinax-network/substreams-evm/blob/970a665e15619de8ad7f686bd89412a1030d46dc/native/balances/src/lib.rs)
discovers candidate addresses and calls `eth_getBalance`; its heuristics are
comparison context, not persistence rules, and are not ported.

## What a row means

Balances are **account state**. Every persisted `BalanceChange` record of the
block is collected by the shared [`evm-persist`](../../common/persist) rules,
sorted by execution ordinal (in a Polygon state-sync transaction, by call and
then ordinal; see [execution order](#execution-order-v078)) and reduced per
account. The row is the final value; the reducer also retains the value before
the account's first change and the ordinals of its first and last change for
host-side checks.

- An account without a persisted change in the block is **not emitted**.
  Absence is no observation; it never initializes an untouched account to zero.
- An account that changes and returns to its starting value **is emitted**
  (a net-zero changing account is different from an individual no-op record,
  which the persistence rules drop).
- Final known zeros are emitted as `"0"`.
- The zero address, precompiles, BSC system contracts and burn-looking
  addresses are ordinary accounts. A credit to `0x…dead` is a balance change,
  not a supply statement.
- An absent `old_value`/`new_value` message **inside an observed record**
  encodes zero under the pinned Ethereum protobuf contract. This does not turn
  a missing record into a zero balance.
- Value-bearing calls and transfer logs are not summed: balances are not a
  sum of inferred transfers, and transfer lists do not cover every
  account-state effect. Call values and a transaction's value and gas fields
  are read only to check that records match an exact producer shape (such as
  the self-transfer pair, the Polygon state sync, the fee vault credit and the
  failed-transaction settlements below); every amount comes from the
  producer's records.
- Rows are sorted by account and carry no block hash; the stream's clock and
  cursor identify the block. An empty output block is a block with no
  persisted native change, not a missing block.
- Block 0 emits the genesis allocations (see [Genesis](#genesis-v070)).

## Persistence and fail-closed rules

Which records persist is decided by [`evm-persist`](../../common/persist):
non-reverted frames of succeeded transactions; only `GAS_BUY`, `GAS_REFUND` and
`REWARD_TRANSACTION_FEE` from the root call of a failed or reverted
transaction (and a failed SetCode transaction's authorization records, which
move no balance); non-reverted system calls; every block-level record. Native
also persists, each in its exact shape, the records of a failed transaction
that some chains keep whatever the outcome, and refuses a failed transaction's
record outside its execution that moves a balance and persists by no rule
([failed transactions](#failed-transactions)).

The reducer rejects the whole block instead of guessing when the producer
data is ambiguous, incomplete or in a shape no rule pins:

| Condition | Error |
| --- | --- |
| Block is not `DETAILLEVEL_EXTENDED` | `Extended blocks required` |
| Missing header, non-32-byte hash/parent/state root, header number mismatch | `missing header` / `invalid block identity` / `header number mismatch` |
| Block 0 with a balance change that is not a `GENESIS_BALANCE` from zero | `genesis block carries a balance change that is not a genesis allocation` |
| Transaction without status 1–3 or without calls | `incomplete transaction persistence data` |
| Failed/reverted transaction whose root call carries a reason other than the three gas reasons, a blob transaction's `REWARD_BLOB_FEE`, a failed deposit's `INCREASE_MINT` in its exact shape, or the reasons known to revert (`TRANSFER`, `TOUCH_ACCOUNT`, `SUICIDE_REFUND`, `SUICIDE_WITHDRAW`, `CALL_BALANCE_OVERRIDE`, `BURN`) | `failed transaction carries a balance-change reason without pinned persistence semantics` |
| Failed SetCode transaction whose root call records `begin_ordinal` 0 and whose authorization records are not in the exact shape that gives the boundary (shared rules), also once the sender's own authorization with the transaction's nonce, which cannot apply, is taken as discarded (v0.7.9) | `failed SetCode transaction has no authorization/execution ordinal boundary` |
| A transaction with the signature of a Polygon `Block.ver` 3 state-sync transaction, outside the recognized shape, whose depth-1 subtrees' records do not follow call order | `unrecognized Bor state-sync ordinal layout` |
| Failed Arbitrum `ArbitrumRetryTx` (type 104) whose root call moves a balance outside the exact ArbOS settlement | `failed Arbitrum retry transaction without the exact ArbOS settlement shape` |
| Failed Arbitrum deposit, retryable submission or internal transaction (types 100, 105, 106) with a record that moves a balance, other than a gas record of its root call or an exact submission settlement | `failed Arbitrum deposit, retryable submission or internal transaction carries balance records that ArbOS keeps` |
| A persisted call self-destructs an account that no CREATE of its transaction creates, and that account's `SUICIDE_WITHDRAW` and `SUICIDE_REFUND` records, no-ops left out, are exactly `d -> 0`, `0 -> d` and `d -> 0` in ordinal order, `d` positive, other than the exact shape native drops on Arbitrum since ArbOS 20 (v0.7.9; [continuity exceptions](#continuity-exceptions)) | `unrecognized self-destruct to itself: 0x<account> in transaction 0x<hash> is recorded as debited, credited and debited again to zero, but an account not created in its transaction keeps its balance (EIP-6780)` |
| Address not 20 bytes | `invalid native account address` |
| Persisted record with ordinal 0 | `persisted native balance change has no execution ordinal` |
| Value longer than 32 bytes | `native balance exceeds uint256` |
| Two records of one account at the same place in execution order (the ordinal; in a recognized Polygon state-sync transaction, the depth-1 call, then the ordinal) | `ambiguous native balance execution order` |
| A record's old value differs from the account's previous new value, apart from the nine [continuity exceptions](#continuity-exceptions) | `discontinuous native balance changes within block` |
| A record of a failed transaction's root call that moves a balance and does not persist, after the execution (past the root call's end ordinal) or before it (at a positive ordinal below the root call's begin ordinal); checked after the reduction, so the errors above come first | `failed transaction carries a balance record after its execution without pinned persistence semantics` / `failed transaction carries a balance record before its execution without pinned persistence semantics` |
| The same, for a root call without an end ordinal in a transaction with one | `failed transaction carries a balance record that its root call's missing end ordinal cannot place` |
| A persisted transaction call without the self-destruct flag holds a `SUICIDE_REFUND` outside the exact shape whose debit native restores, other than a self-destruct to itself recorded as the account's own refund from zero (v0.7.11; [continuity exceptions](#continuity-exceptions)); checked last, after the reduction and the failed-transaction records above, so their errors come first | `unrecognized self-destruct without the self-destruct flag: call <index> of transaction 0x<hash> holds a SUICIDE_REFUND outside the shape whose debit is restored, so the balance of its execution context 0x<account> after the self-destruct is unknown` |

Below, *the first v0.7.8 build* is the source of `cfc2923` (2026-10-14), with
four of the v0.7.8 rules: the self-destruct debit in a call with other
records, HyperEVM's self-transfer pair, and Polygon's and Arbitrum One's
failed-transaction fees. Its CI package (committed in `c07f06d`, `map_events`
`e44375c0…`) ran the pre-release noops of 2026-10-14, and local builds of the
same source ran the spot checks.

### Continuity exceptions

Nine producer shapes are exceptions to the continuity check, each with
captured fixtures: the self-transfer pair, the self-destruct to itself,
Arbitrum One's repeated self-destruct withdraw and its repeated withdraw of
an account that self-destructs to itself since ArbOS 20 are dropped; the fee
pool payout and the self-destruct debit are restored; the beneficiary's
records after a self-destruct of an account created in the transaction are
shifted; and Base's fee vault credit after the vault's transfers during the
execution and Arbitrum One's withdraw of a legacy self-destruct to itself
are rewritten. Every other record of the account must still chain across
them. One related shape, a self-destruct to itself that would chain to a
wrong zero, fails the block instead (below), outside the exact shape that
native v0.7.9 drops on Arbitrum One. Since v0.7.11 so does a
`SUICIDE_REFUND` in a persisted transaction call without the self-destruct
flag outside the exact shape whose debit native restores, other than a
self-destruct to itself recorded as the account's own refund from zero
(below).

**Self-transfer pair (v0.2.2, v0.7.8).** A value transfer from an account to itself,
a CALLCODE with value or a CALL whose caller is its address, does not move the
balance, and geth records nothing for it. The BSC producer records it as two
`TRANSFER` records of that account at consecutive ordinals of the call, both
computed from the same prior balance `b`: `b -> b - v` and `b -> b + v`, where
`v` is the call's value. The second record's old value breaks continuity
although the balance never moved. In a succeeded transaction's non-reverted
call, exactly this pair is dropped, so the account's other records must chain
across it. Any other shape, including the same records under another call
type, value or ordinal gap, still fails the block. v0.2.1 refused BSC block
127,411,449 on it (transaction `0x5c664637…`, account `0x1a84c890…`, a
CALLCODE of 0.08 BNB); the captured transaction is the regression fixture
[`tests/fixtures/bsc-127411449-callcode-self-transfer.json`](tests/fixtures/bsc-127411449-callcode-self-transfer.json).
The HyperEVM producer records a self-transfer as four `TRANSFER` records:
first two at consecutive ordinals that both end at the balance `b`,
`b + v -> b` and `b - v -> b`, then the actual debit and credit
`b -> b - v -> b`. Since v0.7.8 native also drops that first pair (either
order), and the account's other records must chain across it. Native v0.7.7
refused HyperEVM block 44,000,001 on it in the 2026-10-14 spot check
(transaction `0xbda5325e…`, a CALL of `0x20f6ee51…` to itself with 0.00006
HYPE); the transaction is the fixture
[`tests/fixtures/hyperevm-44000001-self-transfer.json`](tests/fixtures/hyperevm-44000001-self-transfer.json).

**Fee pool payout (v0.7.0).** BSC collects a block's transaction fees in its
fee pool, `consensus.SystemAddress` `0xff…fe`, and pays them out at the end of
the block: 1/16 of the pool (`pool >> 4`) to the system reward contract
`0x…1002`, the rest to the validator contract `0x…1000`. Each payout moves the
amount from the pool to the block's coinbase, which sends it on in a system
transaction. The producer of `Block.ver` 3 blocks records the first payout's
pool debit with its old value equal to its new one: the client lowered the
pool's balance in place before setting it, so the tracer read the lowered
balance as the old value. That block-level `REWARD_TRANSACTION_FEE` record
reads `n -> n`, the shared rules drop it as a no-op, and the pool's next record
starts from `n`, so continuity broke although the balance moved exactly as the
coinbase's credit records. When the next block-level record, before any
transaction starts, is a fee credit `r` of the block's coinbase with
`r = (n + r) >> 4`, native restores the debit as `n + r -> n`, and the
continuity check verifies `n + r` against the pool's previous balance. Any
other shape still fails the block. The credit is one or two ordinals after the
`n -> n` record: the producer can skip an ordinal between them. Native v0.2.2
refused BSC blocks 363, 12,700,000 and 25,401,003 on this shape, found by the
full-history noop on 2026-10-13 (every range below block 38.1M failed); v0.7.0
required the credit at the very next ordinal and refused blocks 958 and 4104,
where the producer skipped one, which v0.7.1 accepts. The five blocks are the
fixture
[`tests/fixtures/bsc-fee-pool-payout-ver3.json`](tests/fixtures/bsc-fee-pool-payout-ver3.json)
(blocks 363, 958 and 4104 whole, the others from the pool's last transaction
fee on).

**Self-destruct debit (v0.7.3).** A self-destruct moves the destroyed
account's whole balance to its beneficiary. Since Cancun (EIP-6780) the
account is deleted only when it was created in the same transaction, and for
the others the Ethereum producer of `Block.ver` 3 records only the
beneficiary's `SUICIDE_REFUND` credit `d`: the account's debit to zero is
missing, and its next record starts from zero. (The producer of `Block.ver` 5
records the debit as a `SUICIDE_WITHDRAW` before the credit, as in block
25,741,008, transaction `0xeb4fc94a…`; this shape does not match it.) When a
persisted call (non-reverted, in a
succeeded transaction) self-destructs, holds exactly one balance record, a
positive `SUICIDE_REFUND` credit of another account, and no record of the
destroyed account, native restores that account's debit `d -> 0` at the
credit's ordinal, and the continuity check verifies `d` against its previous
balance. The destroyed account is the call's execution context: its caller
for DELEGATECALL and CALLCODE, its address otherwise. Any other shape still
fails the block. Native v0.7.2 refused Ethereum block 20,249,174 on it, found
by the Ethereum full-history noop on 2026-10-14 (transaction `0x16bb56d4…`, a
DELEGATECALL self-destruct of `0x0000000000bb3d7b…` that sends 0.81 ETH to
`0x475f8f47…`). The transaction is the fixture
[`tests/fixtures/eth-20249174-selfdestruct-6780.json`](tests/fixtures/eth-20249174-selfdestruct-6780.json).

**Self-destruct debit in a call with other records (v0.7.8).** The
self-destructing call may also hold other records, for example the gas
records and the value credit of a contract-creation transaction's root call,
when it holds exactly one `SUICIDE_REFUND`, a positive credit of another
account, no `SUICIDE_WITHDRAW` and no record of the destroyed account after
the credit, and the transaction has no `SUICIDE_WITHDRAW` of the destroyed
account. A producer that records the debit in another shape breaks the
continuity check instead. Polygon's producer of `Block.ver` 3 (to block
73,440,255; `Block.ver` 4 from 73,440,256 records the withdraw) records no
`SUICIDE_WITHDRAW` at all, and from Napoli no self-destruct flag either for
an account not created in the transaction (v0.7.11, below). The Ethereum
producer of `Block.ver` 3 records none after Cancun for an account created in
the transaction either. An account
created and self-destructed in one transaction then kept, in v0.7.7's
output, the balance it was created with: a wrong balance, not a refused
block, since nothing after it breaks continuity, so the full-history noops
passed. Same-block `eth_getBalance` spot checks found it on 2026-10-14:
Polygon block 70,000,124 (transactions `0x4b2ad563…` and `0xf83e56fd…`, whose
CREATE root calls credit `0x3e58cebd…` and `0xa136fffa…` and self-destruct
them to `0x91cea390…`;
[fixture](tests/fixtures/polygon-70000124-selfdestruct-created.json)) and
Ethereum block 19,500,178 (transaction `0xa7aa96be…`, whose CREATE root call
creates `0x727f6fc6…` with 0.001 ETH, credits it six more times and
self-destructs it to the sender;
[fixture](tests/fixtures/eth-19500178-selfdestruct-created-root.json)). A
scan of sample ranges found 33, 23 and 12 such transactions in 2,000 Polygon
blocks from 60,000,000, 56,000,000 and 62,000,000, and 1 in 1,000 Ethereum
blocks from 19,500,000. Ethereum before Cancun, Ethereum's `Block.ver` 5
producer, Polygon's `Block.ver` 4 producer and BSC's producers record the
withdraw.

**Self-destruct debit without the self-destruct flag (v0.7.11, Polygon).**
From Napoli (block 54,876,000, Bor's Cancun, which brings EIP-6780) an
account not created in its transaction that self-destructs to another
account survives with a zero balance. Polygon's producer of `Block.ver` 3
records only the beneficiary's `SUICIDE_REFUND` of the account's whole
balance `d`, in a call without the self-destruct flag, with no
`SUICIDE_WITHDRAW` and no record of the account. It sets the flag on the
self-destruct of an account created in the transaction (block 70,000,124
above). Native v0.7.3 to v0.7.10 restore the debit only in a flagged call,
so the account kept `d`: a wrong balance, not a refused block, since no later
record of the account breaks continuity. The
[native quality comparison](../../docs/quality-native.md#polygon-missed-self-destruct-debits)
of 2026-10-19 found 61 contracts for which
`polygon:evm-balances-native@v0.7.8` keeps 1,694.17 POL in all and the
RPC-era database reads zero, from block 55,412,000 to 71,461,200. 58 of the
61 contracts are located in this shape (1,692.97 POL), each credit equal to
the balance v0.7.8 kept: 56 in the blocks of those zero readings, fetched
on 2026-10-20, and 2 more in blocks 56,042,508 and 58,697,458, fetched the
same day, whose zero-reading blocks hold no refund or record of them. The
last 3 (1.2 POL) lie in blocks not fetched. Since v0.7.11
native restores the debit `d -> 0` at the credit's ordinal also for a call
without the flag, in the v0.7.8 shape (one positive `SUICIDE_REFUND` of
another account, no `SUICIDE_WITHDRAW` in the call and none of the account
in the transaction) and four more conditions:

- the call is not a STATICCALL, which cannot self-destruct;
- no persisted CREATE of the transaction creates the account;
- in a Bor state-sync transaction, whose subtrees restart the ordinals, the
  transaction has no other persisted balance record of the account at all
  (elsewhere, since v0.7.12, its records before the refund chain into the
  debit, below; v0.7.11 refused them);
- the refund is the call's last act, as a `SELFDESTRUCT` ends its frame: it
  lies after the call's begin ordinal and before its end ordinal, it is the
  call's only balance record from its ordinal to the end ordinal, and every
  child call of the call ends before it (a refund within a child's ordinals
  belongs to the child's execution). In all 58 it is at the end ordinal less
  one, and the one call with child calls (block 56,243,453) has two, both
  ending before it; a root call's records after its end ordinal (gas
  refund, base-fee `BURN`, fee) do not matter.

Persisted means in a call that is not reverted, as the shared rules keep
it: a reverted call's records and CREATE do not count. As for a flagged
call, the account's records in earlier transactions of the block chain into
the debit, and its later records, in a later call of the transaction or in
a later transaction, chain from its zero: an account not created in its
transaction survives an EIP-6780 self-destruct and can be credited again.
The rule is not gated to a network, a `Block.ver` or a fork: a
self-destruct to another account leaves the account at zero under the
legacy opcode and EIP-6780 alike. Without the flag the refund is the only
evidence of the self-destruct, so at the owner's direction (2026-10-20) a
near-shape fails the block instead of keeping a balance that may be wrong: a
persisted transaction call without the flag that holds a `SUICIDE_REFUND`
and gets no restored debit, such as the self-destruct of an account created
in its transaction, two refunds, a refund that moves nothing, a refund of
the account itself from a positive balance, a `SUICIDE_WITHDRAW` in the
call, or a refund that does not end the frame. Most of these would keep the balance that the
self-destruct moved away. A few may keep the right balance, such as a debit
that the producer records itself, whose records chain to zero, but no
capture shows them without the flag, so they stop the stream at their
block, for a capture and a decision. One shape is not refused: a
self-destruct to itself recorded as the account's own refund from zero to
`d`, its only record in the transaction, of an account not created there,
in a call that is not a STATICCALL and holds no `SUICIDE_WITHDRAW`, the
refund ending the frame. Its records end at `d`, the balance EIP-6780 leaves
the account, and are kept as they are, as up to v0.7.10. That is how
Polygon's producer would record one, judging by its 58 captured
self-destructs and by Ethereum's `Block.ver` 3 refund from zero to itself
(below); none is captured. The refusal fails no saved block: in the saved
blocks and fixtures of every supported network, every `SUICIDE_REFUND` in a
call without the flag is one of the 58 restored (none is in a system call or
at block level). A reverted call's or a failed transaction's refund does not
persist and is not checked. Like v0.7.8, and unlike v0.7.9 and v0.7.10,
v0.7.11 can therefore refuse blocks that the earlier releases accept. That
is untested on Polygon `Block.ver` 4 and 5 (from 73,440,256), on Polygon
before Napoli and on the networks with no saved self-destruct of a positive
balance (BSC, Base, Optimism, Unichain and HyperEVM): the pre-release noop
and Polygon's full-history noop are its test there. Unlike v0.7.9 and
v0.7.10 (and like v0.7.8's debit in a call with other records), the rule
also changes rows of blocks that the earlier releases accept: each block
with the shape gains the destroyed account's row. The 58 found are Polygon
`Block.ver` 3 blocks from 55,412,000 to 71,461,200.
Fixtures: block 55,741,124
(transaction `0x055a91ac…`, whose root call from `0xd55ffe27…` self-destructs
`0xeb0de914…`, 1,391.16 POL, to the sender;
[fixture](tests/fixtures/polygon-55741124-selfdestruct-unflagged-root.json))
and block 55,412,000 (transaction `0xe8d9e1d3…`, whose depth-1 call from
`0xd98be101…` self-destructs `0xeba4fc1a…`, 19.8 POL, to that caller;
[fixture](tests/fixtures/polygon-55412000-selfdestruct-unflagged-nested.json)).

**Self-destruct debit after earlier records of the account (v0.7.12,
Polygon).** v0.7.11 also refused a self-destruct without the flag of an
account with a persisted record earlier in its transaction, a near-shape no
saved block held. Its Polygon backfill and full-history noop of 2026-10-29
stopped on it in 9 blocks, from 56,954,084 to 72,387,144, each the first
refusal of a 490,000-block range, all in root calls. In 8, a transaction
sends POL to an existing contract, from 1 wei to 100 POL, and the contract
self-destructs in the same call: the contract's `TRANSFER` credit precedes
the beneficiary's `SUICIDE_REFUND` (block 58,316,117, transaction
`0xf5f9b9ed…`, 0.05 POL to `0x2d2644a9…`). In block 68,559,510
(transaction `0x6b852033…`) the contract `0xd3b87fc7…` pays 0.0001 POL in a
call before it self-destructs. The RPC-era rows read zero for each contract
at its block, where native v0.7.8 and v0.7.10 keep the balance those records
leave (96.16 POL in all in v0.7.8's database at block 95,689,999).
A same-block RPC-era zero there looks like an RPC-era pre-change value, the
[quality comparison](../../docs/quality-native.md#polygon)'s class A, so
that comparison did not single them out. At the owner's choice (2026-10-30)
v0.7.12 makes the account's records before the refund part of the shape:
they chain into the restored debit, whose old value is the refund's amount,
so the continuity check requires the balance they leave to be exactly what
the self-destruct moves, as v0.7.11 already requires of a record in an
earlier transaction of the block, and a debit that does not chain fails the
block. A Bor state-sync transaction keeps v0.7.11's rule, since its ordinals
cannot place a record before the refund. Every other near-shape above still
fails the block. v0.7.12 writes the same rows as v0.7.11 for every block
v0.7.11 accepts, since it only restores debits that v0.7.11 refuses. The 9
blocks, fetched on 2026-10-30 and replayed whole, pass with each contract at
zero; their other 1,092 rows equal v0.7.10's. Fixtures: block 58,316,117
(transaction `0xf5f9b9ed…`;
[fixture](tests/fixtures/polygon-58316117-selfdestruct-unflagged-after-credit.json)),
block 68,559,510 (transactions 46 to 48, which create `0xd3b87fc7…`, fund
it and have it pay out and self-destruct;
[fixture](tests/fixtures/polygon-68559510-selfdestruct-unflagged-after-payment.json))
and block 72,387,144 (transaction `0xad39d8c2…`, whose contract calls itself
with the credited value before it self-destructs;
[fixture](tests/fixtures/polygon-72387144-selfdestruct-unflagged-after-self-calls.json)).


**Self-destruct to itself (v0.7.5).** Since Cancun, an account not created
in the same transaction that self-destructs to itself keeps its balance:
geth debits it and credits it back. The Ethereum producer of `Block.ver` 3
records the credit as a `SUICIDE_REFUND` from zero to `d`, without the debit,
and then a
`SUICIDE_WITHDRAW` from `d` to zero at the next ordinal, although the account
keeps `d`. When a persisted call self-destructs and holds exactly those two
records of the destroyed account (resolved as above), `d` positive, and the
transaction has no CREATE of that account, native drops both, and the
account's other records must chain across them. A legacy self-destruct to
itself, which burns the balance, credits from the prior balance and is kept.
Native v0.7.4 refused Ethereum block 19,481,732 on it (transaction
`0x833a78e6…`: `0x6550f9a4…` receives 1 wei and self-destructs to itself in
a CALL to itself; a block explorer reads 1 wei after the block). The
transaction is the fixture
[`tests/fixtures/eth-19481732-selfdestruct-to-self.json`](tests/fixtures/eth-19481732-selfdestruct-to-self.json).

**Self-destruct of an account created in the transaction (v0.7.6, v0.7.7).**
Since Cancun, a self-destruct destroys an account created in the same
transaction, and its balance `d` moves to the beneficiary. The Ethereum
producer of `Block.ver` 5 records the account's `SUICIDE_WITHDRAW` from `d` to
zero and, at the next ordinal, the beneficiary's `SUICIDE_REFUND` from `b` to
`b + d`. The beneficiary's records during the rest of the execution include
the credit, but its records after the execution (the gas refund, a fee
credit) leave out every such credit of the transaction, by their sum `c`. The
balance after the transaction includes them.

Since v0.7.7, a credit counts when a persisted call self-destructs an account
created by a CREATE in the transaction and holds exactly one
`SUICIDE_WITHDRAW`, of that account, from `d` to zero, and exactly one
`SUICIDE_REFUND`, of another account, from `b` to `b + d` at the next ordinal
during the execution, `d` positive and the refund persisted as recorded; the
call's other records are not part of the shape. Native then adds `c` to the
old and new values of each of the beneficiary's records after the execution
(after the root call's end ordinal, before the transaction's) when the first
of them starts exactly `c` below the beneficiary's last balance during the
transaction and each moves the balance by the amount the transaction
implies: a `GAS_REFUND` by `(gas_limit - gas_used) * gas_price`, a
`REWARD_TRANSACTION_FEE` by `gas_used * (gas_price - base_fee)`; so only
records whose old and new values both leave out `c` are shifted. The
continuity check then verifies them. Records during the execution and after
the transaction are not shifted; any other shape is not shifted, and fails
the block only when the continuity check fails (records that already include
the credits pass). v0.7.6
added `d` to every later record of the beneficiary in the transaction, for a
single credit, which the captured blocks below contradict once a transfer or
another credit follows the first.

Captured, each with a block explorer reading the credits in the beneficiary's
balance after the block:
- block 25,741,008 (transaction `0xd6299894…`; native v0.7.5 refused it): a
  CREATE at `0x1af6cfe5…`, which already holds 0.159 ETH, self-destructs in
  its constructor to the sender `0xc6acbee4…`, whose gas refund starts from
  the balance before the credit
  ([fixture](tests/fixtures/eth-25741008-selfdestruct-created-credit.json));
- block 25,747,303 (`0x97a06638…`; v0.7.6 refused it): the self-destructing
  call is the root call of a contract-creation transaction, which also holds
  the gas and fee records
  ([fixture](tests/fixtures/eth-25747303-selfdestruct-creation-tx.json));
- block 25,869,452 (`0x90f5b5c9…`): four credits to the sender, each refund
  chaining from the previous one, and a gas refund short by their sum
  ([fixture](tests/fixtures/eth-25869452-selfdestruct-four-credits.json));
  block 25,750,184 (`0x04812d41…`) has five;
- block 25,920,498 (`0x06fc62ea…`): the self-destructing CREATE also carries
  value, and a transfer from the sender during the execution starts from the
  credited balance
  ([fixture](tests/fixtures/eth-25920498-selfdestruct-transfer-during-execution.json)).

An earlier v0.7.7 build (the CI package of `0ac0c20`, module `map_events`
`1e379e88…`) refused blocks 25,750,184, 25,869,452 and 25,920,498 in its
pre-release noop; the released v0.7.7 accepts them.

**Repeated self-destruct withdraw (v0.7.8, v0.7.9, v0.7.10, Arbitrum One).** Arbitrum One's
producer of `Block.ver` 3 records a self-destructed account's debit, a
`SUICIDE_WITHDRAW` from `d` to zero, and then a second one from `d` to zero
although the account is already at zero; the chain reads zero for the account
after the block, and the beneficiary's `SUICIDE_REFUND` credits `d` once. The
repeat's old value breaks continuity. Native drops the repeat when a persisted
call self-destructs the account and holds exactly one `SUICIDE_REFUND`, a
credit of another account by `d` (positive; in the first order, since v0.7.10,
also of the account itself), and that refund and the account's
`SUICIDE_WITHDRAW` records follow one another at consecutive ordinals in one
of three orders:
- since Cancun (EIP-6780, ArbOS 20), of an account created by a CREATE in the
  transaction: the debit, the refund (two ordinals later when the call creates
  the beneficiary's account in between), a `SUICIDE_WITHDRAW` from zero to
  zero and the repeat; to the account itself, the refund from zero to `d` at
  the ordinal after the debit, then a `SUICIDE_WITHDRAW` from `d` to zero, the
  burn of the destroyed account, in place of that no-op;
- before Cancun, of any account: the refund, the debit and the repeat;
- since Cancun, of an account no CREATE of the transaction creates, which
  survives its self-destruct: the debit, the refund (since v0.7.9 also two
  ordinals later when the call creates the beneficiary's account in between)
  and the repeat.

The account's last persisted record before the repeat must be the debit (or
the burn), the repeat its only persisted record at that ordinal, and no record
of the account may follow it in the transaction. Dropping the repeat moves no
balance; any other shape still fails the block. No order is gated to a
network or an ArbOS version: an account created in its transaction ends at
zero after its self-destruct under the legacy opcode and EIP-6780 alike, also
to itself, unlike an account not created in it (the self-destructs to
themselves below). Sampled self-destructs of a positive
balance have no repeat up to block 169,366,089 and have it from block
169,454,286. Before this rule native refused Arbitrum One blocks 187,009,069,
200,029,376, 250,005,039, 300,007,082, 450,014,091 and 450,028,504 (the first
v0.7.8 build's pre-release noop failed in the 200M, 250M, 300M and 450M
windows on it) and, for accounts not created in the transaction, 176,115,455,
367,502,338, 367,510,845, 407,513,390, 407,514,998 and 514,819,388. The
fixtures, one transaction each:
[250,005,039](tests/fixtures/arbitrum-250005039-stale-selfdestruct-withdraw.json)
(transaction `0xe293fa3a…`, whose constructor self-destructs the account it
creates),
[200,029,376](tests/fixtures/arbitrum-200029376-stale-selfdestruct-withdraw.json)
(the credit creates the beneficiary's account),
[450,014,091](tests/fixtures/arbitrum-450014091-stale-selfdestruct-withdraw.json)
(a nested CREATE at an address that already holds 0.0581 ETH),
[450,028,504](tests/fixtures/arbitrum-450028504-stale-selfdestruct-withdraw.json)
and, before Cancun,
[187,009,069](tests/fixtures/arbitrum-187009069-stale-selfdestruct-withdraw.json);
of accounts not created in the transaction,
[514,819,388](tests/fixtures/arbitrum-514819388-stale-selfdestruct-withdraw-existing.json)
(`0x06bba4e1…`, which keeps its code),
[367,502,338](tests/fixtures/arbitrum-367502338-stale-selfdestruct-withdraw-existing.json)
and, before Cancun,
[176,115,455](tests/fixtures/arbitrum-176115455-stale-selfdestruct-withdraw-existing.json)
(`0x9d4eb718…`, whose code the chain removes). Native v0.7.8 refused
Arbitrum One blocks 223,944,083, 235,153,999, 282,198,061 and 344,105,344
in its full-history noops (2026-10-17 and 18): the root call self-destructs an account not
created in the transaction to a new account that the call creates between the
debit and the refund (transaction `0xd2061702…` of block 235,153,999:
`0x85483824…` to `0x0bb2668e…`, the debit at 12, the creation at 13, the
refund at 14, the repeat at 15). The RPC-era rows read zero for the destroyed
accounts at their blocks, apart from 282,198,061: its destroyed account
`0xbe233078…` has no row at that block, and its only RPC-era reading is zero
at block 282,200,265, 2,204 blocks later. They read 0.001 ETH for
`0x0bb2668e…` at block 235,153,999. v0.7.9 drops the repeat. Fixtures,
transactions 0 and 1 each:
[235,153,999](tests/fixtures/arbitrum-235153999-stale-selfdestruct-withdraw-new-beneficiary.json),
[223,944,083](tests/fixtures/arbitrum-223944083-stale-selfdestruct-withdraw-new-beneficiary.json)
and
[344,105,344](tests/fixtures/arbitrum-344105344-stale-selfdestruct-withdraw-new-beneficiary.json)
(ArbOS 32, where the RPC-era rows also read the beneficiary's
100,000,000,000,000 wei and the sender's balance at the block). Native
v0.7.9 refused Arbitrum One block 500,689,520 (ArbOS 61) in its full-history
noop (2026-10-18): transaction `0x584f566f…` sends 1,000,000,000,000 wei to
`0xe85ee3d5…`, whose CREATE deploys `0x09bb50a3…` with it, and the next call
self-destructs `0x09bb50a3…` to itself: the debit at 128, the refund of the
account itself from zero at 129, the burn at 130 and the repeat at 131. The
RPC-era rows read zero for `0x09bb50a3…` and `0xe85ee3d5…` at the block.
v0.7.10 drops the repeat, and the account ends at zero. Fixture, transactions
0 and 3:
[500,689,520](tests/fixtures/arbitrum-500689520-selfdestruct-created-to-self.json).

**Self-destruct to itself recorded as debit, refund and repeat (v0.7.8
refused; v0.7.9 dropped on Arbitrum One since ArbOS 20).** Since Cancun (on
Arbitrum One ArbOS 20, from block 190,301,731) an account not created in the
transaction keeps its balance `d` when it self-destructs to itself. Arbitrum
One's producer records the self-destruct of such an account to another
beneficiary as the debit, the beneficiary's refund and the repeat (above); to
itself, that order is a `SUICIDE_WITHDRAW` from `d` to zero, a
`SUICIDE_REFUND` of the account from zero to `d` and the repeat from `d` to
zero. These records chain and end at zero while the chain keeps `d`, so no
continuity check would see the wrong balance. Native v0.7.8, with no capture,
refused the shape on any network: a persisted call that self-destructs an
account no CREATE of its transaction creates fails the block when that
account's `SUICIDE_WITHDRAW` and `SUICIDE_REFUND` records, no-ops left out,
are exactly these three in ordinal order, `d` positive (gaps between their
ordinals do not matter). Its full-history noop refused Arbitrum One block
326,011,151 (2026-10-17): transaction `0x2526bf6a…` sends 10 wei to
`0xac2e23b1…`, whose root call self-destructs it to itself at ordinals 14 to
16, and the RPC-era row reads 10 wei for it at the block. Since v0.7.9 native
drops the repeat, and the account ends at `d`, only in an Arbitrum block
since ArbOS 20 (read from the block itself, as for the legacy self-destruct
to itself below) and only when the call's `SUICIDE_WITHDRAW` and
`SUICIDE_REFUND` records, no-ops included, are exactly these three, all of
the account, at consecutive ordinals, the refund the account's last persisted
record before the repeat and no record of it after the repeat in the
transaction. The drop is computed before the
refusal, which exempts only a repeat that is dropped: a gap, an extra no-op
withdraw, another network or ArbOS before 20 still fail the block. Ethereum's
`Block.ver` 3 shape, the refund from zero and then the withdraw, is the
exception above, and a producer that records only the debit and the refund
ends at `d`. An account created in its transaction is destroyed even when it
self-destructs to itself: Arbitrum One records a burn before the repeat, and
the account ends at zero (repeated self-destruct withdraw above, v0.7.10). The
fixture is
[326,011,151](tests/fixtures/arbitrum-326011151-selfdestruct-to-self.json)
(both transactions and the mix hash); the tests also build the shape from
the 514,819,388 capture, with the refund turned to the destroyed account.

**Legacy self-destruct to itself (v0.7.9, Arbitrum One before ArbOS 20).**
Before Cancun a self-destruct credits the account's balance `d` to its
beneficiary and then zeroes the account; to itself, Nitro's `opSelfdestruct`
burns the balance. Arbitrum One's producer records that as a `SUICIDE_REFUND`
of the account from `d` to `2d` and then, at the next ordinal, a
`SUICIDE_WITHDRAW` from `d` to zero, the balance the opcode read before the
credit, so the withdraw's old value breaks continuity. Native v0.7.8 refused
Arbitrum One blocks 136,808,179 and 161,626,224 on it in its full-history
noops (2026-10-17). In block 161,626,224 the root call of transaction
`0xd0cc7c02…` from `0x11799622…` self-destructs `0xb01f8f52…`, which holds
3,304,510,818,628,470 wei, to itself next to the sender's gas and fee
records; in block 161,626,317 the same sender creates the account again with
no balance record, the RPC-era row reads zero for it there, and the sender's
`GAS_BUY` starts from its balance after block 161,626,224. In block
136,808,179 a nested call of `0x585194b7…` self-destructs `0xe37e799d…`
(1 wei) to itself. Since v0.7.9 native rewrites the withdraw to go from `2d`
to zero, so the continuity check verifies the refund's `d`, and the account
ends at zero. It does so only in an Arbitrum block before ArbOS 20: native
reads the version from the block itself (the block's first transaction is
Nitro's `ArbitrumInternalTx`, type 106, from ArbOS `0x…0a4b05`, and the
header's mix digest holds the send count, the L1 block number, the ArbOS
version and zero, eight bytes each), which is neither a parameter nor a
network allowlist. On every other network, and from ArbOS 20, the pair keeps
v0.7.8's refusal: a producer that computes both records from the prior
balance after EIP-6780 would record the same pair for an account that keeps
`d`. The shape is exact: exactly one refund and one withdraw in the call,
both of the destroyed account, at consecutive ordinals, `d` positive, the
refund the account's last persisted record before the withdraw, and no
record of it after the withdraw in the transaction. Fixtures:
[161,626,224](tests/fixtures/arbitrum-161626224-legacy-selfdestruct-to-self-root.json)
(the whole block) and
[136,808,179](tests/fixtures/arbitrum-136808179-legacy-selfdestruct-to-self.json)
(transactions 0 and 5), each with the mix hash. A legacy self-destruct to
itself between the repeat era's start (about block 169.4M) and ArbOS 20
would also carry the repeat; none is captured, and it still fails the block.

**Fee vault credit after a vault withdrawal (v0.7.8) or a transfer into it
(v0.7.9, Base).** An OP-stack
transaction pays its fee after the execution, in the root call: after the
sender's `GAS_REFUND`, `REWARD_TRANSACTION_FEE` credits of the coinbase, the
SequencerFeeVault `0x4200…0011` (the tip), of the BaseFeeVault `0x4200…0019`
(`gas_used * base_fee`) and of the L1FeeVault `0x4200…001a` (the L1 data fee,
the sender's `GAS_BUY` debit less `gas_limit * gas_price`). A vault's
`withdraw()` sends its whole balance away during the execution. Base's
producer of `Block.ver` 5 (from block 45,904,000) records the BaseFeeVault's
and the L1FeeVault's credit after such a withdrawal from the vault's balance
before the transaction instead of from zero; the vault's next record starts
from the fee alone. In a succeeded transaction in which the vault's persisted
records during the execution are `TRANSFER`s, the last to zero, and its only
record after the execution is a `REWARD_TRANSACTION_FEE` from its (positive)
balance before the transaction that moves exactly the fee, native rewrites
that credit to go from zero to the fee, and the continuity check verifies the
vault's next record against it. A credit that already starts from zero chains
and is not rewritten, and any other shape still fails the block. The
OperatorFeeVault `0x4200…001b`, empty on Base, Optimism and Unichain
(2026-10-14), is not part of the shape. When the withdrawal transaction holds
the vault's last record in the block (the rewritten credit), no later record
checks the rewritten value; RPC checked it in all 33 blocks below. Base's
`Block.ver` 4 producer, and the Optimism and Unichain producers of `Block.ver`
5, record the credit from zero. Every withdrawal of either vault on Base from
block 45,904,000 to 52,614,000 (33 blocks) has this shape, and native refused
all of them before this rule; the first v0.7.8 build's pre-release noop
stopped at block 50,024,530. The fixtures are
[50,024,530](tests/fixtures/base-50024530-fee-vault-stale-credit.json)
(transaction `0xbae6554e…` withdraws `0x4200…0011` and `0x4200…0019`) and
[50,326,929](tests/fixtures/base-50326929-fee-vault-stale-credits.json)
(`0x39a5e05e…` withdraws all three vaults, and the next transaction's credits
start from the fees). The producer does the same after a transfer into the
vault: in block 47,424,842, transaction `0x0d38b7d2…` sends
200,000,000,000,000 wei to the L1FeeVault, its credit after the execution
starts from the balance before the transaction, and the next transaction's
credit starts from the transferred balance plus the fee; native v0.7.8
refused the block in its full-history noop (2026-10-17). Since v0.7.9 native
rewrites the credit to go from the vault's last balance during the
execution, `l`, to `l` plus the fee whenever `l` differs from the balance
before the transaction (a withdrawal is `l` zero); when `l` is not zero, a
later persisted record of the vault in the block must check the rewritten
credit, or the block still fails. The vault ends block 47,424,842 at
346,953,702,305,146 wei, and the producer's records of blocks 47,424,843 to
47,424,846 chain from there to the RPC-era close at block 47,424,846. The
fixture is
[47,424,842](tests/fixtures/base-47424842-fee-vault-credit-after-transfer.json)
(transactions 71 and 72).

### Execution order (v0.7.8)

Records are reduced by ordinal, with one exception. Polygon's Bor client
commits a sprint's state-sync events (messages from Ethereum such as bridge
deposits) at the end of a block whose number is a multiple of the sprint
length, in a synthetic transaction. Its producer of `Block.ver` 3 (to block
73,440,255) records it as a succeeded transaction from and to the zero
address, with no gas used, whose root call, a CALL from and to the zero
address with begin ordinal 0, holds one depth-1 `commitState` call per event,
from the system address `0xffff…fffe` to the StateReceiver `0x…1001`, also
with begin ordinal 0. In blocks observed from 3,206,592 to 70,044,000 the
subtree of each depth-1 call restarts the ordinals just after the
transaction's begin ordinal, so the subtrees' ordinals overlap, while the
balances chain in call order. By ordinal alone such a block fails as
ambiguous or discontinuous. By block 73,439,280 the same producer numbers the
subtrees in sequence, which this rule orders as the ordinals do.

When the transaction has the exact shape (the block's last transaction, or the
one right before a last `commitSpan` transaction to the ValidatorSet
`0x…1000`, which the same producer adds at the first block of a span's last
sprint; each depth-1 subtree a contiguous run of calls; every record of the
transaction strictly within its ordinals and no other record of the block
among them; no self-destruct and no self-transfer in it), native orders its
records by depth-1 call, then ordinal, and the continuity check verifies them.
Rows keep the producer's ordinals, so there an account's `ordinal` can be
lower than its `first_ordinal`. A transaction with that signature outside the
shape whose subtrees' records do not follow call order fails the block
(`unrecognized Bor state-sync ordinal layout`) instead of keeping an ordinal
order that could chain by accident; none of 85 Polygon blocks checked from
3,206,592 to 80,000,464 has one. The `Block.ver` 4 producer gives these calls
their own ordinals and does not restart them.

Native up to the first v0.7.8 build refused such blocks (10,076,416,
20,007,040, 30,011,072, 40,033,040, 50,016,032, 54,876,080, 60,023,088,
65,045,168 and 70,044,000; that build's pre-release noop failed in 9 of 14
Polygon windows on it), and a build without the span commit refused blocks
3,206,592, 14,694,592, 20,025,792, 27,526,592 and 69,747,440. Native v0.7.7
and earlier also accepted a state sync whose records happen to chain in
ordinal order, which can give a wrong amount: none was observed, but their
Polygon `Block.ver` 3 output of state-sync blocks is unaudited for it. The
fixtures are [70,044,000](tests/fixtures/polygon-70044000-state-sync.json)
(transaction `0x60891cc1…`: WPOL `0x0d500b1d…` at ordinals 3,409, 3,408 and
3,408 in call order),
[20,007,040](tests/fixtures/polygon-20007040-state-sync.json) (two POL
deposits debit the MRC20 contract `0x…1010` at ordinal 13 each) and
[20,025,792](tests/fixtures/polygon-20025792-state-sync-span-commit.json) (the
state sync before a span commit).

### Failed transactions

The shared rules keep only the gas records of a failed transaction's root
call. Some chains keep more whatever the outcome, and native persists each
such record in its exact shape; like every record, it must chain. Any other
record outside the execution that moves a balance fails the block (the last
rule below).

**Failed blob transactions (v0.7.2).** BNB Chain credits a blob
transaction's blob fee (`REASON_REWARD_BLOB_FEE`, 17) to the fee pool after
execution, as it does the transaction fee, so a failed blob transaction pays
it too. The shared rules persist only the three gas reasons of a failed
transaction's root call; native also persists the root call's blob fee when
the transaction is a blob transaction (type 3), and the record must chain like
any other. Native v0.7.0 refused BSC block 42,357,369 on it, found by the
full-history noop on 2026-10-14: failed blob transaction `0xbe965c35…`
credits the pool 131,072 wei after its transaction fee, and the pool's next
record, the block-level payout to the coinbase, starts from the credited
balance. The fixture is
[`tests/fixtures/bsc-42357369-failed-blob-fee.json`](tests/fixtures/bsc-42357369-failed-blob-fee.json)
(the failed transaction and every later one, and every block-level record).

**Failed transactions' base fee (v0.7.8, Polygon).** Polygon's Bor client does
not burn a transaction's base fee: it credits `gas_used * base_fee` to its burn
contract `0x3ef57def…` with a `BURN` record in the root call, after the gas
refund and before the fee credit, and does so for a failed transaction too.
Native persists such a `BURN` of a failed transaction's root call when it moves
exactly `gas_used * base_fee` (base fee positive); any other `BURN` after the
execution fails the block (see records outside the execution, below), and one
during the execution reverts. Native v0.7.7 refused Polygon block 95,537,024 on
it (reverted transactions `0xad18a4f2…` and `0x20b9dd82…`); the fixture is
[`tests/fixtures/polygon-95537024-failed-base-fee.json`](tests/fixtures/polygon-95537024-failed-base-fee.json)
(transactions 3, 4 and 5).

**Failed transactions' fee transfers (v0.7.8, Arbitrum One).** Arbitrum's
ArbOS pays a transaction's fee after the execution, in the root call: after
the sender's `GAS_REFUND`, `TRANSFER` credits to the infrastructure fee
account `0xbf5041fc…`, the L1 pricer pool `0xa4b0…00f6` and, in later blocks,
the network fee account `0x32e7af5a…`, which may also take the tip as a
`REWARD_TRANSACTION_FEE`. These credits add up to the gas the sender paid, its
`GAS_BUY` debit less its `GAS_REFUND` credit, and ArbOS pays them for a failed
transaction too. Native persists those `TRANSFER` records of a failed
transaction's root call after the execution (ordinal past the call's end) when
the call holds exactly one `GAS_BUY` and at most one `GAS_REFUND` of the same
payer, and every other record after the execution is a positive `TRANSFER` or
`REWARD_TRANSACTION_FEE` credit of another account, at least one a `TRANSFER`,
adding up to the gas paid. In any other shape these records fail the block
(see records outside the execution, below). Native v0.7.7 refused every
Arbitrum One window of the 2026-10-14 spot check (blocks 150,000,072,
300,000,000, 450,000,000 and 514,857,274); the fixtures are
[`tests/fixtures/arbitrum-150000072-failed-fee-transfers.json`](tests/fixtures/arbitrum-150000072-failed-fee-transfers.json)
(transactions 3 to 8, four of them reverted) and
[`tests/fixtures/arbitrum-514857274-failed-fee-tip.json`](tests/fixtures/arbitrum-514857274-failed-fee-tip.json)
(transactions 8, 9 and 10, with the tip).

**Failed deposits' mint (v0.7.8, OP stack).** An OP-stack deposit (type
`0x7E`) mints its `mint` value to its sender before the execution, and the
[deposit specification](https://specs.optimism.io/protocol/deposits.html#execution)
keeps the mint when the deposit fails; a deposit buys no gas and pays no fee.
The Base, Optimism and Unichain producers of `Block.ver` 4 and the Optimism
producer of `Block.ver` 5 record it, also for a failed deposit, as an
`INCREASE_MINT` credit of the sender in the root call, after the transaction's
begin ordinal and before the execution's. Native persists the mint of a failed
deposit whose root call (depth 0) holds exactly one `INCREASE_MINT`, a
positive credit of the transaction's sender, and no other record between those
two ordinals; the deposit's reverted value transfer still reverts, and any
other `INCREASE_MINT` in a failed transaction's root call still fails the
block. The first v0.7.8 build's pre-release noop refused Base blocks 5,044,695
and 10,099,941 on the reason guard, and its discovery noops 3,037,177 and
7,092,606. The fixtures hold one failed deposit each: Base
[5,044,695](tests/fixtures/base-5044695-failed-deposit-mint.json)
(`0xe707514e…`, which fails outside the EVM, so its root call holds only the
mint) and
[7,092,606](tests/fixtures/base-7092606-failed-deposit-mint-reverted-call.json)
(`0x3869ca9e…`, whose call reverts), Optimism
[106,973,772](tests/fixtures/optimism-106973772-failed-deposit-mint.json) and
[107,494,341](tests/fixtures/optimism-107494341-failed-deposit-mint-reverted-call.json)
(`Block.ver` 4) and
[156,295,824](tests/fixtures/optimism-156295824-failed-deposit-mint.json)
(`Block.ver` 5), and Unichain
[744,856](tests/fixtures/unichain-744856-failed-deposit-mint-reverted-call.json).
No failed deposit of Base or Unichain `Block.ver` 5 is captured.

**Failed retry redeems (v0.7.8, Arbitrum One).** ArbOS settles a retryable
redeem, an `ArbitrumRetryTx` (type 104), outside its EVM execution, and the
chain keeps the settlement when the redeem fails. Before the gas purchase it
moves the ticket's call value `v` from the ticket's escrow to the sender and
mints the prepaid gas, `g * b` (gas limit times base fee), to the sender. After
the execution and the sender's `GAS_REFUND` of the gas left, `l * b`, it burns
that refund again, pays the same amount from its fee accounts to the ticket's
refund address (or to the sender past the ticket's maximum refund) and returns
`v` to the escrow. Arbitrum One's producer of `Block.ver` 3 records each move
as a `TRANSFER` in the root call. Native persists these records when, no-op
records left out and in ordinal order, they are exactly that settlement: the
root call's only `GAS_BUY` is the sender's debit of `g * b`, and the refund
pairs add up to `l * b`, pay one refund address at most and pay it nothing
after the sender, as Nitro's `EndTxHook` orders them. A failed retry with
balance records in any other shape, a value past `uint256` or more gas used
than its limit among them, fails the block: without its settlement every
account can still chain with wrong balances. In ArbOS 60, and from ArbOS 61
only under the `MultiGasConstraints` gas model, a failed retry can also get a
multi-dimensional gas refund, one or two more pairs from the network fee
account whose amount the trace does not carry; such a block fails until a
fixture pins it (the six failed retries found under ArbOS 61, up to block
511,131,266, carry none). Of the 573 failed retries that an RPC search of the
Nitro era found (the attempts of tickets that a later manual redeem names), a
mirror of the first v0.7.8 build's rules refuses the blocks of 562 and gives
wrong balances without a refusal in 8 (81,786,421, 183,682,596, 211,802,394,
216,818,967, 227,710,115, 246,963,404, 314,259,052 and 323,064,292); live
checks confirmed the refusals of 52,067,707, 97,021,351 and 127,044,374 and the
wrong rows of 81,786,421, 183,682,596 and 314,259,052, where the sender
`0x4996f81f…` kept 6,035,201,444,132,000 wei and the chain reads zero. The
fixtures are [52,067,707](tests/fixtures/arbitrum-52067707-failed-retry.json)
(the reverted auto-redeem `0x533a62f8…`),
[314,259,052](tests/fixtures/arbitrum-314259052-failed-retry-value.json)
(`0xe0359a51…`, a manual redeem with call value) and
[511,131,266](tests/fixtures/arbitrum-511131266-failed-retry-out-of-gas.json)
(`0x52221587…`, out of gas, with call value, under ArbOS 61).

**Failed retryable submissions and other ArbOS transactions (v0.7.8, Arbitrum
One).** ArbOS applies three transaction types in Nitro's `StartTxHook`,
without an EVM execution or a snapshot, so the chain keeps their balance moves
when the hook fails the transaction: an `ArbitrumDepositTx` (type 100), an
`ArbitrumSubmitRetryableTx` (105) and an `ArbitrumInternalTx` (106). Native
persists the moves of a failed submission in two exact shapes, in a `FAILED`
submission without gas used whose only call is its root and whose moving
records are `TRANSFER`s after the transaction's begin ordinal and before the
root call's end: the mint of its deposit to the sender alone (it fails below
its maximum submission fee), or that mint followed by the submission fee paid
to another account and paid back (it cannot pay its call value). A failed
transaction of type 100, 105 or 106 with any other record that moves a
balance, in any call, other than a gas record of its root call, fails the
block. A failed `ArbitrumUnsignedTx` (101) or `ArbitrumContractTx` (102) runs
in the EVM like any other transaction: its value reverts and only its fee
persists. Of the submissions Arbitrum One's L1 inbox delivered from Nitro's
start to Ethereum block 26,194,531, 16 failed, the last in block 368,841,870:
seven without a balance record and nine in the two shapes, in blocks
71,044,381, 71,049,015, 333,543,134, 336,291,645, 336,577,664, 336,580,718,
336,582,216, 336,586,744 and 336,722,229. Before this rule native dropped
those nine's moves without refusing the block, so their accounts had no row
there. The fixtures are
[71,044,381](tests/fixtures/arbitrum-71044381-failed-submit-retryable-mint.json)
(`0x6afc76c4…`, the mint alone),
[333,543,134](tests/fixtures/arbitrum-333543134-failed-submit-retryable-call-value.json)
(`0x8113baed…`, the fee paid back) and
[22,207,957](tests/fixtures/arbitrum-22207957-failed-submit-retryable.json) (a
failed submission without a record), and three succeeded transactions in the
later layout, which holds the moves in a depth-1 call, that the tests mark as
failed: a [deposit](tests/fixtures/arbitrum-514523401-deposit.json), a
[submission](tests/fixtures/arbitrum-514497707-submit-retryable.json) and a
[batch posting report](tests/fixtures/arbitrum-514709255-batch-posting-report.json).

**SetCode boundary on Arbitrum One (v0.7.8; shared rules, ERC-20 v0.7.3).** A
failed SetCode (EIP-7702) transaction keeps its authorizations' nonce and code
records, which the shared rules tell from the reverted execution by the root
call's `begin_ordinal`. Arbitrum One's producer of `Block.ver` 3 records every
root call's `begin_ordinal` as 0, although the root call's begin still takes
an ordinal that no record carries. The shared rules
([`common/persist`](../../common/persist) and its copy in `erc20/balances`)
then recover the boundary from the exact shape of the authorization records:
after the sender's nonce record, for each non-discarded authorization with an
authority, in list order and at consecutive ordinals, the authority's creation
when it is a new account (nonce 0), its nonce record from the authorization's
nonce to the next and its code record installing the delegation
`0xef0100 || address`. The boundary is the next ordinal, which no record or
call boundary of the transaction may take, before the root call's end; any
other shape still fails the block. In five Arbitrum One windows from block
350,000,000 to 514,813,000 (222,000 blocks), all 8,778 SetCode transactions,
succeeded or reverted, have this shape. The first v0.7.8 build and ERC-20
v0.7.2 refused Arbitrum One blocks 350,048,039, 400,015,655, 400,016,543,
450,044,117, 500,000,077 and 514,791,024 (native's pre-release noop failed on
it in the 350M, 400M, 500M and head windows, ERC-20's also in the 450M
window, where native had stopped earlier, at the repeated withdraw of block
450,014,091). The captured transactions are the `arbitrum-*-setcode.pb`
fixtures of
[`erc20/balances/tests/fixtures/`](../../erc20/balances/tests/fixtures/), with
their provenance in
[`arbitrum-setcode.json`](../../erc20/balances/tests/fixtures/arbitrum-setcode.json);
native's tests check the payers' and fee accounts' rows of blocks 350,048,039
and 514,791,024.

**Unapplicable SetCode self-authorization (v0.7.9, native only).** Geth and
Nitro increment the sender's nonce from the transaction's nonce `n` before
they apply the authorization list, and an authorization applies only with its
authority's current nonce, so the sender's own authorization with nonce `n`
never applies and leaves no record. The Firehose tracer marks an
authorization discarded only when the root call has no unused nonce change of
its authority from its nonce to the next, and the sender's own `n -> n + 1`
is one, so it leaves that authorization unmarked; on Arbitrum One's
`Block.ver` 3 the shared rules then find no records for it and refuse the
block. Native v0.7.8 refused Arbitrum One blocks 358,023,229, 360,021,045,
382,234,350, 420,239,048 and 472,773,590 in its full-history noops
(2026-10-17 and 18), and ERC-20 v0.7.3 refuses them too (checked offline on
the saved blocks). In each, the ordinal after the sender's nonce record, or,
in 360,021,045, after the records of the sender's second authorization with
nonce `n + 1`, is the only one of the transaction that no record or call
boundary takes. Since v0.7.9 native, only when the shared rules refuse the
block as it is, marks such an authorization discarded and takes the unchanged
shared rules' records of the corrected block. The shape is exact: a failed
SetCode transaction whose root call records `begin_ordinal` 0, exactly one
unmarked authorization of the sender with the transaction's nonce, exactly
one nonce record of the sender from that nonce to the next at a positive
ordinal, and then either the shared walk accepts the sender's other
authorizations, or none is left and the ordinal after the sender's nonce
record is free, before the root call's end, with no other nonce or code
record of the sender before it; where only other authorities'
authorizations would be left, the block still fails. `common/persist` and
ERC-20's copy are unchanged, so ERC-20 v0.7.3 still refuses these blocks.
Native keeps no nonce or code record, and a failed transaction's balance
records do not depend on the boundary: the senders pay their gas and the fee
accounts are credited after the execution, as v0.7.8's fee rules give
(`0x207b7745…` ends block 472,773,590 at the RPC-era row's
9,916,520,340,708,000 wei, and `0x0128c9c9…` block 358,023,229 at
1,298,193,932,352,314). The fixtures are the five unedited
`arbitrum-*-failed-setcode.pb` transactions of
[`tests/fixtures/`](tests/fixtures/), with their provenance in
[`arbitrum-setcode-v079.json`](tests/fixtures/arbitrum-setcode-v079.json).

**Records outside a failed transaction's execution (v0.7.8, refused).** Some
failed retries and the failed submissions above gave wrong balances without a
refusal: the shared rules keep only a failed root call's gas records, the
reason guard lets the reasons that revert with the frame through, and a dropped
record that is its account's last in the block still chains. A record outside
the execution cannot belong to the reverted frame. So a record of a failed
transaction's root call that moves a balance (its values differ, or one exceeds
`uint256`) and lies after the execution, past the root call's end ordinal, or
before it, at a positive ordinal below the root call's begin ordinal, fails the
block unless it persists as recorded: as a gas record or by one of the rules
above. The producers of `Block.ver` 3 record every root call's begin ordinal as
0, so only the window after the execution applies to them; those of `Block.ver`
4 and 5 record the begin after the sender's `GAS_BUY` and a deposit's mint. A
root call without an end ordinal, in a transaction with one, cannot place its
records, so any such record of it that does not persist fails the block too; a
transaction without ordinals, as synthetic traces are, is left to the other
checks. A record during the execution, or at ordinal 0, reverts with the frame
as before. The check runs after the reduction, so a dropped record that breaks
continuity still fails as discontinuous. In samples of every supported network
and era (2026-10-15: windows of 1,000 to 27,000 blocks from each chain's early
blocks to its head, and the blocks of every captured failed-transaction shape),
every record of a failed root call outside its execution that moves a balance
persists by these rules, and every failed root call has an end ordinal. The
tests run the guard over every captured failed-transaction shape and show that
unpinned records fail the block even where they are their accounts' last in the
block (from Arbitrum One block 150,000,072, Polygon block 95,537,024 and Base
block 5,044,695).

The failed-transaction reason guard exists because a `REVERT` (19), an
`INCREASE_MINT` (18) outside a failed deposit's exact shape and a blob fee
outside a blob transaction have no fixture inside a failed transaction.
Silently dropping such a record could omit a persisted credit; failing the
block surfaces the case for qualification. See the
[persisted-effect matrix](docs/persisted-effects.md).

### Genesis (v0.7.0)

Block 0 carries the chain's genesis allocations. The BSC producer records them
as one synthetic succeeded transaction (zero hash, from and to the zero
address) whose call holds a `GENESIS_BALANCE` record from zero per funded
account, next to the system contracts' code. Block 0 is accepted when every
balance record in it is such an allocation, and the ordinary rules then emit
one row per funded account: on BSC, TokenHub `0x…1004` with 176,405,560.9 BNB
and `0x446aa6…1f3e` and `0xb00574…c632` with 500 BNB each
([`tests/fixtures/bsc-0-genesis.json`](tests/fixtures/bsc-0-genesis.json)).
Native v0.2.2 refused block 0, and in production mode a stream from block 1
still ran it, because Substreams processes whole 1,000-block segments; a sync
can now start at block 0. All three accounts change later (RPC-era database),
so their current balances do not depend on block 0. BSC block 1 has no
balance change.

## Input and networks

`map_events(block)` takes only an Extended block, with no parameters. At the
owner's direction (2026-10-06), the `producer_versions` parameter and its
version gate were removed. `Block.ver` does not control acceptance: the map
checks the block's identity, complete transaction traces, persisted effects,
execution ordinals and balance continuity instead. Version 3, 4, 5 and an
unknown version are covered by synthetic regression tests using the same
valid and ambiguous records. Since v0.7.9 two Arbitrum One rules depend on the
ArbOS version, which native reads from the block itself: Nitro's internal
transaction first and the version in the header's mix digest
([continuity exceptions](#continuity-exceptions)). It is chain semantics
read from block data, not a parameter or a network allowlist.

`network: bsc` remains a CLI endpoint default, not an admission gate. There
are no per-network parameters or network allowlist, and `db_out` takes no
params either. Select the intended network and endpoint when running it;
blocks do not carry a chain ID that this module can use to verify the
endpoint.

Since v0.7.8 native supports eight EVM networks: Ethereum, BSC, Polygon,
Arbitrum One, Base, Optimism, Unichain and HyperEVM, each spot-checked and
nooped on 2026-10-14 and 15
([matrix](docs/persisted-effects.md#chain--producer--fork-matrix)). Other
target networks, such as Arc, are not checked yet
([#8](https://github.com/pinax-network/substreams-evm-balances/issues/8)). The
Avalanche C-Chain endpoint serves no Extended blocks, so native refuses every
block there.

- **Start blocks.** A production-mode stream processes whole 1,000-block
  segments, so it also runs the blocks before its start block in that segment,
  and a block before the first Extended one fails the stream. Where a
  network's Extended blocks begin after genesis, start at the first whole
  1,000-block segment of them: Optimism at 105,236,000 (Extended from
  105,235,064, after Bedrock) and Arbitrum One at 22,208,000 (Extended from
  22,207,818, after Nitro). The 936 and 182 Extended blocks before those
  starts are not processed, and the balances the legacy chains carried into
  Bedrock and Nitro appear in no block's records: an account there has a row
  only once its balance changes
  ([initialization](../../docs/initialization-and-completeness.md#7-initialization)).
  BSC starts at block 0 ([genesis](#genesis-v070)).
- **`Block.ver` by network** (the versions seen). Ethereum 3 and 5; BSC 3, 4
  and 5; Polygon 3 to block 73,440,255 and 4 from 73,440,256; Arbitrum One 3;
  Base 4 to block 45,903,999 and 5 from 45,904,000; Optimism 4 (seen from
  block 105,712,236 to 140,002,047) and 5 (seen from 155,196,870); Unichain 4
  to at least block 50,000,000 and 5 from at most 52,500,000; HyperEVM 5. The
  endpoint may serve one block under different versions: Polygon block
  95,537,024 came as version 5 and, later the same day, as version 4 with the
  same records at other ordinals. The rules above name the producer versions
  they were captured on.

Acceptance is not qualification for another network or producer release.
Version-3 system-call ordinals can use a different scale from transaction
ordinals, and each rule above covers only the producer shapes its fixtures
capture (see the [persisted-effect matrix](docs/persisted-effects.md) and
[#125](https://github.com/pinax-network/substreams-evm-balances/issues/125)).
The existing data checks reject detectable ambiguity; they do not establish
that every chain's persistent effects are represented correctly: a shape that
chains with a wrong balance, as the missing self-destruct debit did until
v0.7.8, and on Polygon until v0.7.11, passes a noop and shows only in a
same-block RPC check or a comparison across blocks. The dated
BSC evidence below still refers to its original packages and parameters.

## Build and test

```sh
cargo test --locked -p native-balances -p evm-persist
make -C native/balances build schema   # release WASM and clickhouse/schema.sql; no network
make -C native/balances gui            # or `prod`, MODULE=db_out; streams from ENDPOINT
```

Committed spkgs come from CI's "Package hashes" artifact, not from a local
`make pack` (a macOS build has other module hashes than CI's Linux build).

`make gui` and `make prod` read `SUBSTREAMS_API_KEY` from the environment.
The committed v0.1.0 and v0.2.0 SPKGs are offline builds; the historical live-
qualified build is identified by its WASM (`48d89d28…`) and module hash
(`5a2a2e0c…`) below. `substreams pack` embeds this README, so a pack after a
README edit has a new SPKG digest but the same module. The 2026-10-05
substreams 0.8.0 build had WASM `0bc2e2cf…` and, under `bsc`, module
`868ac202…`; it was not live-qualified. The parameter-free
v0.2.0 source changes the native module and dependent `db_out` hashes again.
It has no new live qualification. v0.2.1 is the same source built with Rust
1.99.0 and reproducible paths (#118): CI's canonical Linux build has WASM
`d3b6a549…` and module `72453949…`, also not live-qualified. v0.2.2 adds the
self-transfer pair exception above; until v0.6.1 the combined `evm-balances`
package imported it and wrote its rows. v0.7.0 (package `evm_balances_native`)
adds `db_out`, `NativeEvents`, the fee pool payout and genesis; v0.7.1 accepts
the payout credit after a skipped ordinal; v0.7.2 persists a failed blob
transaction's blob fee; v0.7.3 restores the Ethereum EIP-6780 self-destruct
debit; v0.7.4 decodes the block with a 1 GiB element budget instead of
buffa's 32 MiB default, which BSC block 43,772,003 (29.9 MB encoded) failed.
v0.7.5 keeps the balance of an account that self-destructs to itself.
v0.7.6 carries a self-destruct credit from an account created in the
transaction into the beneficiary's later records in it, and v0.7.7 adds the
sum of such credits to the beneficiary's records after the execution only.
v0.7.8 covers every supported EVM network
([input and networks](#input-and-networks)): it restores a self-destruct
debit also in a call with other records (Polygon, Ethereum), drops HyperEVM's
self-transfer pair and Arbitrum One's repeated self-destruct withdraw,
rewrites Base's fee vault credit after a vault withdrawal, orders Polygon's
`Block.ver` 3 state-sync transaction by its depth-1 calls, persists what a
failed transaction keeps on Polygon (the base-fee `BURN`), Arbitrum One (the
fee transfers and the settlements of retries and retryable submissions) and
the OP stack (the deposit mint), and takes the shared rules' SetCode boundary
for Arbitrum One's `Block.ver` 3, as ERC-20 v0.7.3 does. It refuses any other
failed Arbitrum deposit, submission, retry or internal transaction that moves
a balance, any other record of a failed transaction outside its execution
that moves a balance, and a self-destruct to itself recorded as debit, refund
and repeat. Its history keeps only the 1h, 4h, 1d and 1w bars. v0.7.9 accepts
the thirteen Arbitrum One and Base blocks that v0.7.8's full-history noops
refused up to 2026-10-18, each in a captured producer shape, and changes nothing for a block
v0.7.8 accepts: on Arbitrum One it drops the repeated withdraw also across a
new beneficiary's creation, burns the balance of a self-destruct to itself
before ArbOS 20 and keeps it since ArbOS 20, and takes the sender's own
SetCode authorization with the transaction's nonce as discarded; on Base it
rewrites a fee vault's credit after a transfer into the vault. v0.7.10
accepts Arbitrum One block 500,689,520, which v0.7.9's full-history noop
refused, and changes nothing for a block v0.7.9 accepts: it drops the
repeated withdraw also of an account created in its transaction that
self-destructs to itself, which ends at zero. v0.7.11 restores the debit of a
Polygon self-destruct that the `Block.ver` 3 producer records without the
self-destruct flag, and refuses such a call in any other shape, except a
self-destruct to itself recorded as the account's own refund from zero.
Unlike v0.7.9 and v0.7.10, it changes rows of blocks the earlier releases
accept:
Polygon blocks with that shape (the 58 found are from 55,412,000 to
71,461,200), and it can refuse blocks they accept. No such call was found
in the saved blocks and fixtures of the other networks, so the refusal fails
none of those blocks. v0.7.12 also restores that debit after the account's
earlier records in its transaction, which chain into it, a near-shape on
which v0.7.11's Polygon backfill stopped in 9 blocks from 56,954,084 to
72,387,144, and changes nothing for a block v0.7.11 accepts. None is
live-qualified.

## Live qualification (BSC, 2026-09-23)

The packed map of 2026-09-23 (`spkg` sha256 `fbb46fc7…`, wasm `48d89d28…`, default
parameters `{"producer_versions":[5]}`) was streamed with
`--final-blocks-only` from `bsc.substreams.pinax.network` and checked with
the host tool `native-balances-tools live-parity`
([source at `6dade89`](https://github.com/pinax-network/substreams-evm-balances/tree/6dade8957887c0c278cfa8da6bef61b9cc22f534/native/balances/tools); removed since). It binds
each block number to its canonical hash and batches
`eth_getBalance(address, {blockHash})`:

- **Saved controls** ([report](docs/evidence/live-parity-bsc-2026-09-23-saved-controls.json)):
  the 1,024 contiguous saved blocks 122,288,006–122,289,029. The packaged
  rows equal the offline Rust replay of the saved `.pb` files row for row
  (76,139 rows, 17,670 accounts, 6,366 known zeros), and every row equals
  `eth_getBalance` at the block hash (76,139/76,139).
- **Live window** ([report](docs/evidence/live-parity-bsc-2026-09-23-live.json)):
  the 5,000 contiguous final blocks 123,548,070–123,553,069, streamed without
  a refused block: 670,873 rows for 102,913 accounts (45,162 known zeros).
  Every row of the last 1,000 blocks and every tenth row of the others was
  checked: 200,344/200,344 equal. Block 123,550,024 has no transaction and
  no output; the miner, fee and system accounts are unchanged across it.
- **Reasons** ([report](docs/evidence/live-reasons-bsc-2026-09-23.json),
  [matrix](docs/persisted-effects.md)): the last 250 blocks fetched from
  Firehose and replayed offline equal the packaged output and carry
  transfers, gas, fee rewards, 37 blob-fee rewards and 500 block-level fee
  resets.

```sh
substreams run -e bsc.substreams.pinax.network:443 <spkg> map_events -s <start> -t <stop> \
  --final-blocks-only -o jsonl --bytes-encoding hex > events.jsonl
```

**Retention from a checkpoint** ([report](docs/evidence/live-retention-checkpoint-bsc-2026-09-23.json)):
the host tool `native-balances-tools retention-check` (removed since) seeded
`eth_getBalance` for 14,172 accounts at block 123,548,069 into the
`common/retention` ledger, applied the
next 300 packaged blocks and compared every retained value with RPC at four
block hashes: 56,688/56,688 equal, including 1,252 accounts that were never
emitted in the window and kept their checkpoint value.

Parity covers the emitted accounts only: an account without a persisted
change is not emitted and not checked, and an RPC candidate list (such as the
current RPC module's) would also contain unchanged accounts, which are not
state-derived updates. Other networks, producer versions and the Ethereum,
OP and Arc rows of the matrix stay under
[#8](https://github.com/pinax-network/substreams-evm-balances/issues/8).

## Offline evidence

Rust regressions (`src/tests.rs`) cover the complete captured BSC block
122260950 against all **82 saved native `eth_getBalance` rows**, the historical
prototype's 82 old/new/ordinal rows, the BSC fee-reset order (transaction
ordinal 3237 credit, block ordinal 3244 reset to zero), the two captured
failed SetCode transactions, net-zero accounts, absent value messages, zero and
burn-looking addresses, `uint256` max, tied/zero ordinals, discontinuity,
reverted frames, failed transactions, reverted system calls, the failed-
transaction reason guard, parameter-free acceptance across block versions,
version-independent ambiguity rejection, the `NativeEvents` wire encoding, the
captured fee pool payouts of blocks 363, 12,700,000 and 25,401,003 with the
exact shape they need, and BSC's captured genesis block 0. Every producer
shape of the [rules](#persistence-and-fail-closed-rules) has its captured
transactions under [`tests/fixtures/`](tests/fixtures/) (Ethereum, BSC,
Polygon, Arbitrum One, Base, Optimism, Unichain and HyperEVM), with exact
expected rows and a test that mutates its shape; each rule's tests fail
with the rule disabled. The self-destruct to itself recorded as debit,
refund and repeat is captured (block 326,011,151) and also built from
another capture, and the guard on records outside a failed transaction's
execution runs over every captured failed-transaction shape. Every v0.7.9
test of a new rule fails against the v0.7.8 rules, every v0.7.10 test of
the new shape against the v0.7.9 rules, every v0.7.11 test of the
unflagged self-destruct against the v0.7.10 rules, and every v0.7.12 test
of earlier records chaining into its debit against the v0.7.11 rules; every
test of its refusal fails with the refusal disabled. `tests/db_out.rs` covers the
ClickHouse rows, the manifest wiring and the embedded schema.

The host replay tool (`native/balances/tools`, [source at `6dade89`](https://github.com/pinax-network/substreams-evm-balances/tree/6dade8957887c0c278cfa8da6bef61b9cc22f534/native/balances/tools);
removed since) reduced directories of captured
`<height>.pb` blocks, linked consecutive block hashes, checked that each
account's `old_amount` equals its previously emitted amount whenever every
intervening block was replayed, and compared against saved oracles. Its
reports over the locally retained BSC captures are in
[`docs/evidence/`](docs/evidence/) and summarized in the
[persisted-effect matrix](docs/persisted-effects.md): 1,439 version-5 blocks,
126,180 rows and 86,564 cross-block continuity checks with zero mismatches,
plus the 82-row same-block RPC oracle. Those captures are the
ERC-20 campaign's block cache, not a native-specific RPC audit: only block
122260950 has a same-block native RPC oracle. The historical 192-block,
37,086-check native audit ([legacy qualification](https://github.com/pinax-network/substreams-evm-balances/blob/9b41c7f/erc20/balances/docs/legacy-qualification.md))
applies to the removed prototype package, not to this one.

## ClickHouse

`db_out` writes `native_balances`, `blocks` and the native history of the
substreams-evm schema into this package's database; the sink, the Token API
live sink, the cutover and the examples in
[`clickhouse/examples/`](clickhouse/examples/) are in
[`docs/clickhouse.md`](../../docs/clickhouse.md). Since v0.7.8 the history
keeps only the 1h, 4h, 1d and 1w bars the Token API serves (owner,
2026-10-14), and the replay check and repair work in 1-hour windows. Start a
sink at block 0 on BSC and at the first whole 1,000-block segment of Extended
blocks on Optimism (105,236,000) and Arbitrum One (22,208,000)
([start blocks](#input-and-networks)). The earlier CLI native-sink
path (bridge view, `Balance` TTL, checks of 2026-10-03) is at
[`cb62110`](https://github.com/pinax-network/substreams-evm-balances/tree/cb6211007f1f9d0cb5852a6b666da475a8fa6542/native/balances/clickhouse).

## Boundaries

No RPC, no candidate discovery, no transfer events, no supply or burn
interpretation, no wallet labels, no ERC-20 tables, no custom sink, no global
holder enumeration. Initialization of untouched accounts, exact checkpoints, reorg
completeness and the complete-block clock contract are shared under
[#7](https://github.com/pinax-network/substreams-evm-balances/issues/7).
Other networks' semantics are separate qualification work under
[#8](https://github.com/pinax-network/substreams-evm-balances/issues/8).
This package does not change `erc20/balances`.
