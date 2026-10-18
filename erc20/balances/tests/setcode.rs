//! The captured Arbitrum One failed SetCode transactions whose sender
//! authorizes itself with the transaction's own nonce, an authorization that
//! cannot apply and that the producer leaves unmarked: the embedded
//! persistence rules refuse their blocks, and since v0.7.4 `run` retries them
//! with that authorization marked discarded. Provenance in
//! `fixtures/v074/cases.json` (`setcode_transactions`).
use buffa::Message;
use erc20_balances::{persist, run};
use substreams_ethereum::pb::eth::v2 as eth;
use tiny_keccak::{Hasher, Keccak};

const TOKEN: [u8; 20] = [0x70; 20];
const A: [u8; 20] = [0xa1; 20];
const B: [u8; 20] = [0xb2; 20];
const BOUNDARY: &str = "no authorization/execution ordinal boundary";

fn hash(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0; 32];
    let mut hasher = Keccak::v256();
    hasher.update(bytes);
    hasher.finalize(&mut out);
    out
}
fn w(v: u64) -> Vec<u8> {
    let mut out = vec![0; 32];
    out[24..].copy_from_slice(&v.to_be_bytes());
    out
}
fn pad(a: &[u8; 20]) -> Vec<u8> {
    [vec![0; 12], a.to_vec()].concat()
}

/// A sink that keeps nothing, for asking the shared rules about a transaction.
struct Discard;
impl persist::Sink for Discard {
    fn storage(&mut self, _: &eth::StorageChange, _: persist::Ctx) {}
    fn balance(&mut self, _: &eth::BalanceChange, _: persist::Ctx) {}
    fn nonce(&mut self, _: &eth::NonceChange, _: persist::Ctx) {}
    fn code(&mut self, _: &eth::CodeChange, _: persist::Ctx) {}
}

/// Whether a record or a call boundary of `call` takes `ordinal`.
fn takes(call: &eth::Call, ordinal: u64) -> bool {
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

/// An Arbitrum One `Block.ver` 3 block holding only the captured transaction.
fn block(number: u64, tx: eth::TransactionTrace) -> eth::Block {
    eth::Block {
        ver: 3,
        number,
        hash: vec![1; 32],
        detail_level: eth::block::DetailLevel::DetaillevelExtended.into(),
        header: buffa::MessageField::some(eth::BlockHeader {
            number,
            parent_hash: vec![2; 32],
            state_root: vec![3; 32],
            ..Default::default()
        }),
        transaction_traces: vec![tx],
        ..Default::default()
    }
}

/// Appends a succeeded transaction after the block's last one: a `Transfer`
/// of 30 from A (100) to B (5), under mapping slot 3 with its preimages.
fn with_transfer(b: &eth::Block) -> eth::Block {
    let mut b = b.clone();
    let last = b.transaction_traces.last().unwrap();
    let (index, start) = (last.index + 1, last.end_ordinal + 1);
    let mut root = eth::Call {
        index: 1,
        call_type: eth::CallType::Call.into(),
        address: TOKEN.to_vec(),
        begin_ordinal: start,
        end_ordinal: start + 10,
        ..Default::default()
    };
    root.logs.push(eth::Log {
        address: TOKEN.to_vec(),
        topics: vec![
            hex::decode("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").unwrap(),
            pad(&A),
            pad(&B),
        ],
        data: w(30),
        ordinal: start + 3,
        ..Default::default()
    });
    for (i, (holder, old, new)) in [(A, 100, 70), (B, 5, 35)].into_iter().enumerate() {
        let pre = [pad(&holder), w(3)].concat();
        let key = hash(&pre);
        root.keccak_preimages.insert(hex::encode(key), hex::encode(&pre));
        root.storage_changes.push(eth::StorageChange {
            address: TOKEN.to_vec(),
            key: key.to_vec(),
            old_value: w(old),
            new_value: w(new),
            ordinal: start + 1 + i as u64,
        });
    }
    b.transaction_traces.push(eth::TransactionTrace {
        index,
        from: A.to_vec(),
        status: eth::TransactionTraceStatus::Succeeded.into(),
        begin_ordinal: start,
        end_ordinal: start + 10,
        calls: vec![root],
        ..Default::default()
    });
    b
}

/// Every row of a block as (contract, holder, amount), in hex.
fn rows(b: &eth::Block) -> Vec<(String, String, String)> {
    let rows = run(b).unwrap().balances.into_iter();
    rows.map(|r| (hex::encode(r.contract.unwrap()), hex::encode(r.address), r.amount)).collect()
}
fn transfer_rows() -> Vec<(String, String, String)> {
    let row = |holder: [u8; 20], amount: &str| (hex::encode(TOKEN), hex::encode(holder), amount.to_string());
    vec![row(A, "70"), row(B, "35")]
}
fn fails(b: &eth::Block) -> bool {
    run(b).is_err_and(|e| e.to_string().contains(BOUNDARY))
}

/// The five captured transactions with their provenance records, in the
/// order of `setcode_transactions.records`.
fn captured() -> Vec<(serde_json::Value, eth::Block)> {
    use sha2::Digest;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v074");
    let cases: serde_json::Value = serde_json::from_str(include_str!("fixtures/v074/cases.json")).unwrap();
    let records = cases["setcode_transactions"]["records"].as_array().unwrap();
    assert_eq!(records.len(), 5);
    records
        .iter()
        .map(|r| {
            let file = r["file"].as_str().unwrap();
            let bytes = std::fs::read(format!("{dir}/{file}")).unwrap();
            assert_eq!(hex::encode(sha2::Sha256::digest(&bytes)), r["sha256"], "{file}");
            let tx = eth::TransactionTrace::decode_from_slice(&bytes).unwrap();
            (r.clone(), block(r["block_number"].as_u64().unwrap(), tx))
        })
        .collect()
}
/// The captured block of `number`.
fn of(number: u64) -> eth::Block {
    captured().into_iter().find(|(r, _)| r["block_number"] == number).unwrap().1
}

/// Blocks 358,023,229, 360,021,045, 382,234,350, 420,239,048 and
/// 472,773,590, which ERC-20 v0.7.3 refuses. A failed transaction persists
/// no storage write or log, so each slice alone gives no row; a block that
/// also holds a token transfer gives the transfer's rows, as it would with
/// the authorization marked discarded by the producer.
#[test]
fn captured_arbitrum_unapplicable_self_authorizations_no_longer_fail_their_block() {
    for (r, b) in captured() {
        let n = r["block_number"].as_u64().unwrap();
        let tx = &b.transaction_traces[0];
        let root = &tx.calls[0];
        assert_eq!(format!("0x{}", hex::encode(&tx.hash)), r["transaction_hash"], "{n}");
        assert_eq!(format!("0x{}", hex::encode(&tx.from)), r["sender"], "{n}");
        assert!(persist::is_failed(tx), "{n}");
        assert_eq!(
            (tx.r#type.as_known(), root.begin_ordinal, root.end_ordinal, tx.nonce),
            (
                Some(eth::transaction_trace::Type::TrxTypeSetCode),
                0,
                r["root_end_ordinal"].as_u64().unwrap(),
                r["transaction_nonce"].as_u64().unwrap()
            ),
            "{n}"
        );
        // Every authorization is the sender's, none marked discarded; the
        // first carries the transaction's own nonce.
        assert!(tx
            .set_code_authorizations
            .iter()
            .all(|a| !a.discarded && a.authority.as_deref() == Some(tx.from.as_slice())));
        let nonces: Vec<u64> = tx.set_code_authorizations.iter().map(|a| a.nonce).collect();
        let recorded: Vec<u64> = r["authorizations"].as_array().unwrap().iter().map(|a| a["nonce"].as_u64().unwrap()).collect();
        assert_eq!((nonces[0], &nonces), (tx.nonce, &recorded), "{n}");
        // No record or call boundary takes the boundary, the ordinal after
        // the sender's nonce record and the applied authorization's records.
        let boundary = r["unrecorded_ordinal"].as_u64().unwrap();
        assert!(!tx.calls.iter().any(|c| takes(c, boundary)), "{n}");
        assert!(root.nonce_changes.iter().all(|c| c.ordinal < boundary), "{n}");
        // The shared rules refuse the transaction as it is, and accept it with
        // the self-authorization marked discarded.
        let shared = persist::collect_transaction(tx, &mut Discard).unwrap_err().to_string();
        assert!(shared.contains(BOUNDARY), "{n} {shared}");
        let mut marked = b.clone();
        marked.transaction_traces[0].set_code_authorizations[0].discarded = true;
        assert!(persist::collect_transaction(&marked.transaction_traces[0], &mut Discard).is_ok(), "{n}");
        // ERC-20 v0.7.3 refused the block; v0.7.4 accepts it.
        assert!(rows(&b).is_empty(), "{n}");
        assert_eq!(rows(&with_transfer(&b)), transfer_rows(), "{n}");
        assert_eq!(rows(&with_transfer(&marked)), transfer_rows(), "{n}");
    }
}

#[test]
fn the_unapplicable_self_authorization_shape_is_exact() {
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[0].calls[0]
    }
    // 382,234,350: one self-authorization; 360,021,045: a second, applied
    // authorization of the sender follows it.
    let (single, mixed) = (with_transfer(&of(382_234_350)), with_transfer(&of(360_021_045)));
    let sender = |b: &eth::Block| b.transaction_traces[0].from.clone();
    for (b, boundary) in [(&single, 1975), (&mixed, 125)] {
        assert_eq!(rows(b), transfer_rows());
        // Only the sender's authorization with the transaction's nonce: with
        // the next nonce it would apply, and its records are missing.
        let mut t = b.clone();
        t.transaction_traces[0].set_code_authorizations[0].nonce += 1;
        assert!(fails(&t));
        let mut t = b.clone();
        t.transaction_traces[0].set_code_authorizations[0].authority = Some(vec![0x11; 20]);
        assert!(fails(&t));
        // Exactly one such authorization.
        let mut t = b.clone();
        let again = t.transaction_traces[0].set_code_authorizations[0].clone();
        t.transaction_traces[0].set_code_authorizations.push(again);
        assert!(fails(&t));
        // Exactly one nonce record of the sender from the transaction's nonce
        // to the next.
        let mut t = b.clone();
        let mut twice = root(&mut t).nonce_changes[0].clone();
        twice.ordinal = 0;
        root(&mut t).nonce_changes.push(twice);
        assert!(fails(&t));
        // A record or a call boundary at the boundary still fails the block.
        let mut t = b.clone();
        root(&mut t).gas_changes.push(eth::GasChange {
            ordinal: boundary,
            ..Default::default()
        });
        assert!(fails(&t));
        let mut t = b.clone();
        root(&mut t).end_ordinal = boundary;
        assert!(fails(&t));
        // So does a code record of the sender before it.
        let mut t = b.clone();
        let ordinal = root(&mut t).nonce_changes[0].ordinal - 1;
        root(&mut t).code_changes.push(eth::CodeChange {
            address: sender(b),
            new_hash: vec![9; 32],
            new_code: vec![0xef, 1, 0],
            ordinal,
            ..Default::default()
        });
        assert!(fails(&t));
        // Only of a failed transaction whose root call records no begin
        // ordinal: with one, the shared rules take it as it is.
        let mut t = b.clone();
        root(&mut t).begin_ordinal = boundary;
        assert_eq!(rows(&t), transfer_rows());
        // Nor of a transaction the shared rules accept as it is.
        let mut t = b.clone();
        t.transaction_traces[0].set_code_authorizations[0].discarded = true;
        assert_eq!(rows(&t), transfer_rows());
    }
    // With the sender's applied authorization: its records must follow the
    // sender's nonce record exactly.
    let mut t = mixed.clone();
    root(&mut t).code_changes.clear();
    assert!(fails(&t));
    let mut t = mixed.clone();
    root(&mut t).nonce_changes[1].ordinal = 124;
    root(&mut t).code_changes[0].ordinal = 125;
    assert!(fails(&t));
    // When only another authority's authorization would be left, the block
    // still fails, even with that authorization's records.
    let mut t = mixed.clone();
    let other = vec![0x11; 20];
    t.transaction_traces[0].set_code_authorizations[1].authority = Some(other.clone());
    root(&mut t).nonce_changes[1].address = other.clone();
    root(&mut t).code_changes[0].address = other;
    let mut marked = t.transaction_traces[0].clone();
    marked.set_code_authorizations[0].discarded = true;
    assert!(persist::collect_transaction(&marked, &mut Discard).is_ok());
    assert!(fails(&t));
    // Another refusal of the block stands: a second failed SetCode
    // transaction whose only authorization, of another authority, left no
    // record.
    let mut t = single.clone();
    let mut other = t.transaction_traces[0].clone();
    other.index = 99;
    other.set_code_authorizations[0].authority = Some(vec![0x11; 20]);
    t.transaction_traces.push(other);
    assert!(fails(&t));
}
