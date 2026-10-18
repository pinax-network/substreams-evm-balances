//! Execution order in Polygon Bor state-sync transactions, whose depth-1
//! calls restart the ordinals (`Block.ver` 3, to block 73,440,255).
//!
//! Bor commits a sprint's state-sync events (bridge deposits from Ethereum)
//! in a synthetic transaction at the end of the block: from and to the zero
//! address, with no gas used, whose root call (a CALL from and to the zero
//! address with begin ordinal 0) holds one depth-1 `commitState` call per
//! event, a CALL from the system address `0xffff…fffe` to the StateReceiver
//! `0x…1001`, also with begin ordinal 0. The producer of `Block.ver` 3
//! restarts the ordinals of each depth-1 subtree just after the transaction's
//! begin ordinal, so the subtrees' ordinals overlap, while each subtree's
//! ordinals follow its execution and the subtrees execute in call order. The
//! records of one storage key written in two or more subtrees, such as a
//! bridged token's total supply or a WPOL pool's balance, then tie or run
//! backwards by ordinal. The shape and its ordering are those of native
//! v0.7.8 (`native/balances/src/lib.rs`, `bor_state_sync`), re-derived over
//! storage records as well as balance records: a bridge mint usually moves no
//! native balance.
//!
//! On blocks 20,007,040, 50,016,032 and 70,044,000 (2026-10-17), every one of
//! the 75 keys the state-sync transactions write chains in depth-1 call order,
//! the 34 written in two or more subtrees included, and none of those 34
//! chains by ordinal; the 18 `balanceOf` calls inside them return the word
//! that call order gives. The log ordinals of such a transaction lie after its
//! end ordinal and are never used.
use super::{value, CALL, SUCCEEDED, ZERO};
use crate::eth;
use std::collections::{BTreeMap, BTreeSet};

/// Bor's system address, which calls the StateReceiver and the ValidatorSet.
const SYSTEM: [u8; 20] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe,
];
/// Bor's StateReceiver system contract, `0x…1001`.
const STATE_RECEIVER: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10, 0x01];
/// Bor's ValidatorSet system contract, `0x…1000`.
const VALIDATOR_SET: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10, 0x00];
/// The ValidatorSet's `commitSpan(uint256,uint256,uint256,bytes,bytes)`.
const COMMIT_SPAN: [u8; 4] = [0x23, 0xc2, 0xa2, 0xb4];

/// A record's place in execution order: `(ordinal, 0, 0)`, or in a
/// recognized state-sync transaction `(transaction begin ordinal, depth-1
/// call position + 1, ordinal)`. No record of another transaction lies within
/// that transaction's ordinals, so the two kinds compare consistently.
pub(super) type Seq = (u64, u64, u64);

/// The depth-1 calls of a recognized state-sync transaction ([`state_sync`]).
pub(super) struct StateSync {
    tx_index: u32,
    begin: u64,
    /// The position of each call's depth-1 call, by call index.
    positions: BTreeMap<u32, u32>,
}

/// The execution order of a record at `ordinal` of call `call` in transaction
/// `tx` (`None` for a system call or a block-level record).
pub(super) fn seq(sync: Option<&StateSync>, tx: Option<u32>, call: u32, ordinal: u64) -> Seq {
    let position = sync.filter(|s| tx == Some(s.tx_index)).and_then(|s| Some((s.begin, *s.positions.get(&call)?)));
    match position {
        Some((begin, position)) => (begin, u64::from(position) + 1, ordinal),
        None => (ordinal, 0, 0),
    }
}

/// The ordinals of a call's storage and balance records.
fn records(call: &eth::Call) -> impl Iterator<Item = u64> + '_ {
    let storage = call.storage_changes.iter().map(|c| c.ordinal);
    storage.chain(call.balance_changes.iter().map(|c| c.ordinal))
}

/// The block's state-sync transaction, when its shape is exact: the block's
/// last transaction, or the one before a last span commit
/// ([`is_span_commit`]); succeeded, from and to the zero address, with no gas
/// used and a positive begin ordinal before its end ordinal; its index is no
/// other transaction's; every transaction before it ends before its begin
/// ordinal, and the span commit begins after its end ordinal. Its first call
/// is the root call above, without value or records, not reverted and not
/// self-destructing. Every other call, in increasing call index, does not
/// self-destruct and is either such a depth-1 call of the root call, whose
/// records lie between the transaction's begin ordinal and its own end
/// ordinal, or one level deeper than its parent in the subtree of the latest
/// depth-1 call, so that each subtree is a contiguous run of call indices,
/// beginning after the call before it in its subtree and holding its records
/// between its begin and end ordinals. There is at least one depth-1 call.
/// Every storage and balance record of the transaction lies strictly between
/// its begin and end ordinals, and every other storage and balance record of
/// the block (other transactions, reverted calls included, system calls and
/// block-level records) outside them.
pub(super) fn state_sync(block: &eth::Block) -> Option<StateSync> {
    let (trx, earlier, span) = match block.transaction_traces.as_slice() {
        [earlier @ .., trx, span] if is_span_commit(span) => (trx, earlier, Some(span)),
        [earlier @ .., trx] => (trx, earlier, None),
        [] => return None,
    };
    let (begin, end) = (trx.begin_ordinal, trx.end_ordinal);
    let others = || earlier.iter().chain(span);
    if trx.from != ZERO
        || trx.to != ZERO
        || trx.status != SUCCEEDED
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
        || root.call_type != CALL
        || root.caller != ZERO
        || root.address != ZERO
        || root.begin_ordinal != 0
        || root.state_reverted
        || root.suicide
        || value(root) != Some([0; 32])
        || records(root).next().is_some()
    {
        return None;
    }
    // Each call's (depth-1 call position, depth), by call index.
    let mut placed = BTreeMap::<u32, (u32, u32)>::new();
    let (mut events, mut last_index, mut last_begin) = (0, root.index, 0);
    for call in calls {
        if call.index <= last_index || call.suicide {
            return None;
        }
        let position = if call.depth == 1 {
            let commit = call.parent_index == root.index
                && call.call_type == CALL
                && call.caller == SYSTEM
                && call.address == STATE_RECEIVER
                && call.begin_ordinal == 0
                && records(call).all(|o| o <= call.end_ordinal);
            if !commit {
                return None;
            }
            events += 1;
            events - 1
        } else {
            let within = |o: u64| o >= call.begin_ordinal && o <= call.end_ordinal;
            match placed.get(&call.parent_index) {
                Some(&(position, depth))
                    if call.depth == depth + 1 && position + 1 == events && call.begin_ordinal > last_begin && records(call).all(within) =>
                {
                    position
                }
                _ => return None,
            }
        };
        if records(call).any(|o| o <= begin || o >= end) {
            return None;
        }
        (last_index, last_begin) = (call.index, call.begin_ordinal);
        placed.insert(call.index, (position, call.depth));
    }
    let block_records = block.balance_changes.iter().map(|c| c.ordinal);
    let mut foreign = block_records
        .chain(block.system_calls.iter().flat_map(records))
        .chain(others().flat_map(|t| &t.calls).flat_map(records));
    if events == 0 || foreign.any(|o| o >= begin && o <= end) {
        return None;
    }
    Some(StateSync {
        tx_index: trx.index,
        begin,
        positions: placed.into_iter().map(|(index, (position, _))| (index, position)).collect(),
    })
}

/// Bor's span commit as the producer of `Block.ver` 3 records it, right after
/// the state-sync transaction: succeeded, from the system address to the
/// ValidatorSet, with one call, a CALL from the system address to the
/// ValidatorSet with begin ordinal 0 and the selector of `commitSpan`, not
/// reverted and without balance records.
fn is_span_commit(trx: &eth::TransactionTrace) -> bool {
    let [call] = trx.calls.as_slice() else {
        return false;
    };
    trx.status == SUCCEEDED
        && trx.from == SYSTEM
        && trx.to == VALIDATOR_SET
        && call.depth == 0
        && call.call_type == CALL
        && call.caller == SYSTEM
        && call.address == VALIDATOR_SET
        && call.begin_ordinal == 0
        && call.input.starts_with(&COMMIT_SPAN)
        && !call.state_reverted
        && call.balance_changes.is_empty()
}

/// The transactions, other than a recognized one, with the signature of a
/// state-sync transaction of the producer of `Block.ver` 3: from and to the
/// zero address, whose root call is a CALL from and to the zero address with
/// begin ordinal 0, with a depth-1 call by the system address with begin
/// ordinal 0. Their ordinals need not be their execution order, and records
/// of different subtrees could still chain by ordinal by accident, so every
/// contract they write or call `balanceOf` of is doubted (native fails the
/// block instead).
pub(super) fn unordered(block: &eth::Block, recognized: Option<&StateSync>) -> BTreeSet<u32> {
    let signature = |trx: &eth::TransactionTrace| {
        let Some((root, calls)) = trx.calls.split_first() else {
            return false;
        };
        trx.from == ZERO
            && trx.to == ZERO
            && root.call_type == CALL
            && root.caller == ZERO
            && root.address == ZERO
            && root.begin_ordinal == 0
            && calls.iter().any(|c| c.depth == 1 && c.caller == SYSTEM && c.begin_ordinal == 0)
    };
    let others = block.transaction_traces.iter().filter(|t| recognized.is_none_or(|s| s.tx_index != t.index));
    others.filter(|t| signature(t)).map(|t| t.index).collect()
}
