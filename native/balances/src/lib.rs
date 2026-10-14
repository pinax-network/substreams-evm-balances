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
//! system calls, block-level records).
use evm_persist as persist;
pub mod db_out;

use proto::pb::evm::balances::v1 as balances_pb;
use std::collections::{BTreeMap, HashSet};
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
/// transaction is pinned by the shared persistence rules: the three gas
/// reasons persist, the others revert with the execution frame. Any other
/// reason in a failed transaction's root call (for example BNB blob-fee
/// rewards, OP-stack deposit mints, or a reason this crate cannot name) has no
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
            let pinned = persist::is_gas_reason(change) || change.reason.as_known().is_some_and(|r| REVERTIBLE_FAILED_REASONS.contains(&r));
            require(
                pinned,
                "failed transaction carries a balance-change reason without pinned persistence semantics",
            )?;
        }
    }
    Ok(())
}

/// Every persisted native balance record in the block, in array order, with
/// its persistence scope, retained for package tests and offline inspection.
pub fn records(block: &eth::Block) -> Result<Vec<Record>, Error> {
    validate_block(block)?;
    validate_failed_transaction_reasons(block)?;
    let mut collected = Collected::default();
    persist::collect_block(block, &mut collected)?;
    Ok(collected.records)
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

/// Reduce persisted records by execution ordinal into one row per account.
/// Every record needs a canonical 20-byte account, a positive ordinal, values
/// that fit `uint256`, a strictly increasing ordinal per account and old/new
/// continuity. Ambiguous or discontinuous producer data fails the block. Two
/// producer shapes are exceptions. The self-transfer pair
/// (`self_transfer_pairs`) is dropped, so the account's other records must
/// chain across it. The fee pool's system-reward payout (`fee_pool_payouts`)
/// is restored from the `n -> n` record the shared rules drop, so the pool
/// must chain through it.
pub fn changes(block: &eth::Block) -> Result<Vec<Change>, Error> {
    let mut records = records(block)?;
    let pairs = self_transfer_pairs(block);
    records.retain(|r| !(r.scope == persist::Scope::Tx && pairs.contains(&(r.address.clone(), r.ordinal))));
    records.extend(fee_pool_payouts(block));
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

// The SDK macro generates raw-pointer parameter decoding and discards function
// attributes. Keep its ABI-specific lint exception scoped to this wrapper. As in
// the other packages, the export exists only in the WASM build so host crates
// can link this crate next to other packages.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;
    #[substreams::handlers::map]
    fn map_events(block: eth::Block) -> Result<balances_pb::NativeEvents, Error> {
        project(&block)
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
