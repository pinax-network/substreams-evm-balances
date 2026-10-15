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
//! system calls, block-level records), plus the fees and the OP-stack deposit
//! mint that some chains apply to a failed transaction too ([`records`]).
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
    /// The producer's ordinals of the first and the last persisted change in
    /// execution order. Only where a Bor state-sync transaction restarts the
    /// ordinals in each depth-1 call (`bor_state_sync`; observed in blocks
    /// 10,076,416 to 70,044,000) can `ordinal` be lower than `first_ordinal`,
    /// and two records of an account share an ordinal in different depth-1
    /// calls.
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
    /// For a record of a Bor state-sync transaction (`bor_state_sync`): the
    /// transaction's begin ordinal and the position of the record's depth-1
    /// call among the transaction's depth-1 calls, which orders the record
    /// before its ordinal does. `None` for every other record, which its
    /// ordinal orders.
    pub state_sync_call: Option<(u64, u32)>,
}

#[derive(Default)]
struct Collected {
    records: Vec<Record>,
    state_sync: Option<StateSync>,
}
impl persist::Sink for Collected {
    fn storage(&mut self, _: &eth::StorageChange, _: persist::Ctx) {}
    fn balance(&mut self, c: &eth::BalanceChange, ctx: persist::Ctx) {
        let state_sync_call = self
            .state_sync
            .as_ref()
            .filter(|s| ctx.scope == persist::Scope::Tx && ctx.tx_index == s.tx_index)
            .and_then(|s| s.positions.get(&ctx.call_index).map(|&position| (s.begin_ordinal, position)));
        self.records.push(Record {
            address: c.address.clone(),
            old_value: c.old_value.as_option().map(|v| v.bytes.clone()),
            new_value: c.new_value.as_option().map(|v| v.bytes.clone()),
            ordinal: c.ordinal,
            reason: c.reason.to_i32(),
            scope: ctx.scope,
            state_sync_call,
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
/// do a blob transaction's blob fee ([`failed_blob_fees`]), a base-fee
/// `BURN` credit ([`failed_base_fee_burns`]), the `TRANSFER` credits that
/// pay the fee after the execution ([`failed_fee_transfers`]) and the
/// `INCREASE_MINT` of a failed OP-stack deposit in its exact shape
/// ([`failed_deposit_mint`]), the `TRANSFER` records that settle a retryable
/// redeem ([`failed_retry_settlements`]), and the reasons listed here revert
/// with the execution frame. Any other reason in a failed transaction's root
/// call (for example an `INCREASE_MINT` outside that shape, a blob fee outside
/// a blob transaction, or a reason this crate cannot name) has no fixture, so
/// the block fails instead of silently dropping a possible credit.
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
        let mint = failed_deposit_mint(tx);
        for change in &root.balance_changes {
            let pinned = persist::is_gas_reason(change)
                || is_blob_fee_of(tx, change)
                || mint.is_some_and(|mint| std::ptr::eq(mint, change))
                || change.reason.as_known().is_some_and(|r| REVERTIBLE_FAILED_REASONS.contains(&r));
            require(
                pinned,
                "failed transaction carries a balance-change reason without pinned persistence semantics",
            )?;
        }
    }
    Ok(())
}

/// A failed transaction's root-call record outside its execution cannot
/// belong to the reverted frame. The silent wrong balances found so far came
/// from records that the chain keeps whatever the outcome, most of them after
/// or before the execution: the shared rules keep only a failed root call's
/// gas records, and the reason guard (`validate_failed_transaction_reasons`)
/// lets the reasons that revert with the frame through, so such a record was
/// dropped, and a dropped record that is its account's last in the block still
/// chains. Arbitrum's fee transfers and retry settlements after the execution
/// ([`failed_fee_transfers`], [`failed_retry_settlements`]), Polygon's
/// base-fee `BURN` ([`failed_base_fee_burns`]), BNB Chain's blob fee
/// ([`failed_blob_fees`]) and the OP-stack deposit mint before the execution
/// ([`failed_deposit_mint`]) are each pinned in their exact shape; what ArbOS
/// keeps inside a root call recorded from ordinal 0 is pinned by transaction
/// type ([`failed_arbos_settlements`], [`failed_retry_settlements`]).
///
/// Fails the block when a record of a failed transaction's root call moves a
/// balance (its values differ, or one exceeds `uint256`), lies outside the
/// execution and does not persist as recorded (`records`): after it, past
/// the root call's end ordinal, or before it, at a positive ordinal below the
/// root call's begin ordinal. The producers of `Block.ver` 3 record every root
/// call's begin ordinal as 0 (`evm_persist`), so only the window after the
/// execution applies to them; those of `Block.ver` 4 and 5 record the begin
/// after the sender's `GAS_BUY` and a deposit's mint. A root call without an
/// end ordinal in a transaction with one cannot place its records, so any
/// record that moves a balance and does not persist fails the block too; a
/// transaction without ordinals (end ordinal 0), as synthetic traces are, is
/// left to the other checks. A record during the execution, or at ordinal 0,
/// reverts with the frame as before.
///
/// In samples of every supported chain and era (2026-10-15: Ethereum,
/// BNB Chain, Polygon, Arbitrum One, Base, Optimism, Unichain and HyperEVM,
/// windows of 1,000 to 27,000 blocks from each chain's early blocks to its
/// head, and the blocks of every captured failed-transaction shape), every
/// record of a failed root call outside its execution that moves a balance
/// persists by the rules above or as a gas record, and every failed root call
/// has an end ordinal. `changes` applies this check after the reduction, so a
/// dropped record that already breaks continuity keeps that error.
fn validate_failed_records_outside_execution(block: &eth::Block, records: &[Record]) -> Result<(), Error> {
    type Key<'a> = (&'a [u8], u64, i32, Option<&'a [u8]>, Option<&'a [u8]>);
    let persisted: HashSet<Key> = records
        .iter()
        .filter(|r| r.scope == persist::Scope::TxFailedPersistent)
        .map(|r| (r.address.as_slice(), r.ordinal, r.reason, r.old_value.as_deref(), r.new_value.as_deref()))
        .collect();
    for tx in block.transaction_traces.iter().filter(|tx| persist::is_failed(tx)) {
        let Some(root) = tx.calls.first() else { continue };
        for c in &root.balance_changes {
            let message = if root.end_ordinal == 0 {
                if tx.end_ordinal == 0 {
                    continue;
                }
                "failed transaction carries a balance record that its root call's missing end ordinal cannot place"
            } else if c.ordinal > root.end_ordinal {
                "failed transaction carries a balance record after its execution without pinned persistence semantics"
            } else if c.ordinal > 0 && c.ordinal < root.begin_ordinal {
                "failed transaction carries a balance record before its execution without pinned persistence semantics"
            } else {
                continue;
            };
            let key: Key = (
                c.address.as_slice(),
                c.ordinal,
                c.reason.to_i32(),
                c.old_value.as_option().map(|v| v.bytes.as_slice()),
                c.new_value.as_option().map(|v| v.bytes.as_slice()),
            );
            let moves = value_change(c).is_none_or(|moved| !moved.is_zero());
            require(!moves || persisted.contains(&key), message)?;
        }
    }
    Ok(())
}

/// Every persisted native balance record in the block, with its persistence
/// scope, retained for package tests and offline inspection: the shared
/// rules' records in array order, then the blob fees of failed blob
/// transactions, which BNB Chain pays whatever the outcome, the base-fee
/// credits of failed transactions, which Polygon pays whatever the outcome,
/// the fee transfers of failed transactions and the settlement of failed
/// retryable redeems, which Arbitrum pays whatever the outcome, and the mints
/// of failed deposits, which the OP stack applies whatever the outcome. A
/// record of a Bor state-sync transaction carries its depth-1 call
/// (`bor_state_sync`); a block with a state-sync transaction in another
/// ordinal layout fails (`unrecognized_bor_state_sync`).
pub fn records(block: &eth::Block) -> Result<Vec<Record>, Error> {
    validate_block(block)?;
    validate_failed_transaction_reasons(block)?;
    let state_sync = bor_state_sync(block);
    require(
        !unrecognized_bor_state_sync(block, state_sync.as_ref()),
        "unrecognized Bor state-sync ordinal layout",
    )?;
    let mut collected = Collected {
        records: Vec::new(),
        state_sync,
    };
    persist::collect_block(block, &mut collected)?;
    collected.records.extend(failed_blob_fees(block));
    collected.records.extend(failed_base_fee_burns(block));
    collected.records.extend(failed_fee_transfers(block));
    collected.records.extend(failed_deposit_mints(block));
    collected.records.extend(failed_retry_settlements(block)?);
    collected.records.extend(failed_arbos_settlements(block)?);
    Ok(collected.records)
}

/// An OP-stack deposit (type `0x7E`) mints its `mint` value to its sender
/// before the execution, outside the execution's snapshot: the
/// [deposit specification](https://specs.optimism.io/protocol/deposits.html#execution)
/// keeps the mint ("unconditional, and does not revert on deposit
/// failure"), and increments the sender's nonce, when the deposit fails. A
/// deposit buys no gas and pays no fee. The Base, Optimism and Unichain
/// producers of `Block.ver` 4 and the Optimism producer of `Block.ver` 5
/// record the mint of a failed deposit as an `INCREASE_MINT` credit of the
/// sender in the root call, after the transaction's begin ordinal and before
/// the execution's, as they do for a succeeded deposit. Base `Block.ver` 5
/// records a succeeded deposit's mint the same way (block 50,024,356,
/// `0x4c1fd85c…` from `0x977f82a6…`); no failed deposit of Base or Unichain
/// `Block.ver` 5 is captured. In Base block 5,044,695 the failed deposit
/// `0xe707514e…` (transaction 1) from and to `0x82c8c1fb…` sends more than
/// the sender holds after the mint, so it fails outside the EVM: its root
/// call holds only the credit of 2,966,081,819,000,000 wei, the deposit's
/// `mint`, at ordinal 25, and the chain reads the credited balance after the
/// block (Base blocks 3,037,177, `0x55ff0ede…`, and 10,099,941,
/// `0x6202bc8b…`, have the same shape, as do Optimism block 106,973,772,
/// `0x99ba9cf4…` from `0xbc339e62…`, and Optimism `Block.ver` 5 block
/// 156,295,824, `0xcb154738…` from `0x95a7da8a…`). In Base block 7,092,606
/// the failed deposit `0x3869ca9e…` from `0x8117a188…` reverts in the EVM:
/// its root call also holds the reverted value transfer to `0x00008453…`, and
/// the chain reads the sender's balance with the mint and without the
/// transfer (as in Optimism block 107,494,341, `0xfcefb0a0…` from
/// `0x333601a8…`, and Unichain block 744,856, `0xeff18402…` from
/// `0x9c5a151f…`). Returns the mint when the shape is exact: a failed
/// deposit whose root call (depth 0) holds exactly one `INCREASE_MINT`, a
/// positive credit of the transaction's sender between the transaction's
/// begin ordinal and the execution's, and no other record in that interval.
/// Any other `INCREASE_MINT` in a failed transaction's root call still fails
/// the block (`validate_failed_transaction_reasons`).
fn failed_deposit_mint(tx: &eth::TransactionTrace) -> Option<&eth::BalanceChange> {
    if tx.r#type != eth::transaction_trace::Type::TrxTypeOptimismDeposit || !persist::is_failed(tx) {
        return None;
    }
    let root = tx.calls.first().filter(|c| c.depth == 0)?;
    let before_execution = |c: &eth::BalanceChange| c.ordinal > tx.begin_ordinal && c.ordinal < root.begin_ordinal;
    let mints: Vec<_> = root
        .balance_changes
        .iter()
        .filter(|c| c.reason.as_known() == Some(eth::balance_change::Reason::IncreaseMint))
        .collect();
    let [mint] = mints.as_slice() else {
        return None;
    };
    let old = value(mint.old_value.as_option().map(|v| v.bytes.as_slice())).ok()?;
    let new = value(mint.new_value.as_option().map(|v| v.bytes.as_slice())).ok()?;
    // The mint is the root call's only record before the execution.
    let before: Vec<_> = root.balance_changes.iter().filter(|c| before_execution(c)).collect();
    let alone = matches!(before.as_slice(), [only] if std::ptr::eq(*only, *mint));
    (mint.address == tx.from && alone && new > old).then_some(*mint)
}

/// The mints of failed deposits in their exact shape ([`failed_deposit_mint`]).
/// Like every record, they must chain.
fn failed_deposit_mints(block: &eth::Block) -> Vec<Record> {
    block
        .transaction_traces
        .iter()
        .filter_map(failed_deposit_mint)
        .map(|c| Record {
            address: c.address.clone(),
            old_value: c.old_value.as_option().map(|v| v.bytes.clone()),
            new_value: c.new_value.as_option().map(|v| v.bytes.clone()),
            ordinal: c.ordinal,
            reason: c.reason.to_i32(),
            scope: persist::Scope::TxFailedPersistent,
            state_sync_call: None,
        })
        .collect()
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
                state_sync_call: None,
            });
        }
    }
    fees
}

/// Polygon's Bor client does not burn a transaction's base fee: it credits
/// `gas_used * base_fee` to its burn contract with a `BURN` record in the
/// root call, after the gas refund and before the fee credit, and does so
/// for a failed transaction too. In block 95,537,024 the reverted
/// transactions `0xad18a4f2…` (tx 4) and `0x20b9dd82…` (tx 19) credit
/// `0x3ef57def…` 429,288 and 79,010 gas at the base fee, and the account's
/// next record starts from the credited balance. The shared rules persist
/// only the three gas reasons of a failed transaction's root call. Returns the
/// `BURN` records of failed transactions' root calls that move exactly
/// `gas_used * base_fee` (base fee positive); any other `BURN` there still
/// reverts. Like every record, they must chain.
fn failed_base_fee_burns(block: &eth::Block) -> Vec<Record> {
    let big = |v: Option<&eth::BigInt>| v.map_or_else(|| BigInt::from(0), |v| BigInt::from_unsigned_bytes_be(&v.bytes));
    let base_fee = big(block.header.as_option().and_then(|h| h.base_fee_per_gas.as_option()));
    if base_fee.is_zero() {
        return Vec::new();
    }
    let mut burns = Vec::new();
    for tx in block.transaction_traces.iter().filter(|tx| persist::is_failed(tx)) {
        let Some(root) = tx.calls.first() else { continue };
        let fee = BigInt::from(tx.gas_used) * base_fee.clone();
        for c in root
            .balance_changes
            .iter()
            .filter(|c| c.reason.as_known() == Some(eth::balance_change::Reason::Burn))
        {
            let (old, new) = (
                c.old_value.as_option().map(|v| v.bytes.clone()),
                c.new_value.as_option().map(|v| v.bytes.clone()),
            );
            let (Ok(old_word), Ok(new_word)) = (value(old.as_deref()), value(new.as_deref())) else {
                continue;
            };
            if BigInt::from_unsigned_bytes_be(&new_word) - BigInt::from_unsigned_bytes_be(&old_word) != fee {
                continue;
            }
            burns.push(Record {
                address: c.address.clone(),
                old_value: old,
                new_value: new,
                ordinal: c.ordinal,
                reason: c.reason.to_i32(),
                scope: persist::Scope::TxFailedPersistent,
                state_sync_call: None,
            });
        }
    }
    burns
}

/// Arbitrum's ArbOS pays a transaction's fee after the execution, in the
/// root call: after the sender's `GAS_REFUND`, `TRANSFER` credits to its fee
/// accounts (the infrastructure fee account `0xbf5041fc…`, the L1 pricer pool
/// `0xa4b0…00f6` and, in later blocks, the network fee account
/// `0x32e7af5a…`, which may also take the tip as a `REWARD_TRANSACTION_FEE`).
/// These credits add up to the gas the sender paid, its `GAS_BUY` debit less
/// its `GAS_REFUND` credit, and ArbOS pays them for a failed transaction too.
/// In block 150,000,072 the reverted transactions 4 to 7 (`0x4bfa5ca1…` on)
/// credit both accounts, and their next records, in transaction 8, start
/// from the credited balances. The shared rules persist only the three gas
/// reasons of a failed transaction's root call. Returns the `TRANSFER`
/// records of a failed transaction's root call after its execution (ordinal
/// past the call's end) when the shape is exact: the call holds exactly one
/// `GAS_BUY`, a debit `g` of the payer, and at most one `GAS_REFUND`, a
/// credit `r` of the payer, and every other record after the execution is a
/// positive `TRANSFER` or `REWARD_TRANSACTION_FEE` credit of another account,
/// at least one a `TRANSFER`, adding up to `g - r`. Any other shape still
/// reverts. Like every record, they must chain.
fn failed_fee_transfers(block: &eth::Block) -> Vec<Record> {
    use eth::balance_change::Reason;
    let words = |c: &eth::BalanceChange| -> Option<(BigInt, BigInt)> {
        let old = value(c.old_value.as_option().map(|v| v.bytes.as_slice())).ok()?;
        let new = value(c.new_value.as_option().map(|v| v.bytes.as_slice())).ok()?;
        Some((BigInt::from_unsigned_bytes_be(&old), BigInt::from_unsigned_bytes_be(&new)))
    };
    let mut transfers = Vec::new();
    for tx in block.transaction_traces.iter().filter(|tx| persist::is_failed(tx)) {
        let Some(root) = tx.calls.first() else { continue };
        let of = |reason: Reason| root.balance_changes.iter().filter(move |c| c.reason.as_known() == Some(reason));
        let (buys, refunds): (Vec<_>, Vec<_>) = (of(Reason::GasBuy).collect(), of(Reason::GasRefund).collect());
        let [buy] = buys.as_slice() else { continue };
        let Some((old, new)) = words(buy) else { continue };
        let mut paid = old - new;
        match refunds.as_slice() {
            [] => {}
            [refund] if refund.address == buy.address => {
                let Some((old, new)) = words(refund) else { continue };
                paid -= new - old;
            }
            _ => continue,
        }
        let after: Vec<_> = root
            .balance_changes
            .iter()
            .filter(|c| c.ordinal > root.end_ordinal && c.reason.as_known() != Some(Reason::GasRefund))
            .collect();
        let mut credited = BigInt::from(0);
        let mut exact = after.iter().any(|c| c.reason.as_known() == Some(Reason::Transfer));
        for c in &after {
            let fee = matches!(c.reason.as_known(), Some(Reason::Transfer | Reason::RewardTransactionFee));
            match words(c) {
                Some((old, new)) if fee && c.address != buy.address && new > old => credited += new - old,
                _ => exact = false,
            }
        }
        if !exact || credited != paid {
            continue;
        }
        for c in after.into_iter().filter(|c| c.reason.as_known() == Some(Reason::Transfer)) {
            transfers.push(Record {
                address: c.address.clone(),
                old_value: c.old_value.as_option().map(|v| v.bytes.clone()),
                new_value: c.new_value.as_option().map(|v| v.bytes.clone()),
                ordinal: c.ordinal,
                reason: c.reason.to_i32(),
                scope: persist::Scope::TxFailedPersistent,
                state_sync_call: None,
            });
        }
    }
    transfers
}

/// Arbitrum's ArbOS applies three transaction types in Nitro's `StartTxHook`
/// (`arbos/tx_processor.go`) and ends them there, without an EVM execution
/// or a snapshot, so the chain keeps their balance moves when the hook fails
/// the transaction (the transaction's result carries the hook's error, and
/// the state stays). An `ArbitrumDepositTx` (type 100) mints its value to its
/// sender and moves it to its recipient; since Nitro v3.10.0 a deposit that
/// the onchain filter redirects fails after both moves. An
/// `ArbitrumSubmitRetryableTx` (type 105) mints its deposit to its sender,
/// then pays the submission fee and the excess refund, the call value into
/// the ticket's escrow and the auto-redeem's gas: it fails after the mint
/// when the sender holds less than the maximum submission fee, after the fees
/// and their refunds when it cannot pay the call value (both
/// [`failed_submission_settlement`]), and, since Nitro v3.10.0, after every
/// move when the onchain filter redirects it. An `ArbitrumInternalTx` (type
/// 106) pays batch posters from the L1 pricer pool and reaps expired tickets;
/// Nitro's block processor rejects a block whose internal transaction fails,
/// so none should appear. The Arbitrum One producer of `Block.ver` 3 records
/// these moves as `TRANSFER` records in the root call (block 52,067,707,
/// submission `0x38b8e37a…`) or, in later blocks, in its depth-1 call (block
/// 514,497,707, submission `0x9c77cfe9…`; block 514,709,255, batch posting
/// report `0x414a3691…`; block 514,523,401, deposit `0x4c841c87…`, whose mint
/// stays in the root call). The shared rules persist only the gas records of
/// a failed transaction's root call, so they would drop these moves, and the
/// accounts can still chain with wrong balances. Of the submissions that
/// Arbitrum One's L1 inbox delivered from Nitro's start to Ethereum block
/// 26,194,531, 16 failed, the last in block 368,841,870: seven without a
/// balance record and nine in the two shapes of
/// [`failed_submission_settlement`]. Returns the settlements of failed
/// submissions in their exact shapes. Fails the block when a failed
/// transaction of type 100, 105 or 106 holds, in any of its calls, any other
/// record that moves a balance, other than a gas record of its root call,
/// which persists. An `ArbitrumUnsignedTx` (101) or `ArbitrumContractTx`
/// (102) runs in the EVM like any other transaction, so its value transfer
/// reverts with the execution and only its fee persists
/// ([`failed_fee_transfers`]), and a failed `ArbitrumRetryTx` (104) settles in
/// its exact shape ([`failed_retry_settlements`]); neither is refused here.
/// Like every record, the settlements must chain.
fn failed_arbos_settlements(block: &eth::Block) -> Result<Vec<Record>, Error> {
    use eth::transaction_trace::Type;
    let applied_by_arbos = |tx: &&eth::TransactionTrace| {
        matches!(
            tx.r#type.as_known(),
            Some(Type::TrxTypeArbitrumDeposit | Type::TrxTypeArbitrumSubmitRetryable | Type::TrxTypeArbitrumInternal)
        )
    };
    let mut settled = Vec::new();
    for tx in block.transaction_traces.iter().filter(|tx| persist::is_failed(tx)).filter(applied_by_arbos) {
        let settlement = failed_submission_settlement(tx).unwrap_or_default();
        for (position, call) in tx.calls.iter().enumerate() {
            for change in &call.balance_changes {
                let moves = value_change(change).is_none_or(|moved| !moved.is_zero());
                let persists = (position == 0 && persist::is_gas_reason(change)) || settlement.iter().any(|c| std::ptr::eq(*c, change));
                require(
                    !moves || persists,
                    "failed Arbitrum deposit, retryable submission or internal transaction carries balance records that ArbOS keeps",
                )?;
            }
        }
        settled.extend(settlement.into_iter().map(|c| Record {
            address: c.address.clone(),
            old_value: c.old_value.as_option().map(|v| v.bytes.clone()),
            new_value: c.new_value.as_option().map(|v| v.bytes.clone()),
            ordinal: c.ordinal,
            reason: c.reason.to_i32(),
            scope: persist::Scope::TxFailedPersistent,
            state_sync_call: None,
        }));
    }
    Ok(settled)
}

/// The settlement of an `ArbitrumSubmitRetryableTx` (type 105) that fails in
/// `StartTxHook`, which uses no gas, and which the chain keeps. In block
/// 71,044,381 the submission `0x6afc76c4…` (transaction 1, L1 message
/// 632,385) from `0x26196e02…` mints its deposit of 17,400,000,000,000 wei
/// and fails below its maximum submission fee of 70,000,000,000,000 wei, so
/// the mint is its only move (block 71,049,015, `0x1a8eeca0…`, repeats it). In
/// block 333,543,134 the submission `0x8113baed…` (transaction 1, L1 message
/// 1,962,555) from `0x06fe418a…` mints its deposit of 60,000,000,000,000 wei,
/// pays its submission fee of 2,004,369,337,072 wei to the network fee
/// account `0x32e7af5a…` and, unable to pay its call value of 0.05 ETH, takes
/// the fee back; its deposit, less the call value, leaves nothing for the
/// excess refund (blocks 336,291,645 to 336,722,229 repeat it six times). The
/// Arbitrum One producer of `Block.ver` 3 records each submission as `FAILED`,
/// with one call, its root, holding each move as a `TRANSFER`, and
/// eth_getBalance reads the moved balances after each block. Returns the
/// moves when the shape is exact: a `FAILED` submission without gas used
/// whose only call is its root call (depth 0), in which the records that move
/// a balance, all `TRANSFER` records after the transaction's begin ordinal and
/// before the root call's end, are, in ordinal order, a credit of the
/// transaction's sender (the mint), optionally followed by a debit of the
/// sender of a positive fee, a credit of the fee to another account, a debit
/// of the fee of that account and a credit of the fee to the sender. Any
/// other record of a failed submission, such as the excess or withheld
/// refunds of the call-value failure or a later producer's depth-1 layout,
/// still fails the block ([`failed_arbos_settlements`]).
fn failed_submission_settlement(tx: &eth::TransactionTrace) -> Option<Vec<&eth::BalanceChange>> {
    let failed = tx.status == eth::TransactionTraceStatus::Failed;
    if tx.r#type != eth::transaction_trace::Type::TrxTypeArbitrumSubmitRetryable || !failed || tx.gas_used != 0 {
        return None;
    }
    let [root] = tx.calls.as_slice() else { return None };
    let mut moves = Vec::new();
    for c in &root.balance_changes {
        let moved = value_change(c)?;
        if !moved.is_zero() {
            moves.push((c, moved));
        }
    }
    moves.sort_by_key(|(c, _)| c.ordinal);
    let sender = tx.from.as_slice();
    // A `TRANSFER` of `account` (or of any other account than the sender)
    // that moves exactly `moved`.
    let transfer = |m: &(&eth::BalanceChange, BigInt), account: Option<&[u8]>, moved: &BigInt| {
        m.0.reason.as_known() == Some(eth::balance_change::Reason::Transfer) && account.map_or(m.0.address != sender, |a| m.0.address == a) && m.1 == *moved
    };
    let minted = |m: &(&eth::BalanceChange, BigInt)| m.1 > 0 && transfer(m, Some(sender), &m.1);
    let within = moves.iter().all(|(c, _)| c.ordinal > tx.begin_ordinal && c.ordinal < root.end_ordinal);
    let exact = match moves.as_slice() {
        [mint] => minted(mint),
        [mint, debit, credit, back, refund] => {
            let (fee, payee) = (&credit.1, credit.0.address.as_slice());
            minted(mint)
                && *fee > 0
                && transfer(debit, Some(sender), &-fee.clone())
                && transfer(credit, None, fee)
                && transfer(back, Some(payee), &-fee.clone())
                && transfer(refund, Some(sender), fee)
        }
        _ => false,
    };
    (root.depth == 0 && within && exact).then(|| moves.into_iter().map(|(c, _)| c).collect())
}

/// Arbitrum's ArbOS settles a retryable redeem, an `ArbitrumRetryTx` (type
/// 104), outside its EVM execution (`StartTxHook` and `EndTxHook` in Nitro's
/// `arbos/tx_processor.go`), and the chain keeps the settlement when the
/// redeem fails. Before the gas purchase it moves the ticket's call value `v`
/// from the ticket's escrow to the sender and mints the prepaid gas, `g * b`
/// (gas limit times base fee), to the sender. After the execution and the
/// sender's `GAS_REFUND` of the gas left, `l * b`, it burns that refund again,
/// pays the same amount from its fee accounts (the infrastructure fee account,
/// then the network fee account) to the ticket's refund address, or to the
/// sender past the ticket's maximum refund, and returns `v` to the escrow. The
/// Arbitrum One producer of `Block.ver` 3 records each move as a `TRANSFER` in
/// the root call. In block 52,067,707 the reverted auto-redeem `0x533a62f8…`
/// (transaction 2) mints 7,892,600,000,000 wei to its sender `0xb4b8b6f8…`
/// right before a `GAS_BUY` of that amount, and after its `GAS_REFUND` of
/// 115,400,000,000 wei the network fee account `0xd345e41a…` pays that amount
/// to `0x65a8f07b…`; blocks 97,021,351 and 127,044,374 repeat the shape with
/// the network fee account `0x32e7af5a…`. The shared rules persist only the
/// three gas reasons of a failed transaction's root call, so the `GAS_BUY`
/// starts from a balance that no persisted record reached. In block
/// 314,259,052 the reverted manual redeem `0xe0359a51…` (transaction 6) also
/// moves 0.006025454 ETH from the escrow `0xf1642a4c…` to its sender
/// `0x4996f81f…` and back, and both fee accounts pay the refund address
/// `0x2cd28cda…`; without these records every account still chains, but the
/// sender keeps the call value and the refund, which the chain does not.
///
/// Returns the `TRANSFER` records of each failed `ArbitrumRetryTx`'s root call
/// before its `GAS_BUY` and after its execution (ordinal past the call's end)
/// when, no-op records left out and in ordinal order, they are exactly: when
/// `v` is positive, a debit of `v` of another account (the escrow) and a
/// credit of `v` of the sender; a credit of `g * b` of the sender; then
/// exactly one `GAS_BUY` in the call, the sender's debit of `g * b`; after the
/// execution, when `l` is positive, the sender's `GAS_REFUND` of `l * b` and a
/// debit of `l * b` of the sender; pairs of a debit of an account other than
/// the sender and a credit of the same amount of another account, the sender
/// or one refund address, adding up to `l * b`, with no credit of the refund
/// address after one of the sender; and, when `v` is positive, a debit of `v`
/// of the sender and a credit of `v` of the escrow. Any other shape of a
/// failed `ArbitrumRetryTx` with balance records fails the block, as does a
/// value past `uint256` among them or a gas use past the gas limit: its
/// settlement persists, and a block without it can chain with wrong balances.
/// Like every record, they must chain. The order of the payments is Nitro's
/// (`arbos/tx_processor.go` at v2.0.0, v2.3.0, v3.0.0, v3.9.0 and master):
/// each of `EndTxHook`'s refunds pays the refund address out of what is left
/// of the ticket's maximum refund, then the sender the rest, and nothing
/// restores that maximum, so once the sender is paid every later share of the
/// refund address is zero.
///
/// One such shape is the multi-dimensional gas refund
/// (`BalanceChangeMultiGasRefund`). From ArbOS 60, and from ArbOS 61 only
/// under the `MultiGasConstraints` gas model, Nitro's `EndTxHook` makes one
/// more refund when `b * gas_used` exceeds the transaction's
/// multi-dimensional cost: right after the network fee account's refund of
/// the gas left and before the call value goes back to the escrow, the
/// network fee account pays the difference `m` to the refund address, to the
/// sender past the ticket's maximum refund, or in two pairs to both, the
/// refund address first. A failed retry with it carries one or two more
/// pairs from the network fee account, so its pairs add up to `l * b + m`,
/// and the trace does not carry `m`: the block fails until a captured fixture
/// pins that pair. On Arbitrum One the six failed retries the qualification
/// found under ArbOS 61, up to block 511,131,266, carry no such pair.
fn failed_retry_settlements(block: &eth::Block) -> Result<Vec<Record>, Error> {
    let base_fee = block
        .header
        .as_option()
        .and_then(|h| h.base_fee_per_gas.as_option())
        .map_or_else(|| BigInt::from(0), |v| BigInt::from_unsigned_bytes_be(&v.bytes));
    let mut settled = Vec::new();
    for tx in block
        .transaction_traces
        .iter()
        .filter(|tx| persist::is_failed(tx) && tx.r#type == eth::transaction_trace::Type::TrxTypeArbitrumRetry)
    {
        let Some(root) = tx.calls.first() else { continue };
        if root.balance_changes.iter().all(|c| value_change(c).is_some_and(|moved| moved.is_zero())) {
            continue;
        }
        let records =
            retry_settlement(tx, root, &base_fee).ok_or_else(|| Error::msg("failed Arbitrum retry transaction without the exact ArbOS settlement shape"))?;
        settled.extend(records.into_iter().map(|c| Record {
            address: c.address.clone(),
            old_value: c.old_value.as_option().map(|v| v.bytes.clone()),
            new_value: c.new_value.as_option().map(|v| v.bytes.clone()),
            ordinal: c.ordinal,
            reason: c.reason.to_i32(),
            scope: persist::Scope::TxFailedPersistent,
            state_sync_call: None,
        }));
    }
    Ok(settled)
}

/// A record's signed movement, `new - old`, or `None` when a value exceeds
/// `uint256`.
fn value_change(c: &eth::BalanceChange) -> Option<BigInt> {
    let old = value(c.old_value.as_option().map(|v| v.bytes.as_slice())).ok()?;
    let new = value(c.new_value.as_option().map(|v| v.bytes.as_slice())).ok()?;
    Some(BigInt::from_unsigned_bytes_be(&new) - BigInt::from_unsigned_bytes_be(&old))
}

/// The settlement records of a failed `ArbitrumRetryTx`'s root call when its
/// shape is exact (see [`failed_retry_settlements`]).
fn retry_settlement<'a>(tx: &eth::TransactionTrace, root: &'a eth::Call, base_fee: &BigInt) -> Option<Vec<&'a eth::BalanceChange>> {
    use eth::balance_change::Reason;
    let sender = tx.from.as_slice();
    let call_value = tx
        .value
        .as_option()
        .map_or_else(|| BigInt::from(0), |v| BigInt::from_unsigned_bytes_be(&v.bytes));
    let prepaid = BigInt::from(tx.gas_limit) * base_fee.clone();
    let refund = BigInt::from(tx.gas_limit.checked_sub(tx.gas_used)?) * base_fee.clone();
    let mut moves = Vec::new();
    for c in &root.balance_changes {
        let moved = value_change(c)?;
        if !moved.is_zero() {
            moves.push((c, moved));
        }
    }
    moves.sort_by_key(|(c, _)| c.ordinal);
    // A `TRANSFER` of `account` (or of any other account than the sender)
    // that moves exactly `moved`.
    let transfer = |m: &(&eth::BalanceChange, BigInt), account: Option<&[u8]>, moved: &BigInt| {
        m.0.reason.as_known() == Some(Reason::Transfer) && account.map_or(m.0.address != sender, |a| m.0.address == a) && m.1 == *moved
    };
    let buys: Vec<_> = moves.iter().filter(|m| m.0.reason.as_known() == Some(Reason::GasBuy)).collect();
    let [buy] = buys.as_slice() else { return None };
    if buy.0.address != sender || buy.1 != -prepaid.clone() {
        return None;
    }
    let before: Vec<_> = moves.iter().filter(|m| m.0.ordinal < buy.0.ordinal).collect();
    let after: Vec<_> = moves.iter().filter(|m| m.0.ordinal > root.end_ordinal).collect();
    // Before the gas purchase: the call value from the escrow, then the
    // prepaid gas.
    let (escrow, mint) = match before.as_slice() {
        [mint] if call_value.is_zero() => (None, mint),
        [debit, credit, mint] if transfer(debit, None, &-call_value.clone()) && transfer(credit, Some(sender), &call_value) => {
            (Some(debit.0.address.as_slice()), mint)
        }
        _ => return None,
    };
    if !transfer(mint, Some(sender), &prepaid) {
        return None;
    }
    // After the execution: the gas refund and its burn...
    let mut rest = after.as_slice();
    if !refund.is_zero() {
        let [gas, burn, tail @ ..] = rest else { return None };
        let regas = gas.0.reason.as_known() == Some(Reason::GasRefund) && gas.0.address == sender && gas.1 == refund;
        if !regas || !transfer(burn, Some(sender), &-refund.clone()) {
            return None;
        }
        rest = tail;
    }
    // ...the fee accounts' refunds to one refund address, then to the sender...
    let mut refunded = BigInt::from(0);
    let mut last_payee: Option<&[u8]> = None;
    while let [debit, credit, tail @ ..] = rest {
        if debit.0.address == sender {
            break;
        }
        let paid = credit.1.clone();
        let to = credit.0.address.as_slice();
        let pair = paid > 0 && to != debit.0.address && transfer(debit, None, &-paid.clone()) && transfer(credit, Some(to), &paid);
        // An account other than the sender is paid only first or right after
        // its own payment: one refund address, and none after the sender.
        if !pair || (to != sender && last_payee.is_some_and(|p| p != to)) {
            return None;
        }
        last_payee = Some(to);
        refunded += paid;
        rest = tail;
    }
    if refunded != refund {
        return None;
    }
    // ...and the call value back to the escrow.
    match (rest, escrow) {
        ([], None) => {}
        ([debit, credit], Some(escrow)) if transfer(debit, Some(sender), &-call_value.clone()) && transfer(credit, Some(escrow), &call_value) => {}
        _ => return None,
    }
    let transfers = after.into_iter().filter(|m| m.0.reason.as_known() == Some(Reason::Transfer));
    Some(before.into_iter().chain(transfers).map(|m| m.0).collect())
}

/// A value transfer from an account to itself, a CALLCODE with value or a CALL
/// whose caller is its address, does not move the balance; geth records
/// nothing for it. The BSC producer records it as two `TRANSFER` records of
/// that account, at consecutive ordinals of the call, both computed from the
/// same prior balance `b`: `b -> b - v` and `b -> b + v`, where `v` is the
/// call's value (block 127,411,449, transaction `0x5c664637…`). The second
/// record's old value breaks continuity although the balance never moved.
/// The HyperEVM producer records it as two `TRANSFER` records at consecutive
/// ordinals that both end at the balance `b`: `b + v -> b` and `b - v -> b`,
/// followed by the account's actual debit and credit `b -> b - v -> b`
/// (block 44,000,001, transaction `0xbda5325e…`, account `0x20f6ee51…`).
/// Returns those pairs, as `(account, ordinal)`, from non-reverted calls of
/// succeeded transactions, so the account's other records must chain across
/// them. Only these exact shapes are recognized.
fn self_transfer_pairs(block: &eth::Block) -> HashSet<(Vec<u8>, u64)> {
    let mut pairs = HashSet::new();
    let transfer = |c: &&eth::BalanceChange| c.reason.as_known() == Some(eth::balance_change::Reason::Transfer);
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        for call in trx.calls.iter().filter(|c| !c.state_reverted) {
            let Some(v) = self_transfer_value(call) else {
                continue;
            };
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
                let (Ok(o1), Ok(n1), Ok(o2), Ok(n2)) = words else {
                    continue;
                };
                let [o1, n1, o2, n2] = [o1, n1, o2, n2].map(|w| BigInt::from_unsigned_bytes_be(&w));
                // Either both records start from `b` (BSC) or both end at it
                // (HyperEVM); the other ends are `b - v` and `b + v`.
                let apart = |b: &BigInt, x: &BigInt, y: &BigInt| {
                    let (debit, credit) = (b - &v, b + &v);
                    (*x == debit && *y == credit) || (*x == credit && *y == debit)
                };
                if (o1 == o2 && apart(&o1, &n1, &n2)) || (n1 == n2 && apart(&n1, &o1, &o2)) {
                    pairs.insert((call.caller.clone(), first.ordinal));
                    pairs.insert((call.caller.clone(), second.ordinal));
                }
            }
        }
    }
    pairs
}

/// The value of a call that moves value from an account to itself, a CALLCODE
/// with value or a CALL with value whose caller is its address: the calls
/// `self_transfer_pairs` reads.
fn self_transfer_value(call: &eth::Call) -> Option<BigInt> {
    let to_itself = match call.call_type.as_known() {
        Some(eth::CallType::CALLCODE) => true,
        Some(eth::CallType::CALL) => call.caller == call.address,
        _ => false,
    };
    let v = value(call.value.as_option().map(|v| v.bytes.as_slice())).ok()?;
    (to_itself && v != [0; 32]).then(|| BigInt::from_unsigned_bytes_be(&v))
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
            state_sync_call: None,
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
/// previous balance. Polygon's producer of `Block.ver` 3 (to block
/// 73,440,255) records no `SUICIDE_WITHDRAW` at all: an account created and
/// self-destructed in one transaction keeps, in its records, the balance it
/// was created with (block 70,000,124, transaction `0x4b2ad563…`: the CREATE
/// root call credits `0x3e58cebd…` 0.1067 POL, which its constructor
/// self-destructs to `0x91cea390…`; the chain reads zero after the block).
/// Since v0.7.8 the self-destructing call may hold other records, so the
/// shape is: a persisted call (non-reverted, in a succeeded transaction) that
/// self-destructs, holds exactly one `SUICIDE_REFUND`, a positive credit of
/// another account, no `SUICIDE_WITHDRAW` and no record of the destroyed
/// account after the credit, in a transaction without a `SUICIDE_WITHDRAW`
/// of the destroyed account. The destroyed account is the call's execution
/// context: its caller for DELEGATECALL and CALLCODE, its address otherwise.
/// A producer that records the debit in another shape breaks the continuity
/// check instead.
fn selfdestruct_debits(block: &eth::Block) -> Vec<Record> {
    use eth::balance_change::Reason;
    let mut debits = Vec::new();
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        for call in trx.calls.iter().filter(|c| c.suicide && !c.state_reverted) {
            let destroyed = match call.call_type.as_known() {
                Some(eth::CallType::DELEGATE | eth::CallType::CALLCODE) => &call.caller,
                _ => &call.address,
            };
            let is = |c: &eth::BalanceChange, reason: Reason| c.reason.as_known() == Some(reason);
            let refunds: Vec<_> = call.balance_changes.iter().filter(|c| is(c, Reason::SuicideRefund)).collect();
            let [credit] = refunds.as_slice() else {
                continue;
            };
            let withdrawn = trx
                .calls
                .iter()
                .flat_map(|c| &c.balance_changes)
                .any(|c| is(c, Reason::SuicideWithdraw) && c.address == *destroyed);
            if credit.address == *destroyed
                || withdrawn
                || call
                    .balance_changes
                    .iter()
                    .any(|c| is(c, Reason::SuicideWithdraw) || (c.address == *destroyed && c.ordinal > credit.ordinal))
            {
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
                state_sync_call: None,
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

/// A self-destruct moves the destroyed account's balance `d` to its
/// beneficiary. Arbitrum One's producer of `Block.ver` 3 records the
/// account's debit, a `SUICIDE_WITHDRAW` from `d` to zero, and then a second
/// one from `d` to zero although the account is already at zero: the
/// repeat's old value breaks continuity, and the chain reads zero for the
/// account after the block. For an account created in the same transaction,
/// since Cancun (EIP-6780, ArbOS 20) the records are the debit, the
/// beneficiary's `SUICIDE_REFUND` from `b` to `b + d` at the next ordinal (or
/// two ordinals later, when the call creates the beneficiary's account at the
/// ordinal between), a `SUICIDE_WITHDRAW` from zero to zero and the repeat at
/// the two ordinals after the refund. In block 250,005,039 the
/// contract-creation transaction `0xe293fa3a…` creates `0x06ef189a…` with
/// 1,955,820,000,000 wei, which its constructor self-destructs to
/// `0x0f0cb393…` (ordinals 132 to 135); in block 200,029,376 (`0xc1845f0f…`,
/// `0x8bedff29…` to `0x60657119…`) the credit creates the beneficiary's
/// account between the debit and the refund; in block 450,014,091
/// (`0x223ba1a0…`) a nested CREATE deploys to `0x19df9b67…`, which already
/// holds 0.0581 ETH, and its constructor self-destructs it to the sender
/// `0x22face80…`, whose gas refund then starts from the credited balance;
/// in block 450,028,504 (`0xabef75c6…`) the CREATE root call credits
/// `0xc05da4ea…` 15,786,592,425,450 wei, which its constructor
/// self-destructs to `0xd211dbe9…`, credited from zero (ordinals 188 to 191).
/// Before Cancun the records
/// are the refund, the debit at the next ordinal and the repeat at the one
/// after, also for an account not created in the transaction: in block
/// 187,009,069 the contract-creation transaction `0x50dcae1f…` creates
/// `0x88e60026…` with 750,000,000,000,000 wei, which its constructor
/// self-destructs to `0xb244f017…` (ordinals 516 to 518); in block
/// 176,115,455 a call of transaction `0xbfc4af67…` self-destructs
/// `0x9d4eb718…`, which holds 15,785 wei and code before the block, to
/// `0xc881a48a…` (ordinals 37 to 39), and the chain reads zero and no code
/// for it after the block. Sampled self-destructs of a positive balance have
/// no repeat up to block 169,366,089 (block 40,005,993, `0xe921cfdc…`, the
/// refund and the debit of `0x4f14c42f…`), so their records chain, and have
/// it from block 169,454,286.
/// Since Cancun an account not created in the transaction survives its
/// self-destruct, and the records are the debit, the refund at the next
/// ordinal and the repeat at the one after: in block 514,819,388 the
/// transaction `0xae385f2d…` sends 3,495,043,890,571 wei to `0x06bba4e1…`,
/// whose code `0x600035ff` self-destructs to the address in its call data,
/// here `0xf3359221…` (ordinals 172 to 174). The chain reads zero for
/// `0x06bba4e1…` before and after the block, and its code stays. The same
/// sender's calls of it in blocks 514,819,389 to 514,819,395 have this shape.
/// In block 367,502,338 (`0x5ecb2e03…`) a nested call self-destructs
/// `0x6beb920b…`, which holds 5,000,000,000,000 wei and keeps its code, to
/// the sender `0x62005500…`, whose gas refund then starts from the credited
/// balance (ordinals 451 to 453); block 367,510,845 (`0x5c179506…`) repeats
/// it.
///
/// Returns the repeat, as `(account, ordinal)`, so the account's other
/// records must chain across it. Dropping it moves no balance: the account is
/// at zero before and after it. Only these exact shapes are recognized: a
/// persisted call (non-reverted, in a succeeded transaction) that
/// self-destructs an account and holds exactly one `SUICIDE_REFUND`, a credit
/// of another account by `d`, and `SUICIDE_WITHDRAW` records all of the
/// destroyed account: three in the first order, of an account created by a
/// CREATE in the same transaction, two in the second, and two in the third,
/// of an account no CREATE of the transaction creates. These records are
/// consecutive among the call's balance records and in one of the three
/// orders above, with `d` positive. The account's last persisted record
/// before the repeat is the debit. The repeat is the account's only persisted
/// record at its ordinal, since the repeat is dropped by account and ordinal:
/// another record there is not dropped with it and still fails the block. No
/// record of the destroyed account follows the repeat in the transaction
/// (before its end ordinal), as in every capture. The destroyed account is
/// resolved as in `selfdestruct_debits`.
fn stale_selfdestruct_withdraws(block: &eth::Block, records: &[Record]) -> HashSet<(Vec<u8>, u64)> {
    use eth::balance_change::Reason;
    let words = |c: &eth::BalanceChange| {
        Some((
            value(c.old_value.as_option().map(|v| v.bytes.as_slice())).ok()?,
            value(c.new_value.as_option().map(|v| v.bytes.as_slice())).ok()?,
        ))
    };
    let zero = [0; 32];
    let mut stale = HashSet::new();
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        for call in trx.calls.iter().filter(|c| c.suicide && !c.state_reverted) {
            let destroyed = match call.call_type.as_known() {
                Some(eth::CallType::DELEGATE | eth::CallType::CALLCODE) => &call.caller,
                _ => &call.address,
            };
            let is = |c: &eth::BalanceChange, reason: Reason| c.reason.as_known() == Some(reason);
            let count = |reason: Reason| call.balance_changes.iter().filter(|c| is(c, reason)).count();
            let created = trx
                .calls
                .iter()
                .any(|c| c.call_type.as_known() == Some(eth::CallType::CREATE) && c.address == *destroyed);
            if count(Reason::SuicideRefund) != 1 {
                continue;
            }
            let mut own: Vec<&eth::BalanceChange> = call.balance_changes.iter().collect();
            own.sort_by_key(|c| c.ordinal);
            let Some(start) = own.iter().position(|c| is(c, Reason::SuicideWithdraw) || is(c, Reason::SuicideRefund)) else {
                continue;
            };
            // The debit, the refund and the repeat.
            let sequence = match (created, count(Reason::SuicideWithdraw), own.get(start..)) {
                // Since Cancun, of an account created in the transaction: the
                // debit, the refund, a no-op and the repeat.
                (true, 3, Some(&[debit, refund, noop, repeat, ..])) => {
                    let beneficiary_created = call
                        .account_creations
                        .iter()
                        .any(|a| a.account == refund.address && a.ordinal == debit.ordinal + 1);
                    let exact = (refund.ordinal == debit.ordinal + 1 || (refund.ordinal == debit.ordinal + 2 && beneficiary_created))
                        && noop.address == *destroyed
                        && is(noop, Reason::SuicideWithdraw)
                        && noop.ordinal == refund.ordinal + 1
                        && repeat.ordinal == noop.ordinal + 1
                        && words(noop) == Some((zero, zero));
                    exact.then_some((debit, refund, repeat))
                }
                // Before Cancun, of an account created in the transaction or
                // not: the refund, the debit and the repeat.
                (_, 2, Some(&[refund, debit, repeat, ..])) if is(refund, Reason::SuicideRefund) => {
                    (debit.ordinal == refund.ordinal + 1 && repeat.ordinal == debit.ordinal + 1).then_some((debit, refund, repeat))
                }
                // Since Cancun, of an account not created in the transaction,
                // which survives: the debit, the refund and the repeat.
                (false, 2, Some(&[debit, refund, repeat, ..])) => {
                    (refund.ordinal == debit.ordinal + 1 && repeat.ordinal == refund.ordinal + 1).then_some((debit, refund, repeat))
                }
                _ => None,
            };
            let Some((debit, refund, repeat)) = sequence else {
                continue;
            };
            if [debit, repeat].iter().any(|c| c.address != *destroyed || !is(c, Reason::SuicideWithdraw))
                || refund.address == *destroyed
                || !is(refund, Reason::SuicideRefund)
            {
                continue;
            }
            let (Some((d, debit_new)), Some((b, credited)), Some((repeat_old, repeat_new))) = (words(debit), words(refund), words(repeat)) else {
                continue;
            };
            let [moved, before, credited] = [d, b, credited].map(|w| BigInt::from_unsigned_bytes_be(&w));
            if d == zero || debit_new != zero || repeat_old != d || repeat_new != zero || credited != before + moved {
                continue;
            }
            // The account is at zero when the repeat comes: its last
            // persisted record before it is the debit. The repeat is its only
            // persisted record at that ordinal, since the drop goes by
            // account and ordinal, and its last in the transaction.
            let persisted = || records.iter().filter(|r| r.address == *destroyed);
            let last = persisted().filter(|r| r.ordinal < repeat.ordinal).max_by_key(|r| r.ordinal);
            if !last.is_some_and(|r| r.ordinal == debit.ordinal)
                || persisted().filter(|r| r.ordinal == repeat.ordinal).count() != 1
                || persisted().any(|r| r.ordinal > repeat.ordinal && r.ordinal < trx.end_ordinal)
            {
                continue;
            }
            stale.insert((destroyed.clone(), repeat.ordinal));
        }
    }
    stale
}

/// Since Cancun (EIP-6780; on Arbitrum One ArbOS 20, from block 190,301,731)
/// a self-destruct to itself of an account not created in the same
/// transaction moves nothing: geth debits the balance `d`, credits it back,
/// and the account survives with `d`. Arbitrum One's producer of `Block.ver`
/// 3 records a self-destruct of such an account to another beneficiary as
/// the debit, a `SUICIDE_WITHDRAW` from `d` to zero, the beneficiary's
/// `SUICIDE_REFUND` and the repeat, a `SUICIDE_WITHDRAW` from `d` to zero
/// again, at consecutive ordinals (block 514,819,388, transaction
/// `0xae385f2d…`, `0x06bba4e1…` to `0xf3359221…`; block 191,077,275,
/// `0xa32de857…`, `0x7bd535a8…`; `stale_selfdestruct_withdraws`). To
/// itself, that order is the debit, a refund of the account from zero to
/// `d` and the repeat: these records chain and end at zero, while the chain
/// keeps `d`. No continuity check sees that wrong balance, and no such
/// self-destruct is captured, so this shape fails the block. Other
/// producers record a self-destruct to itself in other shapes: Ethereum's
/// `Block.ver` 3 records the refund from zero and then the withdraw (block
/// 19,481,732, `selfdestruct_to_self_pairs`); a producer that records only
/// the debit and the refund ends at `d`.
///
/// Refused: a persisted call (non-reverted, in a succeeded transaction) that
/// self-destructs an account no CREATE of the transaction creates, whose
/// `SUICIDE_WITHDRAW` and `SUICIDE_REFUND` records of that account, no-ops
/// left out, are exactly a withdraw from `d` to zero, a refund from zero to
/// `d` and a withdraw from `d` to zero, in this order by ordinal, `d`
/// positive. Gaps between their ordinals do not matter: the records end at
/// zero all the same. An account created in the transaction is destroyed,
/// so zero is its balance. The destroyed account is resolved as in
/// `selfdestruct_debits`.
fn validate_selfdestructs_to_self(block: &eth::Block) -> Result<(), Error> {
    use eth::balance_change::Reason;
    let words = |c: &eth::BalanceChange| {
        Some((
            value(c.old_value.as_option().map(|v| v.bytes.as_slice())).ok()?,
            value(c.new_value.as_option().map(|v| v.bytes.as_slice())).ok()?,
        ))
    };
    let zero = [0; 32];
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        for call in trx.calls.iter().filter(|c| c.suicide && !c.state_reverted) {
            let destroyed = match call.call_type.as_known() {
                Some(eth::CallType::DELEGATE | eth::CallType::CALLCODE) => &call.caller,
                _ => &call.address,
            };
            let is = |c: &eth::BalanceChange, reason: Reason| c.reason.as_known() == Some(reason);
            // The account's withdraws and refunds with their (old, new) words.
            let mut own: Vec<_> = call
                .balance_changes
                .iter()
                .filter(|c| c.address == *destroyed && (is(c, Reason::SuicideWithdraw) || is(c, Reason::SuicideRefund)))
                .filter_map(|c| words(c).map(|w| (c, w)))
                .filter(|(_, (old, new))| old != new)
                .collect();
            own.sort_by_key(|(c, _)| c.ordinal);
            let [(debit, (d, debit_new)), (refund, (refund_old, credited)), (repeat, (repeat_old, repeat_new))] = own.as_slice() else {
                continue;
            };
            let created = trx
                .calls
                .iter()
                .any(|c| c.call_type.as_known() == Some(eth::CallType::CREATE) && c.address == *destroyed);
            let shape = !created
                && is(debit, Reason::SuicideWithdraw)
                && is(refund, Reason::SuicideRefund)
                && is(repeat, Reason::SuicideWithdraw)
                && debit.ordinal < refund.ordinal
                && refund.ordinal < repeat.ordinal
                && *debit_new == zero
                && *refund_old == zero
                && credited == d
                && repeat_old == d
                && *repeat_new == zero;
            if shape {
                let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
                return Err(Error::msg(format!(
                    "unrecognized self-destruct to itself: 0x{} in transaction 0x{} is recorded as debited, credited and debited \
                     again to zero, but an account not created in its transaction keeps its balance (EIP-6780)",
                    hex(destroyed),
                    hex(&trx.hash)
                )));
            }
        }
    }
    Ok(())
}

/// Since Cancun (EIP-6780) a self-destruct destroys an account created in the
/// same transaction, and its balance `d` moves to the beneficiary. The
/// Ethereum producer of `Block.ver` 5 records the account's `SUICIDE_WITHDRAW`
/// from `d` to zero and, at the next ordinal, the beneficiary's
/// `SUICIDE_REFUND` from `b` to `b + d`. The beneficiary's records during the
/// rest of the execution include the credit, but its records after the
/// execution, the gas refund and a fee credit, leave out every such credit of
/// the transaction, by their sum `c`. The balance after the transaction
/// includes them. Captured: block 25,741,008 (transaction `0xd6299894…`, one
/// credit), 25,747,303 (`0x97a06638…`, the root call of a contract-creation
/// transaction), 25,750,184 (`0x04812d41…`, five credits, each refund chaining
/// from the previous one), 25,869,452 (`0x90f5b5c9…`, four credits) and
/// 25,920,498 (`0x06fc62ea…`, a transfer during the execution starts from the
/// credited balance); in each, an explorer reads the credits in the
/// beneficiary's balance after the block.
///
/// Returns the shift `c` of each of the beneficiary's records after the
/// execution (after the root call's end ordinal, before the transaction's),
/// when the first of them starts exactly `c` below the beneficiary's last
/// balance during the transaction and each moves the balance by the amount
/// the transaction implies: a `GAS_REFUND` by `(gas_limit - gas_used) *
/// gas_price`, a `REWARD_TRANSACTION_FEE` by `gas_used * (gas_price -
/// base_fee)`. Only a record whose old and new values both leave out `c`
/// passes both checks. The continuity check then verifies them. Without the shift that
/// first record breaks continuity, so the shift changes only blocks native
/// v0.7.5 refuses. A credit counts only in this exact shape: a persisted call
/// (non-reverted, in a succeeded transaction) that self-destructs an account
/// created by a CREATE in the same transaction and holds exactly one
/// `SUICIDE_WITHDRAW`, of that account, from `d` to zero, and exactly one
/// `SUICIDE_REFUND`, of another account, from `b` to `b + d` at the next
/// ordinal and during the execution, `d` positive, the refund persisted as
/// recorded. The call's other records are not part of the shape. The
/// destroyed account is resolved as in `selfdestruct_debits`.
fn selfdestruct_credit_shifts(block: &eth::Block, records: &[Record]) -> HashMap<(Vec<u8>, u64), BigInt> {
    let mut shifts: HashMap<(Vec<u8>, u64), BigInt> = HashMap::new();
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        let Some(root) = trx.calls.first().filter(|c| c.depth == 0) else {
            continue;
        };
        let execution_end = root.end_ordinal;
        // The amounts a gas refund and a fee credit of this transaction move.
        let big = |v: Option<&eth::BigInt>| v.map_or_else(|| BigInt::from(0), |v| BigInt::from_unsigned_bytes_be(&v.bytes));
        let gas_price = big(trx.gas_price.as_option());
        let base_fee = big(block.header.as_option().and_then(|h| h.base_fee_per_gas.as_option()));
        let gas_refund = BigInt::from(trx.gas_limit.saturating_sub(trx.gas_used)) * gas_price.clone();
        let fee = BigInt::from(trx.gas_used) * (gas_price - base_fee);
        let mut credits: BTreeMap<Vec<u8>, BigInt> = BTreeMap::new();
        for call in trx.calls.iter().filter(|c| c.suicide && !c.state_reverted) {
            let destroyed = match call.call_type.as_known() {
                Some(eth::CallType::DELEGATE | eth::CallType::CALLCODE) => &call.caller,
                _ => &call.address,
            };
            let of = |reason| call.balance_changes.iter().filter(move |c| c.reason.as_known() == Some(reason));
            let withdraws: Vec<_> = of(eth::balance_change::Reason::SuicideWithdraw).collect();
            let refunds: Vec<_> = of(eth::balance_change::Reason::SuicideRefund).collect();
            let ([withdraw], [refund]) = (withdraws.as_slice(), refunds.as_slice()) else {
                continue;
            };
            let created = trx
                .calls
                .iter()
                .any(|c| c.call_type.as_known() == Some(eth::CallType::CREATE) && c.address == *destroyed);
            if !created
                || withdraw.address != *destroyed
                || refund.address == *destroyed
                || withdraw.ordinal == 0
                || refund.ordinal != withdraw.ordinal + 1
                || refund.ordinal > execution_end
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
            let credit = credits.entry(refund.address.clone()).or_insert_with(|| BigInt::from(0));
            *credit = credit.clone() + moved;
        }
        for (beneficiary, credit) in credits {
            // The beneficiary's persisted records in the transaction, split at
            // the end of the execution.
            let mut mine: Vec<&Record> = records
                .iter()
                .filter(|r| r.address == beneficiary && r.ordinal > trx.begin_ordinal && r.ordinal < trx.end_ordinal)
                .collect();
            mine.sort_by_key(|r| r.ordinal);
            let split = mine.partition_point(|r| r.ordinal <= execution_end);
            let (during, after) = mine.split_at(split);
            let (Some(last), Some(first)) = (during.last(), after.first()) else {
                continue;
            };
            let (Ok(last_new), Ok(first_old)) = (value(last.new_value.as_deref()), value(first.old_value.as_deref())) else {
                continue;
            };
            let stale = BigInt::from_unsigned_bytes_be(&first_old) + credit.clone() == BigInt::from_unsigned_bytes_be(&last_new);
            let as_implied = after.iter().all(|r| {
                let (Ok(old), Ok(new)) = (value(r.old_value.as_deref()), value(r.new_value.as_deref())) else {
                    return false;
                };
                let moved = BigInt::from_unsigned_bytes_be(&new) - BigInt::from_unsigned_bytes_be(&old);
                (r.reason == eth::balance_change::Reason::GasRefund as i32 && moved == gas_refund)
                    || (r.reason == eth::balance_change::Reason::RewardTransactionFee as i32 && moved == fee)
            });
            if !stale || !as_implied {
                continue;
            }
            for r in after {
                shifts.insert((r.address.clone(), r.ordinal), credit.clone());
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

/// A record's rewritten `(old, new)` values, by `(account, ordinal)`.
type Rewrites = HashMap<(Vec<u8>, u64), (Vec<u8>, Vec<u8>)>;

/// The OP-stack BaseFeeVault predeploy, `0x4200…0019`.
const OP_BASE_FEE_VAULT: [u8; 20] = [0x42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x19];
/// The OP-stack L1FeeVault predeploy, `0x4200…001a`.
const OP_L1_FEE_VAULT: [u8; 20] = [0x42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x1a];

/// An OP-stack transaction pays its fee after the execution, in the root
/// call: after the sender's `GAS_REFUND`, `REWARD_TRANSACTION_FEE` credits of
/// the coinbase, the SequencerFeeVault `0x4200…0011` (the tip), of the
/// BaseFeeVault `0x4200…0019` (`gas_used * base_fee`) and of the L1FeeVault
/// `0x4200…001a` (the L1 data fee, which the sender's `GAS_BUY` debit holds on
/// top of `gas_limit * gas_price`). A vault's `withdraw()` sends its whole
/// balance away, so a transaction that withdraws a vault debits it to zero
/// during the execution. Base's producer of `Block.ver` 5 (from block
/// 45,904,000) records the BaseFeeVault's and the L1FeeVault's credit after
/// such a withdrawal from the vault's balance before the transaction instead
/// of from zero, while the coinbase's credit starts from zero. In block
/// 50,024,530 transaction `0xbae6554e…` withdraws `0x4200…0011` and
/// `0x4200…0019`, and the BaseFeeVault's credit reads 43,588,704,197,761,546,823
/// to 43,588,705,214,861,546,823 wei; in block 50,326,929 transaction
/// `0x39a5e05e…` withdraws all three vaults, and the next transaction's
/// credits of `0x4200…0019` and `0x4200…001a` start from the two fees alone
/// (1,108,685,000,000 and 747,081,578 wei). The credit breaks continuity, and
/// taken as recorded it would give back to the vault what was withdrawn.
/// Every withdrawal of either vault on Base from block 45,904,000 to
/// 52,614,000 (33 blocks) has this shape, among them withdrawals of the
/// L1FeeVault alone (block 47,424,576, `0x4cfe034c…`) and of the BaseFeeVault
/// alone (block 49,240,668, `0x997c4d24…`). The OperatorFeeVault `0x4200…001b`
/// is empty on Base, Optimism and Unichain (2026-10-14) and has no captured
/// credit, so it is not part of the shape. Base's producer of `Block.ver` 4
/// (withdrawals in blocks 15,248,533, 25,227,739, 35,206,936, 44,278,936 and
/// 45,174,097) and the Optimism (block 155,196,870, `0x316c6c1f…`) and
/// Unichain (block 61,187,535, `0x2995bdc3…`) producers of `Block.ver` 5
/// record the credit from zero.
///
/// Returns, by `(vault, ordinal)`, the old and new values that such a credit
/// stands for: from the vault's last balance during the execution, zero, to
/// the fee. The continuity check then verifies the vault's next record
/// against the fee. Without the rewrite the credit breaks continuity, so the
/// rewrite changes only blocks native refuses without this rule (v0.7.7 and
/// earlier). Only this exact shape is recognized, for these two vaults: in a
/// succeeded transaction, the vault's persisted records up to the root call's
/// end ordinal are `TRANSFER`s during the execution, the last of them to
/// zero, and its only record after the execution is a
/// `REWARD_TRANSACTION_FEE` from `b`, the old value of its first record in
/// the transaction, positive, to `b` plus the fee: `gas_used * base_fee` for
/// the BaseFeeVault and, for the L1FeeVault, the debit of the root call's
/// only `GAS_BUY` less `gas_limit * gas_price`, each positive. A credit that
/// already starts from zero chains and is not rewritten. The two vaults'
/// records are indexed in one pass, and only a transaction in which a vault
/// has a `TRANSFER` record during the execution is examined, so a block
/// without one costs that pass alone.
fn fee_vault_credit_rewrites(block: &eth::Block, records: &[Record]) -> Rewrites {
    use eth::balance_change::Reason;
    let big = |v: Option<&eth::BigInt>| v.map_or_else(|| BigInt::from(0), |v| BigInt::from_unsigned_bytes_be(&v.bytes));
    let words = |old: Option<&[u8]>, new: Option<&[u8]>| -> Option<(BigInt, BigInt)> {
        let (old, new) = (value(old).ok()?, value(new).ok()?);
        Some((BigInt::from_unsigned_bytes_be(&old), BigInt::from_unsigned_bytes_be(&new)))
    };
    let mut rewrites = HashMap::new();
    // Each vault's persisted records by ordinal (a stable sort keeps array
    // order among equal ordinals), and the ordinals of their TRANSFERs.
    let mut vaults: [(_, Vec<&Record>); 2] = [(OP_BASE_FEE_VAULT, Vec::new()), (OP_L1_FEE_VAULT, Vec::new())];
    for record in records {
        if let Some((_, mine)) = vaults.iter_mut().find(|(vault, _)| record.address == *vault) {
            mine.push(record);
        }
    }
    for (_, mine) in vaults.iter_mut() {
        mine.sort_by_key(|r| r.ordinal);
    }
    let mut transfers: Vec<u64> = vaults
        .iter()
        .flat_map(|(_, mine)| mine.iter().filter(|r| r.reason == Reason::Transfer as i32).map(|r| r.ordinal))
        .collect();
    if transfers.is_empty() {
        return rewrites;
    }
    transfers.sort_unstable();
    let base_fee = big(block.header.as_option().and_then(|h| h.base_fee_per_gas.as_option()));
    let zero = BigInt::from(0);
    for trx in block.transaction_traces.iter().filter(|t| !persist::is_failed(t)) {
        let Some(root) = trx.calls.first().filter(|c| c.depth == 0) else {
            continue;
        };
        // Only with a vault's TRANSFER during the execution: after the
        // transaction's and the root call's begin ordinals, before the
        // transaction's end ordinal and up to the root call's.
        let (lo, hi) = (
            trx.begin_ordinal.max(root.begin_ordinal),
            trx.end_ordinal.saturating_sub(1).min(root.end_ordinal),
        );
        if !transfers.get(transfers.partition_point(|&o| o <= lo)).is_some_and(|&o| o <= hi) {
            continue;
        }
        // The L1 fee: what the gas purchase debits beyond the gas.
        let buys: Vec<_> = root.balance_changes.iter().filter(|c| c.reason.as_known() == Some(Reason::GasBuy)).collect();
        let l1_fee = match buys.as_slice() {
            [buy] => words(
                buy.old_value.as_option().map(|v| v.bytes.as_slice()),
                buy.new_value.as_option().map(|v| v.bytes.as_slice()),
            )
            .map(|(old, new)| old - new - BigInt::from(trx.gas_limit) * big(trx.gas_price.as_option())),
            _ => None,
        };
        let base = Some(BigInt::from(trx.gas_used) * base_fee.clone());
        for ((vault, mine), fee) in vaults.iter().zip([base, l1_fee]) {
            let Some(fee) = fee.filter(|fee| *fee > zero) else {
                continue;
            };
            // The vault's persisted records in the transaction, split at the
            // end of the execution.
            let start = mine.partition_point(|r| r.ordinal <= trx.begin_ordinal);
            let end = mine.partition_point(|r| r.ordinal < trx.end_ordinal).max(start);
            let mine = &mine[start..end];
            let split = mine.partition_point(|r| r.ordinal <= root.end_ordinal);
            let (during, after) = mine.split_at(split);
            let (Some(first), Some(last), [credit]) = (during.first(), during.last(), after) else {
                continue;
            };
            let withdrawn = first.ordinal > root.begin_ordinal && during.iter().all(|r| r.reason == Reason::Transfer as i32);
            let (Some((before, _)), Some((_, left)), Some((old, new))) = (
                words(first.old_value.as_deref(), first.new_value.as_deref()),
                words(last.old_value.as_deref(), last.new_value.as_deref()),
                words(credit.old_value.as_deref(), credit.new_value.as_deref()),
            ) else {
                continue;
            };
            if !withdrawn || !left.is_zero() || credit.reason != Reason::RewardTransactionFee as i32 || before.is_zero() || old != before || new - old != fee {
                continue;
            }
            // From the vault's balance after the withdrawal.
            let (Ok(from), Ok(to)) = (word(&left.to_bytes_be().1), word(&(left.clone() + fee).to_bytes_be().1)) else {
                continue;
            };
            rewrites.insert((vault.to_vec(), credit.ordinal), (from.to_vec(), to.to_vec()));
        }
    }
    rewrites
}

/// Bor's system address, `0xffff…fffe` like BSC's, which calls the
/// StateReceiver and the ValidatorSet.
const BOR_SYSTEM_ADDRESS: [u8; 20] = BSC_FEE_POOL;
/// Bor's StateReceiver system contract, `0x…1001`.
const BOR_STATE_RECEIVER: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10, 0x01];
/// Bor's ValidatorSet system contract, `0x…1000`.
const BOR_VALIDATOR_SET: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10, 0x00];
/// The selector of the ValidatorSet's
/// `commitSpan(uint256,uint256,uint256,bytes,bytes)`.
const BOR_COMMIT_SPAN: [u8; 4] = [0x23, 0xc2, 0xa2, 0xb4];

/// The depth-1 calls of a Bor state-sync transaction (`bor_state_sync`).
struct StateSync {
    tx_index: u32,
    begin_ordinal: u64,
    /// The position of each call's depth-1 call, by call index.
    positions: HashMap<u32, u32>,
}

/// Polygon's Bor client commits the state-sync events of a sprint, messages
/// from Ethereum such as bridge deposits, at the end of a block whose number is
/// a multiple of the sprint length, in a synthetic transaction. Its producer
/// of `Block.ver` 3 (to block 73,440,255) records it as a succeeded
/// transaction from and to the zero address, with no gas used, whose root call,
/// a CALL from and to the zero address with begin ordinal 0, holds one depth-1
/// call per event: a CALL from the system address `0xffff…fffe` to the
/// StateReceiver `0x…1001` (`commitState`), also with begin ordinal 0. In
/// blocks 10,076,416 to 70,044,000 the subtree of each depth-1 call restarts
/// the ordinals just after the transaction's begin ordinal, so the subtrees'
/// ordinals overlap: each subtree's ordinals follow its execution, and the
/// subtrees execute in call order. In block 70,044,000, transaction
/// `0x60891cc1…` (transaction 46) commits four events, and WPOL `0x0d500b1d…`
/// and `0x884d1aa1…` have records in three of the subtrees at ordinals 3,408 to
/// 3,422: WPOL at 3,409, 3,408 and 3,408 in call order, which is the order
/// their balances chain in. In block 20,007,040, transaction `0xa44f1b27…`
/// commits ten events, and two of them, POL deposits to `0x22df4f7e…` and
/// `0x7becdea5…`, debit the MRC20 contract `0x…1010` at ordinal 13 each. By
/// ordinal alone such blocks fail as ambiguous or discontinuous. By block
/// 73,439,280 the same producer numbers the subtrees in sequence (block
/// 73,439,440, transaction 69 `0x4a023a3b…`), which this rule orders the same
/// as the ordinals do. The producer of `Block.ver` 4 gives the root and
/// depth-1 calls their begin ordinals and does not restart them (block
/// 80,000,272, transaction `0x5b6ce37b…`), which this shape does not match.
///
/// At the first block of a span's last sprint Bor also commits the next span
/// (`commitSpan`). The producer of `Block.ver` 3 records that as a separate
/// transaction (`is_bor_span_commit`), the block's last, right after the
/// state-sync transaction and with the ordinals after its own, in each of the
/// eleven such blocks fetched from 3,206,592 to 73,120,240. In block
/// 20,025,792 the state-sync transaction `0xa0991514…` (transaction 119,
/// ordinals 11,956 to 12,153) carries two POL deposits that debit the MRC20
/// contract at ordinal 11,969 each, and transaction 120, `0xb39c139e…`
/// (ordinals 12,154 to 12,633), commits span 3,130. Block 70,041,840 commits
/// span 10,945 in its last transaction, `0x95d9f755…`, without a state sync.
///
/// Returns the transaction's depth-1 calls when the shape is exact: the block's
/// last transaction, or the one before a last span commit; succeeded, from and
/// to the zero address, with no gas used and a positive begin ordinal; its
/// index is no other transaction's; every transaction before it ends before its
/// begin ordinal, and the span commit begins after its end ordinal; its first
/// call is the root call above, without value or balance records, not reverted
/// and not self-destructing; every other call, in increasing call index, is
/// either such a depth-1 call of the root call or one level deeper than its
/// parent in the subtree of the latest depth-1 call, so that each subtree is a
/// contiguous run of call indices; a deeper call begins after the call before
/// it in its subtree and holds its balance records between its begin and end
/// ordinals; no call self-destructs or moves value to itself (the
/// self-destruct rules and `self_transfer_pairs` key records by account and
/// ordinal alone); every balance record of the transaction lies strictly
/// between its begin and end ordinals, and every other balance record of the
/// block outside them. An end ordinal after the begin ordinal and at least one
/// depth-1 call are implied by the rest whenever the transaction has a balance
/// record to place, and checked only as a guard. Its records are then ordered
/// by depth-1 call and ordinal (`changes`), and the continuity check verifies
/// them. Any other shape keeps the ordinal order, unless its subtrees' records
/// do not follow call order (`unrecognized_bor_state_sync`).
fn bor_state_sync(block: &eth::Block) -> Option<StateSync> {
    let (trx, earlier, span) = match block.transaction_traces.as_slice() {
        [earlier @ .., trx, span] if is_bor_span_commit(span) => (trx, earlier, Some(span)),
        [earlier @ .., trx] => (trx, earlier, None),
        [] => return None,
    };
    let (begin, end) = (trx.begin_ordinal, trx.end_ordinal);
    let zero = [0; 20];
    let no_value = |c: &eth::Call| value(c.value.as_option().map(|v| v.bytes.as_slice())).is_ok_and(|v| v == [0; 32]);
    let is_call = |c: &eth::Call| c.call_type.as_known() == Some(eth::CallType::CALL);
    let others = || earlier.iter().chain(span);
    if trx.from != zero
        || trx.to != zero
        || persist::is_failed(trx)
        || trx.gas_used != 0
        || begin == 0
        || end <= begin
        || others().any(|t| t.index == trx.index)
        || earlier.iter().any(|t| t.end_ordinal >= begin)
        || span.is_some_and(|t| t.begin_ordinal <= end)
    {
        return None;
    }
    let (root, calls) = trx.calls.split_first()?;
    if root.depth != 0
        || !is_call(root)
        || root.caller != zero
        || root.address != zero
        || root.begin_ordinal != 0
        || root.state_reverted
        || root.suicide
        || !no_value(root)
        || !root.balance_changes.is_empty()
    {
        return None;
    }
    // Each call's (depth-1 call position, depth), by call index.
    let mut placed: HashMap<u32, (u32, u32)> = HashMap::new();
    let (mut events, mut last_index, mut last_begin) = (0, root.index, 0);
    for call in calls {
        if call.index <= last_index || call.suicide || self_transfer_value(call).is_some() {
            return None;
        }
        let position = if call.depth == 1 {
            let commit = call.parent_index == root.index
                && is_call(call)
                && call.caller == BOR_SYSTEM_ADDRESS
                && call.address == BOR_STATE_RECEIVER
                && call.begin_ordinal == 0;
            if !commit {
                return None;
            }
            events += 1;
            events - 1
        } else {
            let within = |c: &eth::BalanceChange| c.ordinal >= call.begin_ordinal && c.ordinal <= call.end_ordinal;
            match placed.get(&call.parent_index) {
                Some(&(position, depth))
                    if call.depth == depth + 1 && position + 1 == events && call.begin_ordinal > last_begin && call.balance_changes.iter().all(within) =>
                {
                    position
                }
                _ => return None,
            }
        };
        if call.balance_changes.iter().any(|c| c.ordinal <= begin || c.ordinal >= end) {
            return None;
        }
        (last_index, last_begin) = (call.index, call.begin_ordinal);
        placed.insert(call.index, (position, call.depth));
    }
    let mut others = block
        .balance_changes
        .iter()
        .chain(block.system_calls.iter().flat_map(|c| &c.balance_changes))
        .chain(others().flat_map(|t| &t.calls).flat_map(|c| &c.balance_changes));
    if events == 0 || others.any(|c| c.ordinal >= begin && c.ordinal <= end) {
        return None;
    }
    Some(StateSync {
        tx_index: trx.index,
        begin_ordinal: begin,
        positions: placed.into_iter().map(|(index, (position, _))| (index, position)).collect(),
    })
}

/// Whether a transaction is Bor's span commit as the producer of `Block.ver` 3
/// records it (`bor_state_sync`): succeeded, from the system address to the
/// ValidatorSet, with one call, a CALL from the system address to the
/// ValidatorSet with begin ordinal 0 and the selector of `commitSpan`, not
/// reverted and without balance records.
fn is_bor_span_commit(trx: &eth::TransactionTrace) -> bool {
    let [call] = trx.calls.as_slice() else {
        return false;
    };
    !persist::is_failed(trx)
        && trx.from == BOR_SYSTEM_ADDRESS
        && trx.to == BOR_VALIDATOR_SET
        && call.depth == 0
        && call.call_type.as_known() == Some(eth::CallType::CALL)
        && call.caller == BOR_SYSTEM_ADDRESS
        && call.address == BOR_VALIDATOR_SET
        && call.begin_ordinal == 0
        && call.input.starts_with(&BOR_COMMIT_SPAN)
        && !call.state_reverted
        && call.balance_changes.is_empty()
}

/// Whether the block has a transaction, other than the one `bor_state_sync`
/// recognized, with the signature of a Bor state-sync transaction of the
/// producer of `Block.ver` 3 whose depth-1 subtrees' balance records do not
/// follow one another in call order: a subtree's lowest record ordinal is at
/// most the highest of the subtree with records before it, so their ordinals
/// overlap or run backwards. The signature is a transaction from and to the
/// zero address whose root call is a CALL from and to the zero address with
/// begin ordinal 0, with a depth-1 call by the system address with begin
/// ordinal 0; a deeper call counts in the subtree of the latest depth-1 call
/// before it. The ordinals of such records are not their execution order, and
/// they could still chain in ordinal order by accident, so the block fails
/// (`records`) rather than keep the ordinal order. None of the 85 Polygon
/// blocks checked from 3,206,592 to 80,000,464 has such a transaction.
fn unrecognized_bor_state_sync(block: &eth::Block, recognized: Option<&StateSync>) -> bool {
    let zero = [0; 20];
    let mut others = block.transaction_traces.iter().filter(|t| recognized.is_none_or(|s| s.tx_index != t.index));
    others.any(|trx| {
        let Some((root, calls)) = trx.calls.split_first() else {
            return false;
        };
        let signature = trx.from == zero
            && trx.to == zero
            && root.call_type.as_known() == Some(eth::CallType::CALL)
            && root.caller == zero
            && root.address == zero
            && root.begin_ordinal == 0
            && calls.iter().any(|c| c.depth == 1 && c.caller == BOR_SYSTEM_ADDRESS && c.begin_ordinal == 0);
        if !signature {
            return false;
        }
        // Each depth-1 subtree's lowest and highest record ordinal, in call
        // order; empty for a subtree without records.
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        for call in calls {
            if call.depth == 1 {
                ranges.push((u64::MAX, 0));
            }
            if let Some((low, high)) = ranges.last_mut() {
                for c in &call.balance_changes {
                    (*low, *high) = ((*low).min(c.ordinal), (*high).max(c.ordinal));
                }
            }
        }
        ranges.retain(|(low, high)| low <= high);
        ranges.windows(2).any(|w| w[1].0 <= w[0].1)
    })
}

/// A record's place in execution order (`execution_order`).
type Order = (u64, u64, u64);

/// A record's place in execution order: its ordinal, except that a record of
/// a Bor state-sync transaction (`bor_state_sync`) is placed at the
/// transaction's begin ordinal, then by its depth-1 call and ordinal. No other
/// record lies within that transaction's ordinals.
fn execution_order(record: &Record) -> Order {
    match record.state_sync_call {
        Some((begin, position)) => (begin, u64::from(position) + 1, record.ordinal),
        None => (record.ordinal, 0, 0),
    }
}

/// Reduce persisted records in execution order into one row per account. Every
/// record needs a canonical 20-byte account, a positive ordinal, values that
/// fit `uint256`, a strictly increasing execution order per account and old/new
/// continuity. Execution order is the ordinal, except in a Bor state-sync
/// transaction (`bor_state_sync`), where it is the depth-1 call, then the
/// ordinal (`execution_order`). Ambiguous or discontinuous producer data fails
/// the block, as does a failed transaction's record outside its execution that
/// no rule persists (`validate_failed_records_outside_execution`). Seven
/// producer shapes are exceptions. The self-transfer pair
/// (`self_transfer_pairs`), the EIP-6780 self-destruct to itself
/// (`selfdestruct_to_self_pairs`) and Arbitrum's repeated withdraw of a
/// self-destructed account
/// (`stale_selfdestruct_withdraws`) are dropped, so the account's other
/// records must chain across them. The beneficiary's records after the
/// execution of a transaction with EIP-6780 self-destruct credits from
/// accounts created in it (`selfdestruct_credit_shifts`) are shifted by the
/// credits' sum, so they must chain from the balance during the execution.
/// The fee pool's system-reward payout (`fee_pool_payouts`) is
/// restored from the `n -> n` record the shared rules drop, so the pool must
/// chain through it. The debit of an account destroyed by an EIP-6780
/// self-destruct (`selfdestruct_debits`) is restored from its beneficiary's
/// credit, so the account must chain through it. An OP-stack fee vault's fee
/// credit after the execution of a transaction that withdrew the vault
/// (`fee_vault_credit_rewrites`) is rewritten to start from the withdrawn
/// vault's zero, so the vault's next record must chain from the fee. A
/// self-destruct to itself whose records chain to zero although the account
/// keeps its balance fails the block (`validate_selfdestructs_to_self`).
pub fn changes(block: &eth::Block) -> Result<Vec<Change>, Error> {
    let mut records = records(block)?;
    // Returned after the reduction, so a dropped record that breaks continuity
    // keeps that error.
    let outside_execution = validate_failed_records_outside_execution(block, &records);
    validate_selfdestructs_to_self(block)?;
    let mut pairs = self_transfer_pairs(block);
    pairs.extend(selfdestruct_to_self_pairs(block));
    pairs.extend(stale_selfdestruct_withdraws(block, &records));
    records.retain(|r| !(r.scope == persist::Scope::Tx && pairs.contains(&(r.address.clone(), r.ordinal))));
    records.extend(fee_pool_payouts(block));
    records.extend(selfdestruct_debits(block));
    // After the restored records, so the beneficiary's last balance during the
    // execution is the one the continuity check sees.
    let shifts = selfdestruct_credit_shifts(block, &records);
    for record in records.iter_mut() {
        if let Some(shift) = shifts.get(&(record.address.clone(), record.ordinal)) {
            record.old_value = Some(shifted(record.old_value.as_deref(), shift)?);
            record.new_value = Some(shifted(record.new_value.as_deref(), shift)?);
        }
    }
    let rewrites = fee_vault_credit_rewrites(block, &records);
    for record in records.iter_mut() {
        if let Some((old, new)) = rewrites.get(&(record.address.clone(), record.ordinal)) {
            record.old_value = Some(old.clone());
            record.new_value = Some(new.clone());
        }
    }
    // Array order is not execution order: block-level records follow the
    // transactions in the message but can precede or follow them by ordinal.
    records.sort_by_key(execution_order);
    let mut rows: BTreeMap<Vec<u8>, (Change, [u8; 32], Order)> = BTreeMap::new();
    for record in records {
        require(record.address.len() == 20, "invalid native account address")?;
        require(record.ordinal > 0, "persisted native balance change has no execution ordinal")?;
        let old = value(record.old_value.as_deref())?;
        let new = value(record.new_value.as_deref())?;
        let at = execution_order(&record);
        match rows.get_mut(&record.address) {
            Some((row, last, last_at)) => {
                require(at > *last_at, "ambiguous native balance execution order")?;
                require(old == *last, "discontinuous native balance changes within block")?;
                row.amount = amount(&new);
                row.ordinal = record.ordinal;
                row.records += 1;
                *last = new;
                *last_at = at;
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
                        at,
                    ),
                );
            }
        }
    }
    outside_execution?;
    Ok(rows.into_values().map(|(row, _, _)| row).collect())
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
