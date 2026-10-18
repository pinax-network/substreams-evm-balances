//! The one correction of a block that the embedded persistence rules
//! ([`persist`]) refuse: a failed SetCode (EIP-7702) transaction's
//! self-authorization that cannot apply. Since v0.7.4 `run_sized` retries
//! only when the rules refuse the block as it is, on a copy with each such
//! authorization marked discarded ([`unapplicable_self_authorizations_discarded`]),
//! and the unchanged rules then resolve that copy. Native v0.7.9 takes the
//! same correction in its own crate; `persist.rs` and `common/persist` are
//! unchanged.
use crate::{eth, persist, Discard};

/// Whether a record or a call boundary of `call` takes `ordinal`: the check
/// of the persistence rules' SetCode boundary walk.
fn takes_ordinal(call: &eth::Call, ordinal: u64) -> bool {
    call.begin_ordinal == ordinal
        || call.end_ordinal == ordinal
        || call.storage_changes.iter().any(|c| c.ordinal == ordinal)
        || call.balance_changes.iter().any(|c| c.ordinal == ordinal)
        || call.nonce_changes.iter().any(|c| c.ordinal == ordinal)
        || call.logs.iter().any(|c| c.ordinal == ordinal)
        || call.code_changes.iter().any(|c| c.ordinal == ordinal)
        || call.gas_changes.iter().any(|c| c.ordinal == ordinal)
        || call.account_creations.iter().any(|c| c.ordinal == ordinal)
}

/// The block with each self-authorization that cannot apply
/// ([`unapplicable_self_authorization`]) marked discarded, or `None` when the
/// block has none. `run_sized` calls it only when the persistence rules
/// refuse the block as it is; the rules then resolve the corrected block,
/// and inference reads the block as delivered with the rules' records of
/// the corrected one.
pub(crate) fn unapplicable_self_authorizations_discarded(block: &eth::Block) -> Option<eth::Block> {
    let mut corrected: Option<eth::Block> = None;
    for (index, trx) in block.transaction_traces.iter().enumerate() {
        if let Some(position) = unapplicable_self_authorization(trx) {
            let b = corrected.get_or_insert_with(|| block.clone());
            b.transaction_traces[index].set_code_authorizations[position].discarded = true;
        }
    }
    corrected
}

/// A failed SetCode (EIP-7702) transaction keeps its authorizations' nonce
/// and code records, and for a root call recorded with `begin_ordinal` 0 the
/// persistence rules recover the boundary between them and the execution
/// from the exact records of every authorization not marked discarded. Geth
/// (and Nitro) increments the sender's nonce from the transaction's nonce `n`
/// before it applies the authorization list, and an authorization applies
/// only with its authority's current nonce, so the sender's own
/// authorization with nonce `n` never applies and leaves no record. The
/// Firehose tracer (`discardUncommittedSetCodeAuthorization`) marks an
/// authorization discarded only when the root call has no unused nonce
/// change of its authority from its nonce to the next, and the sender's own
/// nonce change `n -> n + 1` is one: it leaves that authorization unmarked,
/// and the rules find no record for it and refuse the block. Arbitrum One's
/// producer of `Block.ver` 3 records every root call's begin ordinal as 0,
/// and v0.7.3 refused blocks 358,023,229 (`0x95af748b…` of `0x0128c9c9…`,
/// reverted), 382,234,350 (`0xfb8afece…` of `0xbd25f6e6…`, failed out of
/// gas), 420,239,048 (`0x80435ffd…`, reverted) and 472,773,590
/// (`0xc495d3c3…`, reverted, a delegation to the zero address), whose only
/// authorization is such a self-authorization, and 360,021,045
/// (`0x1550f5d1…` of `0xb8b2730a…`, reverted), whose first authorization is
/// one and whose second, of the sender with nonce `n + 1`, applies. In each,
/// the ordinal after the sender's nonce record, or after the second
/// authorization's records, is the only one of the transaction that no
/// record or call boundary takes.
///
/// Returns the position of that authorization in the list when the shape is
/// exact, as native v0.7.9 takes it: a failed SetCode transaction whose root
/// call records `begin_ordinal` 0 and that the rules refuse as it is;
/// exactly one authorization not marked discarded whose authority is the
/// transaction's sender and whose nonce is the transaction's; exactly one
/// nonce record of the sender in the root call from that nonce to the next,
/// at a positive ordinal; and, with the authorization marked discarded,
/// either the rules accept the transaction and the sender keeps another
/// authorization (whose records the rules' walk then checks, with every
/// nonce and code record of the sender before the boundary), or no
/// authorization is left and the boundary is where the rules' walk would put
/// it: the ordinal after the sender's nonce record, which no record or call
/// boundary of the transaction takes, before the root call's end, with no
/// other nonce or code record of the sender before it. Any other shape, such
/// as one where only other authorities' authorizations are left, keeps the
/// rules' refusal. A failed transaction persists no storage write and no
/// log, so the correction changes no vote, read or row of the block's other
/// transactions: inference keeps no nonce record, and a code record only
/// doubts a candidate contract whose code it removes.
fn unapplicable_self_authorization(trx: &eth::TransactionTrace) -> Option<usize> {
    let root = trx.calls.first()?;
    if trx.r#type != eth::transaction_trace::Type::TrxTypeSetCode
        || !persist::is_failed(trx)
        || root.begin_ordinal != 0
        || persist::collect_transaction(trx, &mut Discard).is_ok()
    {
        return None;
    }
    let own: Vec<usize> = (0..trx.set_code_authorizations.len())
        .filter(|&i| {
            let a = &trx.set_code_authorizations[i];
            !a.discarded && !trx.from.is_empty() && a.authority.as_deref() == Some(trx.from.as_slice()) && a.nonce == trx.nonce
        })
        .collect();
    let [position] = own.as_slice() else {
        return None;
    };
    let nonces: Vec<_> = root
        .nonce_changes
        .iter()
        .filter(|n| n.address == trx.from && n.old_value == trx.nonce && Some(n.new_value) == trx.nonce.checked_add(1))
        .collect();
    let [sender_nonce] = nonces.as_slice() else {
        return None;
    };
    if sender_nonce.ordinal == 0 {
        return None;
    }
    let mut corrected = trx.clone();
    corrected.set_code_authorizations[*position].discarded = true;
    let authorities = persist::authorities(&corrected);
    if authorities.contains(&trx.from) {
        persist::collect_transaction(&corrected, &mut Discard).ok()?;
    } else if authorities.is_empty() {
        let boundary = sender_nonce.ordinal.checked_add(1)?;
        let before = |ordinal: u64| ordinal > 0 && ordinal < boundary;
        let sender_records = root
            .nonce_changes
            .iter()
            .filter(|n| !std::ptr::eq(*n, *sender_nonce) && n.address == trx.from && before(n.ordinal))
            .count()
            + root.code_changes.iter().filter(|c| c.address == trx.from && before(c.ordinal)).count();
        let taken = trx.begin_ordinal == boundary || trx.end_ordinal == boundary || trx.calls.iter().any(|c| takes_ordinal(c, boundary));
        if sender_records != 0 || taken || boundary >= root.end_ordinal {
            return None;
        }
    } else {
        return None;
    }
    Some(*position)
}
