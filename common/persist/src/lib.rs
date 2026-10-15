// Adapted from pinax-network/substreams-evm-state at 29251362c8651fd6d5dbe7b3d69820c29ae172c5 (Apache-2.0).
//! Persistence rules: which state-change records in an Extended
//! `sf.ethereum.type.v2.Block` actually survived execution.
//!
//! This crate is the shared home of the rules first qualified inside
//! `erc20/balances/src/persist.rs`. That package keeps its own embedded copy;
//! the two must not diverge in semantics. New Extended packages depend on this
//! crate.
//!
//! Rules (from the `type.proto` documentation, verified on BSC Firehose):
//!
//! * `SUCCEEDED` tx: every change of every call with `state_reverted == false`.
//! * `FAILED` / `REVERTED` tx: only the root call is consulted. Balance
//!   changes with reason `GAS_BUY`, `GAS_REFUND`, `REWARD_TRANSACTION_FEE`
//!   persist; the sender's transaction nonce increment persists.
//! * EIP-7702 (`TRX_TYPE_SET_CODE`): nonce + code changes of every
//!   non-discarded `authority` before root-call execution persist even when
//!   the tx fails. Reverted execution can change the same authority again.
//!   A root call recorded with `begin_ordinal` 0 (`Block.ver` 3) has its
//!   boundary recovered from the exact shape of the authorization records.
//! * `Block.system_calls`: calls with `state_reverted == false`.
//! * `Block.balance_changes`, `Block.code_changes`: always.
//! * No-op records (`old == new`) are dropped from changes. Storage no-ops have
//!   a separate optional validation hook for invariants that forbid any write.

use std::collections::HashSet;
use substreams::errors::Error;

use substreams_ethereum::pb::eth::v2::{
    balance_change::Reason, transaction_trace::Type as TxType, BalanceChange, Block, Call, CodeChange, NonceChange, StorageChange, TransactionTrace,
    TransactionTraceStatus,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Tx,
    TxFailedPersistent,
    Tx7702,
    SystemCall,
    Block,
}

/// Provenance attached to each emitted record.
#[derive(Clone, Copy, Debug)]
pub struct Ctx<'a> {
    pub scope: Scope,
    pub tx_hash: &'a [u8],
    pub tx_index: u32,
    pub tx_status: i32,
    pub call_index: u32,
}

impl Ctx<'_> {
    fn block(scope: Scope) -> Self {
        Ctx {
            scope,
            tx_hash: &[],
            tx_index: 0,
            tx_status: 0,
            call_index: 0,
        }
    }
}

/// Receives persisted records. Implemented by the map module.
pub trait Sink {
    fn storage(&mut self, c: &StorageChange, ctx: Ctx);
    fn storage_noop(&mut self, _: &StorageChange, _: Ctx) {}
    fn balance(&mut self, c: &BalanceChange, ctx: Ctx);
    fn nonce(&mut self, c: &NonceChange, ctx: Ctx);
    fn code(&mut self, c: &CodeChange, ctx: Ctx);
}

pub fn is_failed(trx: &TransactionTrace) -> bool {
    trx.status != TransactionTraceStatus::Succeeded
}

pub fn is_gas_reason(bc: &BalanceChange) -> bool {
    matches!(bc.reason.as_known(), Some(Reason::GasBuy | Reason::GasRefund | Reason::RewardTransactionFee))
}

fn balance_is_noop(bc: &BalanceChange) -> bool {
    let old = bc.old_value.as_option().map(|v| v.bytes.as_slice()).unwrap_or(&[]);
    let new = bc.new_value.as_option().map(|v| v.bytes.as_slice()).unwrap_or(&[]);
    strip_zeros(old) == strip_zeros(new)
}

fn strip_zeros(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() && b[i] == 0 {
        i += 1;
    }
    &b[i..]
}

fn storage_is_noop(sc: &StorageChange) -> bool {
    strip_zeros(&sc.old_value) == strip_zeros(&sc.new_value)
}

/// Non-discarded EIP-7702 authorities of a SetCode transaction.
pub fn authorities(trx: &TransactionTrace) -> HashSet<Vec<u8>> {
    if trx.r#type != TxType::TrxTypeSetCode {
        return HashSet::new();
    }
    trx.set_code_authorizations
        .iter()
        .filter(|a| !a.discarded)
        .filter_map(|a| a.authority.clone())
        .filter(|a| !a.is_empty())
        .collect()
}

fn emit_call_all(call: &Call, ctx: Ctx, out: &mut impl Sink) {
    for c in &call.storage_changes {
        if !storage_is_noop(c) {
            out.storage(c, ctx);
        } else {
            out.storage_noop(c, ctx);
        }
    }
    for c in &call.balance_changes {
        if !balance_is_noop(c) {
            out.balance(c, ctx);
        }
    }
    for c in &call.nonce_changes {
        if c.old_value != c.new_value {
            out.nonce(c, ctx);
        }
    }
    for c in &call.code_changes {
        if c.old_hash != c.new_hash {
            out.code(c, ctx);
        }
    }
}

/// Whether a record or a call boundary of `call` takes `ordinal`.
fn takes_ordinal(call: &Call, ordinal: u64) -> bool {
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

/// The producer of `Block.ver` 3 records every root call's `begin_ordinal` as
/// 0, although the root call's begin still takes an ordinal, one that no
/// record carries. On Arbitrum One a SetCode transaction keeps the order of
/// geth's state transition before that begin: the sender's `GAS_BUY`, the
/// sender's nonce record and then, for each applied authorization in list
/// order, the authority's nonce record from the authorization's nonce to the
/// next and its code record installing the delegation `0xef0100 || address`
/// (no code for the zero address), at consecutive ordinals, after the
/// authority's creation when the account did not exist. In block 350,048,039
/// the reverted transaction `0xfed9d2ec…` (tx 4) of `0xcd21f0d3…`, which
/// authorizes itself, records `GAS_BUY` at 188, the sender's nonce 588 -> 589
/// at 191, its authorization's nonce 589 -> 590 at 192 and its delegation to
/// `0x63c0c19a…` at 193; no record takes 194, and the execution starts at 195
/// (the first child call at 198). In block 500,001,384 the succeeded
/// transaction `0x320cf8c8…` (tx 3) records the creation of the new authority
/// `0x0a01def3…` at 127 (listed twice), before its nonce 0 -> 1 at 128 and its
/// delegation at 129; no record takes 130. Arbitrum One was scanned in five
/// windows, 350.00–350.02M, 400.00–400.02M, 450.00–450.08M, 500.00–500.08M and
/// 514.791–514.813M (222,000 blocks): in each of their 8,778 SetCode
/// transactions, succeeded or reverted, the ordinal after these records is the
/// only one in the transaction that no record or call boundary takes.
///
/// Returns that ordinal, the boundary between the authorization records and
/// the execution, of a failed SetCode transaction whose root call records
/// `begin_ordinal` 0, only in this exact shape: from the ordinal after the
/// sender's nonce record, every non-discarded authorization with an authority
/// has, at consecutive ordinals, optionally the authority's creation (listed
/// once or more, nothing else there, for nonce 0 only), then exactly one nonce
/// record, of the authority from the authorization's nonce to the next, then
/// exactly one code record, of the authority with the delegation as its new
/// code (the authorization's address has 20 bytes); the root call holds no
/// other nonce or code record of an authority before the returned ordinal; no
/// record or call boundary of the transaction takes the returned ordinal; and
/// the root call ends after it. Otherwise `None`, and the transaction fails its
/// block.
fn unrecorded_root_begin(trx: &TransactionTrace, root: &Call, sender_nonce: Option<&NonceChange>, auths: &HashSet<Vec<u8>>) -> Option<u64> {
    let mut next = sender_nonce?.ordinal.checked_add(1)?;
    let mut walked = 0;
    for auth in trx.set_code_authorizations.iter().filter(|a| !a.discarded) {
        // Geth cannot apply an authorization whose authority it cannot recover.
        let Some(authority) = auth.authority.as_deref().filter(|a| !a.is_empty()) else {
            continue;
        };
        let created: Vec<_> = root.account_creations.iter().filter(|c| c.ordinal == next).collect();
        if !created.is_empty() {
            if auth.nonce != 0 || created.iter().any(|c| c.account != authority) {
                return None;
            }
            next += 1;
        }
        let nonces: Vec<_> = root.nonce_changes.iter().filter(|n| n.ordinal == next).collect();
        let codes: Vec<_> = root.code_changes.iter().filter(|c| c.ordinal == next + 1).collect();
        let ([nonce], [code]) = (nonces.as_slice(), codes.as_slice()) else {
            return None;
        };
        let delegation = match auth.address.as_slice() {
            address if address.len() != 20 => return None,
            address if address.iter().all(|b| *b == 0) => Vec::new(),
            address => [&[0xef, 0x01, 0x00][..], address].concat(),
        };
        if nonce.address != authority
            || nonce.old_value != auth.nonce
            || Some(nonce.new_value) != auth.nonce.checked_add(1)
            || code.address != authority
            || code.new_code != delegation
        {
            return None;
        }
        next += 2;
        walked += 2;
    }
    let before = |ordinal: u64| ordinal > 0 && ordinal < next;
    let authority_records = root
        .nonce_changes
        .iter()
        .filter(|n| !sender_nonce.is_some_and(|sender| std::ptr::eq(*n, sender)))
        .filter(|n| before(n.ordinal) && auths.contains(&n.address))
        .count()
        + root.code_changes.iter().filter(|c| before(c.ordinal) && auths.contains(&c.address)).count();
    let taken = trx.begin_ordinal == next || trx.end_ordinal == next || trx.calls.iter().any(|c| takes_ordinal(c, next));
    (authority_records == walked && !taken && next < root.end_ordinal).then_some(next)
}

pub fn collect_transaction(trx: &TransactionTrace, out: &mut impl Sink) -> Result<(), Error> {
    // As prost's `status()` accessor did, an unknown status reads as UNKNOWN (0).
    let status = trx.status.as_known().unwrap_or_default() as i32;
    let base = |scope: Scope, call_index: u32| Ctx {
        scope,
        tx_hash: &trx.hash,
        tx_index: trx.index,
        tx_status: status,
        call_index,
    };

    if !is_failed(trx) {
        for call in &trx.calls {
            if call.state_reverted {
                continue;
            }
            emit_call_all(call, base(Scope::Tx, call.index), out);
        }
        return Ok(());
    }

    // Failed or reverted: only the root call carries persisted effects.
    let Some(root) = trx.calls.first() else { return Ok(()) };
    let ctx = base(Scope::TxFailedPersistent, root.index);

    for bc in &root.balance_changes {
        if is_gas_reason(bc) && !balance_is_noop(bc) {
            out.balance(bc, ctx);
        }
    }

    // Reverted execution records may have ordinal zero. Neither array order nor
    // the smallest ordinal across all accounts identifies the sender. Match the
    // transaction's actual sender and nonce increment instead.
    let sender_nonce = root
        .nonce_changes
        .iter()
        .filter(|n| n.ordinal > 0 && n.address == trx.from && n.old_value == trx.nonce && Some(n.new_value) == trx.nonce.checked_add(1))
        .min_by_key(|n| n.ordinal);
    if let Some(change) = sender_nonce {
        out.nonce(change, ctx);
    }

    // EIP-7702: authorization nonce/code changes persist for non-discarded authorities.
    let auths = authorities(trx);
    if !auths.is_empty() {
        let begin = match root.begin_ordinal {
            0 => unrecorded_root_begin(trx, root, sender_nonce, &auths),
            begin => Some(begin),
        };
        let Some(begin) = begin else {
            return Err(Error::msg("failed SetCode transaction has no authorization/execution ordinal boundary"));
        };
        let ctx7702 = base(Scope::Tx7702, root.index);
        // Geth and Reth emit the auth-list changes before the root call begins.
        // An authority can execute CREATE (nonce change), or other reverted code,
        // later in that same root call. Address membership is not persistence.
        let before_execution = |ordinal| ordinal > 0 && ordinal < begin;
        for nc in &root.nonce_changes {
            if sender_nonce.is_some_and(|sender| std::ptr::eq(nc, sender)) {
                continue; // already emitted above
            }
            if before_execution(nc.ordinal) && auths.contains(&nc.address) && nc.old_value != nc.new_value {
                out.nonce(nc, ctx7702);
            }
        }
        for cc in &root.code_changes {
            if before_execution(cc.ordinal) && auths.contains(&cc.address) && cc.old_hash != cc.new_hash {
                out.code(cc, ctx7702);
            }
        }
    }
    Ok(())
}

pub fn collect_block(block: &Block, out: &mut impl Sink) -> Result<(), Error> {
    for call in &block.system_calls {
        if call.state_reverted {
            continue;
        }
        emit_call_all(
            call,
            Ctx {
                call_index: call.index,
                ..Ctx::block(Scope::SystemCall)
            },
            out,
        );
    }
    let ctx = Ctx::block(Scope::Block);
    for bc in &block.balance_changes {
        if !balance_is_noop(bc) {
            out.balance(bc, ctx);
        }
    }
    for cc in &block.code_changes {
        if cc.old_hash != cc.new_hash {
            out.code(cc, ctx);
        }
    }
    for trx in &block.transaction_traces {
        collect_transaction(trx, out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa::Message;
    use substreams_ethereum::pb::eth::v2::{AccountCreation, BigInt, GasChange, SetCodeAuthorization};

    #[derive(Clone, Debug, Default, PartialEq)]
    struct Rec {
        storage: Vec<(Vec<u8>, u64, Scope)>,
        balance: Vec<(Vec<u8>, i32, Scope)>,
        nonce: Vec<(Vec<u8>, u64, Scope)>,
        code: Vec<(Vec<u8>, Scope)>,
    }
    impl Sink for Rec {
        fn storage(&mut self, c: &StorageChange, ctx: Ctx) {
            self.storage.push((c.address.clone(), c.ordinal, ctx.scope))
        }
        fn balance(&mut self, c: &BalanceChange, ctx: Ctx) {
            self.balance.push((c.address.clone(), c.reason.to_i32(), ctx.scope))
        }
        fn nonce(&mut self, c: &NonceChange, ctx: Ctx) {
            self.nonce.push((c.address.clone(), c.new_value, ctx.scope))
        }
        fn code(&mut self, c: &CodeChange, ctx: Ctx) {
            self.code.push((c.address.clone(), ctx.scope))
        }
    }

    fn big(v: u64) -> Option<BigInt> {
        Some(BigInt {
            bytes: v.to_be_bytes().to_vec(),
        })
    }
    fn bal(addr: u8, reason: Reason, ord: u64, old: u64, new: u64) -> BalanceChange {
        BalanceChange {
            address: vec![addr; 20],
            old_value: big(old).into(),
            new_value: big(new).into(),
            reason: reason.into(),
            ordinal: ord,
        }
    }
    fn nonce(addr: u8, ord: u64, old: u64) -> NonceChange {
        NonceChange {
            address: vec![addr; 20],
            old_value: old,
            new_value: old + 1,
            ordinal: ord,
        }
    }
    fn storage(addr: u8, ord: u64, old: u8, new: u8) -> StorageChange {
        StorageChange {
            address: vec![addr; 20],
            key: vec![1; 32],
            old_value: vec![old; 32],
            new_value: vec![new; 32],
            ordinal: ord,
        }
    }
    fn code(addr: u8, ord: u64) -> CodeChange {
        CodeChange {
            address: vec![addr; 20],
            old_hash: vec![],
            old_code: vec![],
            new_hash: vec![9; 32],
            new_code: vec![0xef, 1, 0],
            ordinal: ord,
        }
    }

    #[test]
    fn succeeded_tx_keeps_non_reverted_calls_only() {
        let trx = TransactionTrace {
            status: TransactionTraceStatus::Succeeded.into(),
            calls: vec![
                Call {
                    index: 0,
                    storage_changes: vec![storage(0xA, 10, 0, 1)],
                    ..Default::default()
                },
                Call {
                    index: 1,
                    state_reverted: true,
                    storage_changes: vec![storage(0xB, 11, 0, 1)],
                    ..Default::default()
                },
                Call {
                    index: 2,
                    storage_changes: vec![storage(0xC, 12, 5, 5)],
                    ..Default::default()
                }, // no-op
            ],
            ..Default::default()
        };
        let mut r = Rec::default();
        collect_transaction(&trx, &mut r).unwrap();
        assert_eq!(r.storage, vec![(vec![0xA; 20], 10, Scope::Tx)]);
    }

    #[test]
    fn failed_tx_keeps_gas_and_sender_nonce_only() {
        let trx = TransactionTrace {
            status: TransactionTraceStatus::Reverted.into(),
            from: vec![0xA; 20],
            nonce: 7,
            calls: vec![
                Call {
                    index: 0,
                    state_reverted: true,
                    balance_changes: vec![
                        bal(0xA, Reason::GasBuy, 1, 100, 90),
                        bal(0xA, Reason::Transfer, 2, 90, 80),
                        bal(0xB, Reason::Transfer, 3, 0, 10),
                        bal(0xA, Reason::GasRefund, 8, 80, 85),
                        bal(0xF, Reason::RewardTransactionFee, 9, 0, 5),
                    ],
                    nonce_changes: vec![nonce(0xA, 1, 7), nonce(0xB, 4, 0)],
                    storage_changes: vec![storage(0xB, 5, 0, 1)],
                    ..Default::default()
                },
                Call {
                    index: 1,
                    state_reverted: true,
                    storage_changes: vec![storage(0xC, 6, 0, 1)],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut r = Rec::default();
        collect_transaction(&trx, &mut r).unwrap();
        assert!(r.storage.is_empty());
        assert_eq!(r.balance.len(), 3);
        assert!(r
            .balance
            .iter()
            .all(|(_, reason, s)| *s == Scope::TxFailedPersistent && *reason != Reason::Transfer as i32));
        assert_eq!(r.nonce, vec![(vec![0xA; 20], 8, Scope::TxFailedPersistent)]);
    }

    #[test]
    fn failed_7702_tx_keeps_authority_nonce_and_code() {
        let trx = TransactionTrace {
            status: TransactionTraceStatus::Failed.into(),
            from: vec![0xA; 20],
            nonce: 7,
            r#type: TxType::TrxTypeSetCode.into(),
            set_code_authorizations: vec![
                SetCodeAuthorization {
                    authority: Some(vec![0xB; 20]),
                    discarded: false,
                    ..Default::default()
                },
                SetCodeAuthorization {
                    authority: Some(vec![0xC; 20]),
                    discarded: true,
                    ..Default::default()
                },
            ],
            calls: vec![Call {
                index: 0,
                begin_ordinal: 6,
                state_reverted: true,
                balance_changes: vec![bal(0xA, Reason::GasBuy, 1, 100, 90)],
                nonce_changes: vec![nonce(0xA, 1, 7), nonce(0xB, 2, 3), nonce(0xC, 3, 0)],
                code_changes: vec![code(0xB, 4), code(0xC, 5)],
                storage_changes: vec![storage(0xB, 6, 0, 1)],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut r = Rec::default();
        collect_transaction(&trx, &mut r).unwrap();
        assert_eq!(r.nonce, vec![(vec![0xA; 20], 8, Scope::TxFailedPersistent), (vec![0xB; 20], 4, Scope::Tx7702)]);
        assert_eq!(r.code, vec![(vec![0xB; 20], Scope::Tx7702)]);
        assert!(r.storage.is_empty());
    }

    #[test]
    fn failed_sender_selection_ignores_zero_ordinals_and_other_accounts() {
        let trx = TransactionTrace {
            status: TransactionTraceStatus::Reverted.into(),
            from: vec![0xA; 20],
            nonce: 7,
            calls: vec![Call {
                state_reverted: true,
                nonce_changes: vec![nonce(0xB, 0, 0), nonce(0xC, 1, 0), nonce(0xA, 0, 8), nonce(0xA, 2, 7), nonce(0xA, 20, 8)],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut r = Rec::default();
        collect_transaction(&trx, &mut r).unwrap();
        assert_eq!(r.nonce, vec![(vec![0xA; 20], 8, Scope::TxFailedPersistent)]);
    }

    #[test]
    fn failed_self_and_multiple_authorizations_exclude_reverted_execution() {
        // Auth-list effects precede root begin (10); the same delegated account
        // then attempts CREATE in reverted execution (20). Also include a
        // zero-ordinal reverted record, a discarded auth and explicit code clear.
        let mut clear = code(0xB, 8);
        clear.old_hash = vec![8; 32];
        clear.new_hash = vec![];
        clear.new_code = vec![];
        let trx = TransactionTrace {
            status: TransactionTraceStatus::Failed.into(),
            r#type: TxType::TrxTypeSetCode.into(),
            from: vec![0xA; 20],
            nonce: 7,
            set_code_authorizations: vec![
                SetCodeAuthorization {
                    authority: Some(vec![0xA; 20]),
                    nonce: 8,
                    ..Default::default()
                },
                SetCodeAuthorization {
                    authority: Some(vec![0xB; 20]),
                    nonce: 3,
                    ..Default::default()
                },
                SetCodeAuthorization {
                    authority: Some(vec![0xB; 20]),
                    nonce: 4,
                    ..Default::default()
                },
                SetCodeAuthorization {
                    authority: Some(vec![0xC; 20]),
                    discarded: true,
                    ..Default::default()
                },
            ],
            calls: vec![Call {
                begin_ordinal: 10,
                end_ordinal: 30,
                state_reverted: true,
                nonce_changes: vec![
                    nonce(0xA, 2, 7),
                    nonce(0xA, 3, 8),
                    nonce(0xB, 5, 3),
                    nonce(0xB, 7, 4),
                    nonce(0xC, 9, 0),
                    nonce(0xA, 20, 9),
                    nonce(0xB, 0, 5),
                ],
                code_changes: vec![code(0xA, 4), code(0xB, 6), clear, code(0xC, 9), code(0xA, 21), code(0xB, 0)],
                storage_changes: vec![storage(0xA, 25, 0, 1)],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut r = Rec::default();
        collect_transaction(&trx, &mut r).unwrap();
        assert_eq!(
            r.nonce,
            vec![
                (vec![0xA; 20], 8, Scope::TxFailedPersistent),
                (vec![0xA; 20], 9, Scope::Tx7702),
                (vec![0xB; 20], 4, Scope::Tx7702),
                (vec![0xB; 20], 5, Scope::Tx7702)
            ]
        );
        assert_eq!(
            r.code,
            vec![(vec![0xA; 20], Scope::Tx7702), (vec![0xB; 20], Scope::Tx7702), (vec![0xB; 20], Scope::Tx7702)]
        );
        assert!(r.storage.is_empty());
    }

    #[test]
    fn ambiguous_failed_authorization_boundary_is_rejected() {
        let trx = TransactionTrace {
            status: TransactionTraceStatus::Failed.into(),
            r#type: TxType::TrxTypeSetCode.into(),
            set_code_authorizations: vec![SetCodeAuthorization {
                authority: Some(vec![0xA; 20]),
                ..Default::default()
            }],
            calls: vec![Call::default()],
            ..Default::default()
        };
        assert!(collect_transaction(&trx, &mut Rec::default()).is_err());
    }

    #[test]
    fn captured_bsc_failed_setcode_transactions_keep_sender_and_authority_nonces() {
        // Unmodified TransactionTrace messages fetched from BSC Extended v5.
        // Provenance, block/transaction hashes and checksums are next to the files.
        let samples: [(&[u8], u64, u64, i32); 2] = [
            (
                include_bytes!("../../../erc20/balances/tests/fixtures/bsc-121114122-failed-setcode.pb"),
                27164,
                4308,
                3,
            ),
            (
                include_bytes!("../../../erc20/balances/tests/fixtures/bsc-121114153-failed-setcode.pb"),
                27167,
                4311,
                2,
            ),
        ];
        let sender = hex::decode("d52573f6d4f68d8e7f8fe2ed50a1023c5f6fe82a").unwrap();
        let authority = hex::decode("417204ea716dfc4427bf9883521c820b036cdb7a").unwrap();
        for (bytes, sender_nonce, authority_nonce, status) in samples {
            let tx = TransactionTrace::decode_from_slice(bytes).unwrap();
            assert_eq!(tx.status.to_i32(), status);
            assert_eq!(tx.r#type, TxType::TrxTypeSetCode);
            assert!(tx.calls[0].state_reverted);
            let mut r = Rec::default();
            collect_transaction(&tx, &mut r).unwrap();
            assert_eq!(
                r.nonce,
                vec![
                    (sender.clone(), sender_nonce, Scope::TxFailedPersistent),
                    (authority.clone(), authority_nonce, Scope::Tx7702)
                ]
            );
            assert!(r.storage.is_empty() && r.code.is_empty());
            assert!(!r.balance.is_empty());
            assert!(r.balance.iter().all(|(_, reason, scope)| *scope == Scope::TxFailedPersistent
                && [Reason::GasBuy as i32, Reason::GasRefund as i32, Reason::RewardTransactionFee as i32].contains(reason)));
        }
    }

    // Unmodified TransactionTrace messages of Arbitrum One's `Block.ver` 3
    // producer, whose root calls record `begin_ordinal` 0: four reverted
    // SetCode transactions and a succeeded one with a new authority.
    // Provenance, block/transaction hashes and checksums are in
    // `arbitrum-setcode.json` next to the files.
    const ARBITRUM_SELF_AUTHORIZATION: &[u8] = include_bytes!("../../../erc20/balances/tests/fixtures/arbitrum-350048039-failed-setcode.pb");
    const ARBITRUM_ROOT_ONLY: &[u8] = include_bytes!("../../../erc20/balances/tests/fixtures/arbitrum-400015655-failed-setcode.pb");
    const ARBITRUM_DISCARDED_FIRST: &[u8] = include_bytes!("../../../erc20/balances/tests/fixtures/arbitrum-450003654-failed-setcode.pb");
    const ARBITRUM_THREE_AUTHORIZATIONS: &[u8] = include_bytes!("../../../erc20/balances/tests/fixtures/arbitrum-514791024-failed-setcode.pb");
    const ARBITRUM_NEW_AUTHORITY: &[u8] = include_bytes!("../../../erc20/balances/tests/fixtures/arbitrum-500001384-setcode-new-authority.pb");

    fn account(hex: &str) -> Vec<u8> {
        hex::decode(hex).unwrap()
    }
    fn collected(tx: &TransactionTrace) -> Result<Rec, Error> {
        let mut r = Rec::default();
        collect_transaction(tx, &mut r).map(|()| r)
    }
    /// `unrecorded_root_begin` with the sender's nonce record that
    /// `collect_transaction` selects.
    fn recovered_begin(tx: &TransactionTrace) -> Option<u64> {
        let root = &tx.calls[0];
        let sender = root
            .nonce_changes
            .iter()
            .filter(|n| n.ordinal > 0 && n.address == tx.from && n.old_value == tx.nonce && n.new_value == tx.nonce + 1)
            .min_by_key(|n| n.ordinal);
        unrecorded_root_begin(tx, root, sender, &authorities(tx))
    }

    #[test]
    fn captured_arbitrum_failed_setcode_transactions_keep_sender_and_authority_records() {
        use Scope::{Tx7702 as Auth, TxFailedPersistent as Failed};
        /// A captured transaction: the ordinal no record takes, the sender
        /// and the authority with the nonces they persist, whether the
        /// delegation changes the code, and the account of a tip.
        struct Sample {
            bytes: &'static [u8],
            boundary: u64,
            sender: &'static str,
            sender_nonce: u64,
            authority: &'static str,
            authority_nonces: &'static [u64],
            code_changed: bool,
            tip: Option<&'static str>,
        }
        let samples = [
            // 0xfed9d2ec…: the sender authorizes itself; its nonce 588 -> 589
            // at 191, the authorization's 589 -> 590 at 192, code at 193.
            Sample {
                bytes: ARBITRUM_SELF_AUTHORIZATION,
                boundary: 194,
                sender: "cd21f0d3ad097308c39e2b7914779787d22e831c",
                sender_nonce: 589,
                authority: "cd21f0d3ad097308c39e2b7914779787d22e831c",
                authority_nonces: &[590],
                code_changed: true,
                tip: None,
            },
            // 0xa777590e…: no child call; the delegation is renewed unchanged.
            Sample {
                bytes: ARBITRUM_ROOT_ONLY,
                boundary: 253,
                sender: "4cf609fc3fd878b34c7ea3d63f732248fd7a5b4c",
                sender_nonce: 34866,
                authority: "b9c0de368bece5e76b52545a8e377a4c118f597b",
                authority_nonces: &[6420],
                code_changed: false,
                tip: None,
            },
            // 0x009b640c…: a discarded authorization (nonce 21), then four
            // applied ones of the same authority, nonces 22 to 25.
            Sample {
                bytes: ARBITRUM_DISCARDED_FIRST,
                boundary: 359,
                sender: "bd43bd6b25e80310869a56d4e395026b8256dd2a",
                sender_nonce: 157,
                authority: "40bc59c8b9f16d8e58d9e13580e00b7386364ab5",
                authority_nonces: &[23, 24, 25, 26],
                code_changed: true,
                tip: None,
            },
            // 0xdb5f4993…: three applied authorizations of one authority, and
            // a tip to 0x32e7af5a….
            Sample {
                bytes: ARBITRUM_THREE_AUTHORIZATIONS,
                boundary: 121,
                sender: "1a222f9b072aed5e9e815849de5a11b0fe3958ab",
                sender_nonce: 1279,
                authority: "02579ad4af7eaee9d5319f946c2ca5cc831c87d9",
                authority_nonces: &[11, 12, 13],
                code_changed: false,
                tip: Some("32e7af5a8151934f3787d0cd59eb6edd0a736b1d"),
            },
        ];
        for sample in samples {
            let tx = TransactionTrace::decode_from_slice(sample.bytes).unwrap();
            assert_eq!(tx.status.as_known(), Some(TransactionTraceStatus::Reverted));
            assert_eq!(tx.r#type, TxType::TrxTypeSetCode);
            assert_eq!(tx.calls[0].begin_ordinal, 0);
            assert_eq!(recovered_begin(&tx), Some(sample.boundary));
            let (sender, authority) = (account(sample.sender), account(sample.authority));
            let r = collected(&tx).unwrap();
            let mut nonces = vec![(sender.clone(), sample.sender_nonce, Failed)];
            nonces.extend(sample.authority_nonces.iter().map(|n| (authority.clone(), *n, Auth)));
            assert_eq!(r.nonce, nonces);
            assert_eq!(r.code, if sample.code_changed { vec![(authority.clone(), Auth)] } else { vec![] });
            let mut gas = vec![(sender.clone(), Reason::GasBuy as i32, Failed), (sender, Reason::GasRefund as i32, Failed)];
            gas.extend(sample.tip.map(|tip| (account(tip), Reason::RewardTransactionFee as i32, Failed)));
            assert_eq!(r.balance, gas);
            assert!(r.storage.is_empty());
            // A producer that records the root call's begin there persists the same.
            let mut recorded = tx.clone();
            recorded.calls[0].begin_ordinal = sample.boundary;
            assert_eq!(collected(&recorded).unwrap(), r);
        }
    }

    #[test]
    fn captured_arbitrum_new_authority_is_created_before_its_records() {
        // 0x320cf8c8… succeeded: the new authority 0x0a01def3… is created at
        // 127 (listed twice), its nonce 0 -> 1 at 128, its delegation at 129;
        // no record takes 130. Its execution writes the authority's storage.
        let mut tx = TransactionTrace::decode_from_slice(ARBITRUM_NEW_AUTHORITY).unwrap();
        assert_eq!(tx.status.as_known(), Some(TransactionTraceStatus::Succeeded));
        let authority = account("0a01def33fc763fb513a6dc2924af7503f878f91");
        assert_eq!(tx.calls[0].account_creations.len(), 2);
        assert!(tx.calls[0].storage_changes.iter().any(|c| c.address == authority && c.ordinal == 136));
        // As if its execution had reverted; the records before it stay.
        tx.status = TransactionTraceStatus::Reverted.into();
        tx.calls.iter_mut().for_each(|c| c.state_reverted = true);
        assert_eq!(recovered_begin(&tx), Some(130));
        let r = collected(&tx).unwrap();
        assert_eq!(
            r.nonce,
            vec![
                (account("0440e61b3b648e59059d18e0c8ac71478b85808b"), 45719, Scope::TxFailedPersistent),
                (authority.clone(), 1, Scope::Tx7702)
            ]
        );
        assert_eq!(r.code, vec![(authority.clone(), Scope::Tx7702)]);
        assert!(r.storage.is_empty());
        // Only the authority's own creation...
        let mut t = tx.clone();
        t.calls[0].account_creations[1].account = vec![0x11; 20];
        assert!(collected(&t).is_err());
        // ...and only for nonce 0.
        let mut t = tx.clone();
        t.set_code_authorizations[0].nonce = 1;
        let authorization = t.calls[0].nonce_changes.iter_mut().find(|n| n.address == authority).unwrap();
        (authorization.old_value, authorization.new_value) = (1, 2);
        assert!(collected(&t).is_err());
    }

    #[test]
    fn the_unrecorded_root_begin_shape_is_exact() {
        fn fails(tx: &TransactionTrace) -> bool {
            collected(tx).is_err_and(|e| e.to_string().contains("no authorization/execution ordinal boundary"))
        }
        let tx = TransactionTrace::decode_from_slice(ARBITRUM_SELF_AUTHORIZATION).unwrap();
        let authority = account("cd21f0d3ad097308c39e2b7914779787d22e831c");
        let expected = collected(&tx).unwrap();
        // Records of the root call: the sender's nonce (191), the authorization's
        // nonce (192) and code (193).
        assert_eq!(tx.calls[0].nonce_changes.iter().map(|n| n.ordinal).collect::<Vec<_>>(), [191, 192]);
        assert_eq!(tx.calls[0].code_changes[0].ordinal, 193);
        // No record takes the unrecorded ordinal 194...
        let mut t = tx.clone();
        t.calls[0].gas_changes.push(GasChange {
            ordinal: 194,
            ..Default::default()
        });
        assert!(fails(&t));
        let mut t = tx.clone();
        t.calls[2].storage_changes.push(storage(0xB, 194, 0, 1));
        assert!(fails(&t));
        // ...nor a call boundary.
        let mut t = tx.clone();
        t.calls[1].begin_ordinal = 194;
        assert!(fails(&t));
        // The authorization's nonce record follows the sender's directly.
        let mut t = tx.clone();
        t.calls[0].nonce_changes[1].ordinal = 190;
        assert!(fails(&t));
        // From the authorization's nonce to the next.
        let mut t = tx.clone();
        t.set_code_authorizations[0].nonce = 590;
        assert!(fails(&t));
        let mut t = tx.clone();
        t.calls[0].nonce_changes[1].old_value = 588;
        assert!(fails(&t));
        let mut t = tx.clone();
        t.calls[0].nonce_changes[1].new_value = 591;
        assert!(fails(&t));
        // Then its code record, installing the authorization's delegation.
        let mut t = tx.clone();
        t.set_code_authorizations[0].address = vec![0x11; 20];
        assert!(fails(&t));
        let mut t = tx.clone();
        t.calls[0].code_changes.clear();
        assert!(fails(&t));
        // The authorization's address has 20 bytes: an empty one installs
        // nothing, not even with an empty new code.
        let mut t = tx.clone();
        t.set_code_authorizations[0].address = vec![];
        t.calls[0].code_changes[0].new_code = vec![];
        assert!(fails(&t));
        // The zero address clears the delegation: the code record's new code
        // is empty, and it persists like any other.
        let mut t = tx.clone();
        t.set_code_authorizations[0].address = vec![0; 20];
        let clearing = &mut t.calls[0].code_changes[0];
        (clearing.old_hash, clearing.new_hash) = (clearing.new_hash.clone(), clearing.old_hash.clone());
        (clearing.old_code, clearing.new_code) = (std::mem::take(&mut clearing.new_code), vec![]);
        let r = collected(&t).unwrap();
        assert_eq!(
            r.nonce,
            vec![(authority.clone(), 589, Scope::TxFailedPersistent), (authority.clone(), 590, Scope::Tx7702)]
        );
        assert_eq!(r.code, vec![(authority.clone(), Scope::Tx7702)]);
        assert_eq!(r, expected);
        // Exactly one record there, of any account.
        let mut t = tx.clone();
        t.calls[0].nonce_changes.push(nonce(0x11, 192, 0));
        assert!(fails(&t));
        let mut t = tx.clone();
        t.calls[0].code_changes.push(code(0x11, 193));
        assert!(fails(&t));
        // No other nonce or code record of an authority before the boundary.
        let mut t = tx.clone();
        t.calls[0].code_changes.push(CodeChange {
            address: authority.clone(),
            ..code(0, 189)
        });
        assert!(fails(&t));
        // A creation only for an authorization of nonce 0.
        let mut t = tx.clone();
        t.calls[0].account_creations.push(AccountCreation {
            account: authority.clone(),
            ordinal: 192,
        });
        assert!(fails(&t));
        // The root call ends after the boundary.
        let mut t = tx.clone();
        t.calls[0].end_ordinal = 0;
        assert!(fails(&t));
        // The walk starts at the sender's nonce record.
        let mut t = tx.clone();
        t.calls[0].nonce_changes.remove(0);
        assert!(fails(&t));
        // Every non-discarded authorization with an authority has its records...
        let other = SetCodeAuthorization {
            authority: Some(vec![0x11; 20]),
            address: vec![0x12; 20],
            ..Default::default()
        };
        let mut t = tx.clone();
        t.set_code_authorizations.push(other.clone());
        assert!(fails(&t));
        // ...while a discarded one and one without an authority have none.
        let mut t = tx.clone();
        t.set_code_authorizations.push(SetCodeAuthorization {
            discarded: true,
            ..other.clone()
        });
        t.set_code_authorizations.push(SetCodeAuthorization { authority: None, ..other });
        assert_eq!(collected(&t).unwrap(), expected);
        // With a recorded begin the shape is not consulted.
        let mut t = tx.clone();
        t.calls[0].code_changes.clear();
        t.calls[0].begin_ordinal = 194;
        assert!(collected(&t).is_ok());
    }

    #[test]
    fn block_level_and_system_calls() {
        let block = Block {
            balance_changes: vec![
                bal(0xF, Reason::RewardTransactionFee, 100, 0, 1),
                bal(0xE, Reason::RewardTransactionFee, 101, 5, 5),
            ],
            system_calls: vec![
                Call {
                    index: 0,
                    storage_changes: vec![storage(0x29, 2, 0, 1)],
                    ..Default::default()
                },
                Call {
                    index: 1,
                    state_reverted: true,
                    storage_changes: vec![storage(0x30, 3, 0, 1)],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut r = Rec::default();
        collect_block(&block, &mut r).unwrap();
        assert_eq!(r.storage, vec![(vec![0x29; 20], 2, Scope::SystemCall)]);
        assert_eq!(r.balance, vec![(vec![0xF; 20], Reason::RewardTransactionFee as i32, Scope::Block)]);
    }
}
