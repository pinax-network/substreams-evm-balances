//! One RPC-free `map_events`: the final persisted native account balance of
//! every account changed in an Extended block. `db_out` writes them into the
//! substreams-evm ClickHouse tables (`db_out` module).
//!
//! Output is `evm.balances.v1.NativeEvents`: `Events` without the contract,
//! which native amounts never have, with the same field numbers, so the bytes
//! also decode as `Events` with no contract. `NativeBalance.amount` is the
//! account's balance after the last persisted change in the block, as an exact
//! decimal `uint256` string. Accounts without a persisted change in the block
//! are not emitted; absence is no observation, never zero. Block 0 emits the
//! genesis allocations ([`validate_block`]).
//!
//! Balances are account state, reduced from the producer's persisted
//! `BalanceChange` records in execution order. They are not a sum of inferred
//! transfers: value-bearing calls and transfer logs are not summed. A call's
//! value is read only to recognize the producer's self-transfer pair (see
//! [`changes`]). Which records persist is decided by the shared
//! [`evm_persist`] rules (failed-transaction gas effects, reverted frames,
//! system calls, block-level records), plus the blob fee of a failed blob
//! transaction ([`records`]).
use evm_persist as persist;
pub mod db_out;

use proto::pb::evm::balances::v1 as balances_pb;
use std::collections::{BTreeMap, HashMap, HashSet};
use substreams::{errors::Error, scalar::BigInt};
use substreams_ethereum::pb::eth::v2 as eth;

fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error::msg(message.to_string()))
    }
}

/// One account's persisted native balance movement within a block: the value
/// before its first persisted change and after its last one. Intermediate
/// values are reduced away; a net-zero account that changed is still reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub address: Vec<u8>,
    /// Balance before the first persisted change in this block.
    pub old_amount: String,
    /// Balance after the last persisted change in this block.
    pub amount: String,
    pub first_ordinal: u64,
    pub ordinal: u64,
    /// Number of persisted change records reduced into this row.
    pub records: u32,
}

/// A persisted native balance record with its persistence provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub address: Vec<u8>,
    pub old_value: Option<Vec<u8>>,
    pub new_value: Option<Vec<u8>>,
    pub ordinal: u64,
    pub reason: i32,
    pub scope: persist::Scope,
}

#[derive(Default)]
struct Collected {
    records: Vec<Record>,
}
impl persist::Sink for Collected {
    fn storage(&mut self, _: &eth::StorageChange, _: persist::Ctx) {}
    fn balance(&mut self, c: &eth::BalanceChange, ctx: persist::Ctx) {
        self.records.push(Record {
            address: c.address.clone(),
            old_value: c.old_value.as_option().map(|v| v.bytes.clone()),
            new_value: c.new_value.as_option().map(|v| v.bytes.clone()),
            ordinal: c.ordinal,
            reason: c.reason.to_i32(),
            scope: ctx.scope,
        });
    }
    fn nonce(&mut self, _: &eth::NonceChange, _: persist::Ctx) {}
    fn code(&mut self, _: &eth::CodeChange, _: persist::Ctx) {}
}

fn word(bytes: &[u8]) -> Result<[u8; 32], Error> {
    require(bytes.len() <= 32, "native balance exceeds uint256")?;
    let mut out = [0; 32];
    out[32 - bytes.len()..].copy_from_slice(bytes);
    Ok(out)
}
/// An absent `BigInt` message inside an observed `BalanceChange` encodes zero
/// under the pinned Ethereum protobuf contract. This applies only to a record
/// the producer emitted; it never initializes an untouched account.
fn value(bytes: Option<&[u8]>) -> Result<[u8; 32], Error> {
    word(bytes.unwrap_or(&[]))
}
fn amount(word: &[u8; 32]) -> String {
    BigInt::from_unsigned_bytes_be(word).to_string()
}

pub fn validate_block(block: &eth::Block) -> Result<(), Error> {
    require(block.detail_level == eth::block::DetailLevel::DetaillevelExtended, "Extended blocks required")?;
    let header = block.header.as_option().ok_or_else(|| Error::msg("missing header"))?;
    require(
        block.hash.len() == 32 && header.parent_hash.len() == 32 && header.state_root.len() == 32,
        "invalid block identity",
    )?;
    require(header.number == block.number, "header number mismatch")?;
    if block.number == 0 {
        validate_genesis(block)?;
    }
    for tx in &block.transaction_traces {
        require(
            (1..=3).contains(&tx.status.to_i32()) && !tx.calls.is_empty(),
            "incomplete transaction persistence data",
        )?;
    }
    Ok(())
}

/// Block 0 carries the chain's genesis allocations. The BSC producer records
/// them as one synthetic succeeded transaction (zero hash, from and to the
/// zero address) whose call holds a `GENESIS_BALANCE` record from zero per
/// funded account, next to the system contracts' code: TokenHub `0x…1004` and
/// two accounts on BSC. Every balance record of block 0 must be such an
/// allocation; the shared persistence rules then emit one row per funded
/// account, like any other block.
fn validate_genesis(block: &eth::Block) -> Result<(), Error> {
    let changes = block
        .balance_changes
        .iter()
        .chain(block.system_calls.iter().flat_map(|c| &c.balance_changes))
        .chain(block.transaction_traces.iter().flat_map(|t| &t.calls).flat_map(|c| &c.balance_changes));
    for c in changes {
        let from_zero = matches!(value(c.old_value.as_option().map(|v| v.bytes.as_slice())), Ok(w) if w == [0; 32]);
        require(
            c.reason.as_known() == Some(eth::balance_change::Reason::GenesisBalance) && from_zero,
            "genesis block carries a balance change that is not a genesis allocation",
        )?;
    }
    Ok(())
}

/// Balance-change reasons whose behavior inside a failed or reverted
/// transaction is pinned: the three gas reasons persist (shared rules), as
/// does a blob transaction's blob fee ([`failed_blob_fees`]), and the
/// reasons listed here revert with the execution frame. Any other reason in a failed
/// transaction's root call (for example OP-stack deposit mints, a blob fee
/// outside a blob transaction, or a reason this crate cannot name) has no
/// fixture, so the block fails instead of silently dropping a possible credit.
const REVERTIBLE_FAILED_REASONS: [eth::balance_change::Reason; 6] = [
    eth::balance_change::Reason::Transfer,
    eth::balance_change::Reason::TouchAccount,
    eth::balance_change::Reason::SuicideRefund,
    eth::balance_change::Reason::CallBalanceOverride,
    eth::balance_change::Reason::SuicideWithdraw,
    eth::balance_change::Reason::Burn,
];
fn validate_failed_transaction_reasons(block: &eth::Block) -> Result<(), Error> {
    for tx in block.transaction_traces.iter().filter(|tx| persist::is_failed(tx)) {
        let Some(root) = tx.calls.first() else { continue };
        for change in &root.balance_changes {
            let pinned = persist::is_gas_reason(change)
                || is_blob_fee_of(tx, change)
                || change.reason.as_known().is_some_and(|r| REVERTIBLE_FAILED_REASONS.contains(&r));
            require(
                pinned,
                "failed transaction carries a balance-change reason without pinned persistence semantics",
            )?;
        }
    }
    Ok(())
}

/// Every persisted native balance record in the block, with its persistence
/// scope, retained for package tests and offline inspection: the shared
/// rules' records in array order, then the blob fees of failed blob
/// transactions, which BNB Chain pays whatever the outcome.
pub fn records(block: &eth::Block) -> Result<Vec<Record>, Error> {
    validate_block(block)?;
    validate_failed_transaction_reasons(block)?;
    let mut collected = Collected::default();
    persist::collect_block(block, &mut collected)?;
    collected.records.extend(failed_blob_fees(block));
    Ok(collected.records)
}

/// Whether `change` is the blob fee of blob transaction `tx`.
fn is_blob_fee_of(tx: &eth::TransactionTrace, change: &eth::BalanceChange) -> bool {
    tx.r#type == eth::transaction_trace::Type::TrxTypeBlob && change.reason.as_known() == Some(eth::balance_change::Reason::RewardBlobFee)
}

/// BNB Chain credits a blob transaction's blob fee to the fee pool after
/// execution, with a `REWARD_BLOB_FEE` record, as it does the transaction fee,
/// so a failed blob transaction pays it too. In block 42,357,369 the failed
/// blob transaction `0xbe965c35…` credits the pool 131,072 wei after its
/// transaction fee, and the pool's next record, the block-level payout to the
/// coinbase, starts from the credited balance. The shared rules persist only
/// the three gas reasons of a failed transaction's root call. Returns the
/// blob-fee records of failed blob transactions' root calls, without no-ops;
/// like every record, they must chain.
fn failed_blob_fees(block: &eth::Block) -> Vec<Record> {
    let mut fees = Vec::new();
    for tx in block.transaction_traces.iter().filter(|tx| persist::is_failed(tx)) {
        let Some(root) = tx.calls.first() else { continue };
        for c in root.balance_changes.iter().filter(|c| is_blob_fee_of(tx, c)) {
            let (old, new) = (
                c.old_value.as_option().map(|v| v.bytes.clone()),
                c.new_value.as_option().map(|v| v.bytes.clone()),
            );
            if matches!((value(old.as_deref()), value(new.as_deref())), (Ok(a), Ok(b)) if a == b) {
                continue;
            }
            fees.push(Record {
                address: c.address.clone(),
                old_value: old,
                new_value: new,
                ordinal: c.ordinal,
                reason: c.reason.to_i32(),
                scope: persist::Scope::TxFailedPersistent,
            });
        }
    }
    fees
}

/// A value transfer from an account to itself, a CALLCODE with value or a CALL
/// whose caller is its address, does not move the balance; geth records
/// nothing for it. The BSC producer records it as two `TRANSFER` records of
/// that account, at consecutive ordinals of the call, both computed from the
/// same prior balance `b`: `b -> b - v` and `b -> b + v`, where `v` is the
/// call's value (block 127,411,449, transaction `0x5c664637…`). The second
/// record's old value breaks continuity although the balance never moved.
/// Returns those pairs, as `(account, ordinal)`, from non-reverted calls of
/// succeeded transactions. Only this exact shape is recognized.
fn self_transfer_pairs(block: &eth::Block) -> HashSet<(Vec<u8>, u64)> {
    let mut pairs = HashSet::new();
    let transfer = |c: &&eth::BalanceChange| c.reason.as_known() == Some(eth::balance_change::Reason::Transfer);
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        for call in trx.calls.iter().filter(|c| !c.state_reverted) {
            let to_itself = match call.call_type.as_known() {
                Some(eth::CallType::CALLCODE) => true,
                Some(eth::CallType::CALL) => call.caller == call.address,
                _ => false,
            };
            let Ok(v) = value(call.value.as_option().map(|v| v.bytes.as_slice())) else {
                continue;
            };
            if !to_itself || v == [0; 32] {
                continue;
            }
            let v = BigInt::from_unsigned_bytes_be(&v);
            let mut own: Vec<&eth::BalanceChange> = call.balance_changes.iter().filter(|c| c.address == call.caller).filter(transfer).collect();
            own.sort_by_key(|c| c.ordinal);
            for pair in own.windows(2) {
                let (first, second) = (pair[0], pair[1]);
                if first.ordinal == 0 || second.ordinal != first.ordinal + 1 {
                    continue;
                }
                let words = (
                    value(first.old_value.as_option().map(|v| v.bytes.as_slice())),
                    value(first.new_value.as_option().map(|v| v.bytes.as_slice())),
                    value(second.old_value.as_option().map(|v| v.bytes.as_slice())),
                    value(second.new_value.as_option().map(|v| v.bytes.as_slice())),
                );
                let (Ok(old), Ok(first_new), Ok(second_old), Ok(second_new)) = words else {
                    continue;
                };
                if old != second_old {
                    continue;
                }
                let [b, n1, n2] = [old, first_new, second_new].map(|w| BigInt::from_unsigned_bytes_be(&w));
                let (debit, credit) = (b.clone() - v.clone(), b + v.clone());
                if (n1 == debit && n2 == credit) || (n1 == credit && n2 == debit) {
                    pairs.insert((call.caller.clone(), first.ordinal));
                    pairs.insert((call.caller.clone(), second.ordinal));
                }
            }
        }
    }
    pairs
}

/// BSC's fee pool, `consensus.SystemAddress`: transaction fees collect there
/// until the consensus engine pays them out at the end of the block.
const BSC_FEE_POOL: [u8; 20] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe,
];

/// BSC pays 1/16 of the fee pool (`pool >> 4`, `systemRewardPercent`) to its
/// system reward contract before paying the rest to the validator. Each payout
/// moves the amount from the pool to the block's coinbase, which then sends it
/// on in a system transaction. The BSC producer of `Block.ver` 3 records the
/// first move's pool debit with its old value equal to its new one: the client
/// lowered the pool's balance in place before setting it, so the tracer read
/// the lowered balance as the old value (blocks 363, 958, 4104, 12,700,000
/// and 25,401,003). The block-level `REWARD_TRANSACTION_FEE` record of the
/// pool reads `n -> n`, and the next block-level record credits the coinbase
/// with `r`, where `r = (n + r) >> 4`, before any transaction starts. The
/// credit is one or two ordinals later: the producer can skip an ordinal
/// between them. The shared rules drop the `n -> n` record as a no-op, and
/// the pool's next record starts from `n`, so continuity breaks although the
/// balance moved exactly as recorded for the coinbase.
///
/// Returns the records these stand for: the pool from `n + r` to `n` at the
/// `n -> n` record's ordinal. The continuity check then verifies `n + r`
/// against the pool's previous balance. Only this exact shape is recognized.
fn fee_pool_payouts(block: &eth::Block) -> Vec<Record> {
    let fee = |c: &eth::BalanceChange| c.reason.as_known() == Some(eth::balance_change::Reason::RewardTransactionFee);
    let Some(header) = block.header.as_option() else {
        return Vec::new();
    };
    let mut payouts = Vec::new();
    for pool in block.balance_changes.iter().filter(|c| c.address == BSC_FEE_POOL && fee(c) && c.ordinal > 0) {
        // The next block-level record, with no transaction starting between.
        let Some(credit) = block.balance_changes.iter().filter(|c| c.ordinal > pool.ordinal).min_by_key(|c| c.ordinal) else {
            continue;
        };
        let interrupted = block
            .transaction_traces
            .iter()
            .any(|t| t.begin_ordinal > pool.ordinal && t.begin_ordinal < credit.ordinal);
        if credit.address != header.coinbase || !fee(credit) || interrupted {
            continue;
        }
        let words = (
            value(pool.old_value.as_option().map(|v| v.bytes.as_slice())),
            value(pool.new_value.as_option().map(|v| v.bytes.as_slice())),
            value(credit.old_value.as_option().map(|v| v.bytes.as_slice())),
            value(credit.new_value.as_option().map(|v| v.bytes.as_slice())),
        );
        let (Ok(pool_old), Ok(pool_new), Ok(credit_old), Ok(credit_new)) = words else {
            continue;
        };
        let [n, credit_old, credit_new] = [pool_new, credit_old, credit_new].map(|w| BigInt::from_unsigned_bytes_be(&w));
        if pool_old != pool_new || credit_new <= credit_old {
            continue;
        }
        let reward = credit_new - credit_old;
        let before = n + reward.clone();
        let Ok(before_word) = word(&before.to_bytes_be().1) else {
            continue;
        };
        if before >> 4u32 != reward {
            continue;
        }
        payouts.push(Record {
            address: pool.address.clone(),
            old_value: Some(before_word.to_vec()),
            new_value: Some(pool_new.to_vec()),
            ordinal: pool.ordinal,
            reason: pool.reason.to_i32(),
            scope: persist::Scope::Block,
        });
    }
    payouts
}

/// A self-destruct moves the destroyed account's whole balance to its
/// beneficiary. Since Cancun (EIP-6780) the Ethereum producer of `Block.ver` 3
/// records only the beneficiary's `SUICIDE_REFUND` credit `d` when the account
/// was not created in the same transaction: the account's debit to zero is
/// missing, and its next record starts from zero (block 20,249,174,
/// transaction `0x16bb56d4…`, a DELEGATECALL self-destruct of
/// `0x0000000000bb3d7b…`). The producer of `Block.ver` 5 records that debit as
/// a `SUICIDE_WITHDRAW` before the credit (block 25,741,008, transaction
/// `0xeb4fc94a…`), which this shape does not match. Returns the debits
/// these stand for: the destroyed account from `d` to zero at the credit's
/// ordinal. The continuity check then verifies `d` against the account's
/// previous balance. Only this exact shape is recognized: a persisted call
/// (non-reverted, in a succeeded transaction) that self-destructs, holds
/// exactly one balance record, a positive `SUICIDE_REFUND` credit of another
/// account, and no record of the destroyed account. The destroyed account is
/// the call's execution context: its caller for DELEGATECALL and CALLCODE, its
/// address otherwise.
fn selfdestruct_debits(block: &eth::Block) -> Vec<Record> {
    let mut debits = Vec::new();
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        for call in trx.calls.iter().filter(|c| c.suicide && !c.state_reverted) {
            let destroyed = match call.call_type.as_known() {
                Some(eth::CallType::DELEGATE | eth::CallType::CALLCODE) => &call.caller,
                _ => &call.address,
            };
            let [credit] = call.balance_changes.as_slice() else {
                continue;
            };
            if credit.reason.as_known() != Some(eth::balance_change::Reason::SuicideRefund) || credit.address == *destroyed {
                continue;
            }
            let words = (
                value(credit.old_value.as_option().map(|v| v.bytes.as_slice())),
                value(credit.new_value.as_option().map(|v| v.bytes.as_slice())),
            );
            let (Ok(old), Ok(new)) = words else {
                continue;
            };
            let [old, new] = [old, new].map(|w| BigInt::from_unsigned_bytes_be(&w));
            if new <= old {
                continue;
            }
            let Ok(moved) = word(&(new - old).to_bytes_be().1) else {
                continue;
            };
            debits.push(Record {
                address: destroyed.clone(),
                old_value: Some(moved.to_vec()),
                new_value: Some([0; 32].to_vec()),
                ordinal: credit.ordinal,
                reason: eth::balance_change::Reason::SuicideWithdraw as i32,
                scope: persist::Scope::Tx,
            });
        }
    }
    debits
}

/// Since Cancun (EIP-6780) a self-destruct to itself of an account not created
/// in the same transaction moves nothing: geth debits the balance `d` and
/// credits it back, and the account survives. The Ethereum producer of
/// `Block.ver` 3 records the credit as a `SUICIDE_REFUND` from zero to `d`,
/// without the debit, and then a `SUICIDE_WITHDRAW` from `d` to zero at the
/// next ordinal, although the account keeps `d` (block 19,481,732,
/// transaction `0x833a78e6…`: `0x6550f9a4…` keeps 1 wei). The refund's zero
/// old value breaks continuity, and the withdraw would zero the account.
/// Returns that pair, as
/// `(account, ordinal)`. Only this exact shape is recognized: a persisted call
/// (non-reverted, in a succeeded transaction) that self-destructs and holds
/// exactly those two records of the destroyed account, `d` positive, in a
/// transaction with no CREATE of that account. The destroyed account is
/// resolved as in `selfdestruct_debits`.
fn selfdestruct_to_self_pairs(block: &eth::Block) -> HashSet<(Vec<u8>, u64)> {
    let mut pairs = HashSet::new();
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        for call in trx.calls.iter().filter(|c| c.suicide && !c.state_reverted) {
            let destroyed = match call.call_type.as_known() {
                Some(eth::CallType::DELEGATE | eth::CallType::CALLCODE) => &call.caller,
                _ => &call.address,
            };
            let [refund, withdraw] = call.balance_changes.as_slice() else {
                continue;
            };
            let created = trx
                .calls
                .iter()
                .any(|c| c.call_type.as_known() == Some(eth::CallType::CREATE) && c.address == *destroyed);
            if created
                || refund.address != *destroyed
                || withdraw.address != *destroyed
                || refund.reason.as_known() != Some(eth::balance_change::Reason::SuicideRefund)
                || withdraw.reason.as_known() != Some(eth::balance_change::Reason::SuicideWithdraw)
                || refund.ordinal == 0
                || withdraw.ordinal != refund.ordinal + 1
            {
                continue;
            }
            let words = (
                value(refund.old_value.as_option().map(|v| v.bytes.as_slice())),
                value(refund.new_value.as_option().map(|v| v.bytes.as_slice())),
                value(withdraw.old_value.as_option().map(|v| v.bytes.as_slice())),
                value(withdraw.new_value.as_option().map(|v| v.bytes.as_slice())),
            );
            let (Ok(refund_old), Ok(d), Ok(withdraw_old), Ok(withdraw_new)) = words else {
                continue;
            };
            if refund_old != [0; 32] || d == [0; 32] || withdraw_old != d || withdraw_new != [0; 32] {
                continue;
            }
            pairs.insert((destroyed.clone(), refund.ordinal));
            pairs.insert((destroyed.clone(), withdraw.ordinal));
        }
    }
    pairs
}

/// Since Cancun (EIP-6780) a self-destruct destroys an account created in the
/// same transaction, and its balance `d` moves to the beneficiary. The
/// Ethereum producer of `Block.ver` 5 records the account's `SUICIDE_WITHDRAW`
/// from `d` to zero and, at the next ordinal, the beneficiary's
/// `SUICIDE_REFUND` from `b` to `b + d`, but the beneficiary's later records in
/// that transaction start from `b`, without the credit. The balance after the
/// transaction includes it (block 25,741,008, transaction `0xd6299894…`: a
/// CREATE at an address holding 0.159 ETH self-destructs in its constructor
/// to the sender `0xc6acbee4…`, whose gas refund starts from the balance
/// before the credit; an explorer reads the credit in the sender's balance
/// after the block). Returns the shift `d` of each such later record, by
/// `(account, ordinal)`: every persisted record of the beneficiary after the
/// credit and before the transaction ends, when the first of them starts from
/// `b` and each of them moves the balance by an amount ([`SHIFTABLE`]). A
/// later record that sets the balance, such as the beneficiary's own
/// self-destruct withdraw to zero, would keep `d` once shifted, so the block
/// fails instead. The continuity check then verifies the shifted records.
/// Without the shift that first record breaks continuity with the persisted
/// credit, so blocks that native v0.7.5 accepts are unchanged. Only this exact
/// shape is recognized: a persisted call (non-reverted, in a succeeded
/// transaction) that self-destructs an account created by a CREATE in the
/// same transaction and holds exactly those two records, `d` positive, the
/// refund persisted as recorded. The destroyed account is resolved as in
/// `selfdestruct_debits`. Several such credits to one beneficiary in a
/// transaction add up.
/// The reasons of records that move a balance by an amount, which
/// `selfdestruct_credit_shifts` may shift.
const SHIFTABLE: [eth::balance_change::Reason; 4] = [
    eth::balance_change::Reason::Transfer,
    eth::balance_change::Reason::GasRefund,
    eth::balance_change::Reason::RewardTransactionFee,
    eth::balance_change::Reason::SuicideRefund,
];

fn selfdestruct_credit_shifts(block: &eth::Block, records: &[Record]) -> HashMap<(Vec<u8>, u64), BigInt> {
    let mut shifts: HashMap<(Vec<u8>, u64), BigInt> = HashMap::new();
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        for call in trx.calls.iter().filter(|c| c.suicide && !c.state_reverted) {
            let destroyed = match call.call_type.as_known() {
                Some(eth::CallType::DELEGATE | eth::CallType::CALLCODE) => &call.caller,
                _ => &call.address,
            };
            let [withdraw, refund] = call.balance_changes.as_slice() else {
                continue;
            };
            let created = trx
                .calls
                .iter()
                .any(|c| c.call_type.as_known() == Some(eth::CallType::CREATE) && c.address == *destroyed);
            if !created
                || withdraw.address != *destroyed
                || refund.address == *destroyed
                || withdraw.reason.as_known() != Some(eth::balance_change::Reason::SuicideWithdraw)
                || refund.reason.as_known() != Some(eth::balance_change::Reason::SuicideRefund)
                || withdraw.ordinal == 0
                || refund.ordinal != withdraw.ordinal + 1
            {
                continue;
            }
            let words = (
                value(withdraw.old_value.as_option().map(|v| v.bytes.as_slice())),
                value(withdraw.new_value.as_option().map(|v| v.bytes.as_slice())),
                value(refund.old_value.as_option().map(|v| v.bytes.as_slice())),
                value(refund.new_value.as_option().map(|v| v.bytes.as_slice())),
            );
            let (Ok(d), Ok(withdraw_new), Ok(b), Ok(refund_new)) = words else {
                continue;
            };
            let [moved, before, credited] = [d, b, refund_new].map(|w| BigInt::from_unsigned_bytes_be(&w));
            if d == [0; 32] || withdraw_new != [0; 32] || credited != before + moved.clone() {
                continue;
            }
            let persisted = records
                .iter()
                .any(|r| r.address == refund.address && r.ordinal == refund.ordinal && value(r.new_value.as_deref()).is_ok_and(|new| new == refund_new));
            if !persisted {
                continue;
            }
            // The beneficiary's later persisted records in the transaction.
            let mut later: Vec<&Record> = records
                .iter()
                .filter(|r| r.address == refund.address && r.ordinal > refund.ordinal && r.ordinal < trx.end_ordinal)
                .collect();
            later.sort_by_key(|r| r.ordinal);
            let starts_without_credit = later.first().is_some_and(|r| value(r.old_value.as_deref()).is_ok_and(|old| old == b));
            let shiftable = later.iter().all(|r| SHIFTABLE.iter().any(|reason| *reason as i32 == r.reason));
            if !starts_without_credit || !shiftable {
                continue;
            }
            for r in later {
                let shift = shifts.entry((r.address.clone(), r.ordinal)).or_insert_with(|| BigInt::from(0));
                *shift = shift.clone() + moved.clone();
            }
        }
    }
    shifts
}

/// Adds `shift` to a record's value.
fn shifted(bytes: Option<&[u8]>, shift: &BigInt) -> Result<Vec<u8>, Error> {
    let total = BigInt::from_unsigned_bytes_be(&value(bytes)?) + shift.clone();
    Ok(word(&total.to_bytes_be().1)?.to_vec())
}

/// Reduce persisted records by execution ordinal into one row per account.
/// Every record needs a canonical 20-byte account, a positive ordinal, values
/// that fit `uint256`, a strictly increasing ordinal per account and old/new
/// continuity. Ambiguous or discontinuous producer data fails the block. Five
/// producer shapes are exceptions. The self-transfer pair
/// (`self_transfer_pairs`) and the EIP-6780 self-destruct to itself
/// (`selfdestruct_to_self_pairs`) are dropped, so the account's other records
/// must chain across them. The beneficiary's records that follow an EIP-6780
/// self-destruct credit from an account created in the transaction
/// (`selfdestruct_credit_shifts`) are shifted by the credit, so they must
/// chain from it. The fee pool's system-reward payout (`fee_pool_payouts`) is
/// restored from the `n -> n` record the shared rules drop, so the pool must
/// chain through it. The debit of an account destroyed by an EIP-6780
/// self-destruct (`selfdestruct_debits`) is restored from its beneficiary's
/// credit, so the account must chain through it.
pub fn changes(block: &eth::Block) -> Result<Vec<Change>, Error> {
    let mut records = records(block)?;
    let mut pairs = self_transfer_pairs(block);
    pairs.extend(selfdestruct_to_self_pairs(block));
    records.retain(|r| !(r.scope == persist::Scope::Tx && pairs.contains(&(r.address.clone(), r.ordinal))));
    records.extend(fee_pool_payouts(block));
    records.extend(selfdestruct_debits(block));
    // After the restored records, so the first record shifted is the one that
    // follows the credit in the continuity check.
    let shifts = selfdestruct_credit_shifts(block, &records);
    for record in records.iter_mut() {
        if let Some(shift) = shifts.get(&(record.address.clone(), record.ordinal)) {
            record.old_value = Some(shifted(record.old_value.as_deref(), shift)?);
            record.new_value = Some(shifted(record.new_value.as_deref(), shift)?);
        }
    }
    // Array order is not execution order: block-level records follow the
    // transactions in the message but can precede or follow them by ordinal.
    records.sort_by_key(|r| r.ordinal);
    let mut rows: BTreeMap<Vec<u8>, (Change, [u8; 32])> = BTreeMap::new();
    for record in records {
        require(record.address.len() == 20, "invalid native account address")?;
        require(record.ordinal > 0, "persisted native balance change has no execution ordinal")?;
        let old = value(record.old_value.as_deref())?;
        let new = value(record.new_value.as_deref())?;
        match rows.get_mut(&record.address) {
            Some((row, last)) => {
                require(record.ordinal > row.ordinal, "ambiguous native balance execution order")?;
                require(old == *last, "discontinuous native balance changes within block")?;
                row.amount = amount(&new);
                row.ordinal = record.ordinal;
                row.records += 1;
                *last = new;
            }
            None => {
                rows.insert(
                    record.address.clone(),
                    (
                        Change {
                            address: record.address,
                            old_amount: amount(&old),
                            amount: amount(&new),
                            first_ordinal: record.ordinal,
                            ordinal: record.ordinal,
                            records: 1,
                        },
                        new,
                    ),
                );
            }
        }
    }
    Ok(rows.into_values().map(|(row, _)| row).collect())
}

/// Final balances of every changed account, sorted by account. Includes the
/// zero address, precompiles, system and burn-looking addresses whenever the
/// producer persisted a change for them.
pub fn project(block: &eth::Block) -> Result<balances_pb::NativeEvents, Error> {
    Ok(balances_pb::NativeEvents {
        balances: changes(block)?
            .into_iter()
            .map(|c| balances_pb::NativeBalance {
                address: c.address,
                amount: c.amount,
            })
            .collect(),
    })
}

/// The element memory one block decode may materialize: 1 GiB.
///
/// buffa bounds what the elements of repeated fields materialize, 32 MiB by
/// default, so that a small untrusted payload cannot force a large
/// allocation. A Firehose block is trusted input, and BSC block 43,772,003
/// (29.9 MB encoded) failed to decode under the defaults that the SDK's
/// handler macro applies. The wasm32 heap bounds the decode anyway; this
/// budget only keeps its failure an explicit decode error.
pub const BLOCK_ELEMENT_MEMORY_LIMIT: usize = 1 << 30;

/// Decodes the `map_events` input block with [`BLOCK_ELEMENT_MEMORY_LIMIT`];
/// buffa's other decode limits keep their defaults.
pub fn decode_block(bytes: &[u8]) -> Result<eth::Block, buffa::DecodeError> {
    buffa::DecodeOptions::new()
        .with_element_memory_limit(BLOCK_ELEMENT_MEMORY_LIMIT)
        .decode_from_slice(bytes)
}

// The SDK macro generates raw-pointer parameter decoding and discards function
// attributes. Keep its ABI-specific lint exception scoped to this wrapper. As in
// the other packages, the export exists only in the WASM build so host crates
// can link this crate next to other packages.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;

    // Written out because the macro decodes with buffa's default limits (see
    // `decode_block`). Otherwise it is the macro's map export: panic hook,
    // empty-output skip, and a panic with the error's debug text.
    #[no_mangle]
    pub extern "C" fn map_events(block_ptr: *mut u8, block_len: usize) {
        substreams::register_panic_hook();
        // SAFETY: the runtime passes the block's bytes, valid for this call.
        let bytes = unsafe { std::slice::from_raw_parts(block_ptr, block_len) };
        let block =
            decode_block(bytes).unwrap_or_else(|error| panic!("Unable to decode Protobuf data ({block_len} bytes) to 'eth::Block' message's struct: {error}"));
        substreams::skip_empty_output();
        match project(&block) {
            Ok(events) => substreams::output(events),
            Err(error) => panic!("{error:?}"),
        }
    }

    #[substreams::handlers::map]
    fn db_out(
        clock: substreams::pb::substreams::Clock,
        events: balances_pb::NativeEvents,
    ) -> Result<substreams_database_change::pb::sf::substreams::sink::database::v1::DatabaseChanges, Error> {
        db_out::database_changes(&clock, &events)
    }
}

#[cfg(test)]
mod tests;
