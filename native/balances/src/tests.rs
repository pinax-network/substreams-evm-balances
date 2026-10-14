use super::*;
use buffa::Message;
use serde_json::Value;
use std::str::FromStr;

const FIXTURE_BLOCK: &[u8] = include_bytes!("../../../erc20/balances/tests/fixtures/bsc-122260950.pb");
const FIXTURE_ORACLE: &str = include_str!("../../../erc20/balances/tests/fixtures/bsc-122260950.json");
const PROTOTYPE_ROWS: &str = include_str!("../tests/fixtures/bsc-122260950-prototype-native-rows.json");
const SYSTEM_FEE_ADDRESS: &str = "fffffffffffffffffffffffffffffffffffffffe";
const SELF_TRANSFER: &str = include_str!("../tests/fixtures/bsc-127411449-callcode-self-transfer.json");
const FEE_POOL_PAYOUTS: &str = include_str!("../tests/fixtures/bsc-fee-pool-payout-ver3.json");
const GENESIS: &str = include_str!("../tests/fixtures/bsc-0-genesis.json");
const FAILED_BLOB_FEE: &str = include_str!("../tests/fixtures/bsc-42357369-failed-blob-fee.json");
const SELFDESTRUCT_6780: &str = include_str!("../tests/fixtures/eth-20249174-selfdestruct-6780.json");

fn block() -> eth::Block {
    eth::Block {
        ver: 5,
        number: 122260950,
        hash: vec![1; 32],
        detail_level: eth::block::DetailLevel::DetaillevelExtended.into(),
        header: buffa::MessageField::some(eth::BlockHeader {
            number: 122260950,
            parent_hash: vec![2; 32],
            state_root: vec![3; 32],
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn tx(call: eth::Call) -> eth::TransactionTrace {
    eth::TransactionTrace {
        status: eth::TransactionTraceStatus::Succeeded.into(),
        calls: vec![call],
        ..Default::default()
    }
}
fn big(v: u64) -> Option<eth::BigInt> {
    Some(eth::BigInt {
        bytes: v.to_be_bytes().to_vec(),
    })
}
fn change(address: u8, ordinal: u64, old: Option<u64>, new: Option<u64>) -> eth::BalanceChange {
    eth::BalanceChange {
        address: vec![address; 20],
        old_value: old.and_then(big).into(),
        new_value: new.and_then(big).into(),
        ordinal,
        reason: eth::balance_change::Reason::Transfer.into(),
    }
}
fn row(rows: &[Change], address: u8) -> &Change {
    rows.iter().find(|r| r.address == vec![address; 20]).unwrap()
}

#[test]
fn captured_block_matches_all_82_historical_native_rpc_balances() {
    let b = eth::Block::decode_from_slice(FIXTURE_BLOCK).unwrap();
    let oracle: Value = serde_json::from_str(FIXTURE_ORACLE).unwrap();
    assert_eq!(format!("0x{}", hex::encode(&b.hash)), oracle["block_hash"]);
    assert_eq!(b.number, 122260950);
    assert_eq!(b.ver, 5);
    let native: Vec<&Value> = oracle["rpc_checks"].as_array().unwrap().iter().filter(|r| r["contract"] == "").collect();
    assert_eq!(native.len(), 82);
    let events = project(&b).unwrap();
    assert_eq!(events.balances.len(), 82);
    for expected in native {
        let address = hex::decode(expected["address"].as_str().unwrap().trim_start_matches("0x")).unwrap();
        let actual = events.balances.iter().find(|b| b.address == address).unwrap();
        assert_eq!(actual.amount, expected["balance"].as_str().unwrap(), "address {}", expected["address"]);
    }
    // Sorted by account for deterministic sink rows.
    assert!(events.balances.windows(2).all(|w| w[0].address < w[1].address));
}

#[test]
fn captured_block_matches_the_historical_prototype_old_new_and_ordinal_rows() {
    let b = eth::Block::decode_from_slice(FIXTURE_BLOCK).unwrap();
    let prototype: Value = serde_json::from_str(PROTOTYPE_ROWS).unwrap();
    assert_eq!(prototype["block"]["number"], 122260950);
    assert_eq!(format!("0x{}", hex::encode(&b.hash)), prototype["block"]["hash"]);
    assert_eq!(
        format!("0x{}", hex::encode(&b.header.as_option().unwrap().parent_hash)),
        prototype["block"]["parent_hash"]
    );
    let rows = changes(&b).unwrap();
    let expected = prototype["native_rows"].as_array().unwrap();
    assert_eq!(expected.len(), 82);
    assert_eq!(rows.len(), 82);
    for e in expected {
        let address = hex::decode(e["address"].as_str().unwrap().trim_start_matches("0x")).unwrap();
        let actual = rows.iter().find(|r| r.address == address).unwrap();
        assert_eq!(actual.old_amount, e["old_amount"].as_str().unwrap(), "{}", e["address"]);
        assert_eq!(actual.amount, e["amount"].as_str().unwrap(), "{}", e["address"]);
        assert_eq!(actual.ordinal, e["ordinal"].as_u64().unwrap(), "{}", e["address"]);
    }
}

#[test]
fn bsc_fee_reset_wins_over_the_earlier_transaction_fee_credit() {
    // Transaction ordinal 3237 credits the system fee account; the block-level
    // record at ordinal 3244 resets it to zero. Array order puts the block-level
    // record after every transaction, so only ordinal order reproduces RPC.
    let b = eth::Block::decode_from_slice(FIXTURE_BLOCK).unwrap();
    let system = hex::decode(SYSTEM_FEE_ADDRESS).unwrap();
    let credit = b
        .transaction_traces
        .iter()
        .flat_map(|t| &t.calls)
        .flat_map(|c| &c.balance_changes)
        .find(|c| c.address == system && c.ordinal == 3237)
        .unwrap();
    assert_eq!(
        amount(&value(credit.new_value.as_option().map(|v| v.bytes.as_slice())).unwrap()),
        "1675456549641341"
    );
    let reset = b.balance_changes.iter().find(|c| c.address == system).unwrap();
    assert_eq!(reset.ordinal, 3244);
    // The BSC v5 producer records the block-level reset under the ordinary
    // fee-reward reason; REWARD_FEE_RESET is not what this fixture carries.
    assert_eq!(reset.reason, eth::balance_change::Reason::RewardTransactionFee);
    assert_eq!(amount(&value(reset.new_value.as_option().map(|v| v.bytes.as_slice())).unwrap()), "0");
    let rows = changes(&b).unwrap();
    let row = rows.iter().find(|r| r.address == system).unwrap();
    assert_eq!((&*row.old_amount, &*row.amount, row.ordinal), ("0", "0", 3244));
    assert!(row.records >= 2 && row.first_ordinal < 3237);
    // Known-zero finals, precompile-style and burn-looking accounts are emitted.
    let events = project(&b).unwrap();
    assert!(events.balances.iter().any(|b| b.address == system && b.amount == "0"));
    assert!(events
        .balances
        .iter()
        .any(|b| hex::encode(&b.address) == "0000000000000000000000000000000000001000"));
    assert!(events
        .balances
        .iter()
        .any(|b| hex::encode(&b.address) == "000000000000000000000000000000000000dead"));
}

#[test]
fn captured_failed_setcode_transactions_keep_only_gas_effects() {
    for bytes in [
        include_bytes!("../../../erc20/balances/tests/fixtures/bsc-121114122-failed-setcode.pb").as_slice(),
        include_bytes!("../../../erc20/balances/tests/fixtures/bsc-121114153-failed-setcode.pb").as_slice(),
    ] {
        let trace = eth::TransactionTrace::decode_from_slice(bytes).unwrap();
        assert!(persist::is_failed(&trace));
        let root = &trace.calls[0];
        // Independent reduction of the root call's gas-reason records only.
        let mut expected: BTreeMap<Vec<u8>, (String, String)> = BTreeMap::new();
        let mut gas: Vec<_> = root.balance_changes.iter().filter(|c| persist::is_gas_reason(c)).collect();
        gas.sort_by_key(|c| c.ordinal);
        for c in gas {
            let old = amount(&value(c.old_value.as_option().map(|v| v.bytes.as_slice())).unwrap());
            let new = amount(&value(c.new_value.as_option().map(|v| v.bytes.as_slice())).unwrap());
            if old == new {
                continue;
            }
            expected.entry(c.address.clone()).or_insert((old, String::new())).1 = new;
        }
        let mut b = block();
        b.transaction_traces = vec![trace];
        let rows = changes(&b).unwrap();
        assert!(!rows.is_empty());
        assert_eq!(rows.len(), expected.len());
        for r in rows {
            assert_eq!((r.old_amount, r.amount), expected[&r.address]);
        }
    }
}

#[test]
fn takes_last_persisted_value_by_ordinal_and_preserves_first_old() {
    let mut b = block();
    b.balance_changes = vec![change(4, 30, Some(9), Some(0)), change(4, 10, Some(5), Some(9))];
    let rows = changes(&b).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!((&*rows[0].old_amount, &*rows[0].amount), ("5", "0"));
    assert_eq!((rows[0].first_ordinal, rows[0].ordinal, rows[0].records), (10, 30, 2));
    let events = project(&b).unwrap();
    assert_eq!(events.balances[0].amount, "0");
}

#[test]
fn net_zero_changing_account_is_emitted_but_individual_noop_is_not() {
    let mut b = block();
    b.balance_changes = vec![
        change(4, 10, Some(0), Some(5)),
        change(4, 20, Some(5), Some(0)),
        change(6, 30, Some(7), Some(7)),
    ];
    let rows = changes(&b).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!((&*rows[0].old_amount, &*rows[0].amount, rows[0].records), ("0", "0", 2));
    assert_eq!(project(&b).unwrap().balances.len(), 1);
}

#[test]
fn missing_value_messages_inside_an_observed_record_encode_zero() {
    let mut b = block();
    b.balance_changes = vec![change(4, 10, None, Some(7)), change(5, 11, Some(7), None)];
    let rows = changes(&b).unwrap();
    assert_eq!((&*row(&rows, 4).old_amount, &*row(&rows, 4).amount), ("0", "7"));
    assert_eq!((&*row(&rows, 5).old_amount, &*row(&rows, 5).amount), ("7", "0"));
    // A record whose absent and zero values agree is a no-op and never a row.
    b.balance_changes = vec![change(6, 12, None, Some(0))];
    assert!(changes(&b).unwrap().is_empty());
}

#[test]
fn zero_precompile_and_burn_looking_accounts_are_ordinary_accounts() {
    let mut b = block();
    let mut zero = change(0, 10, Some(1), Some(2));
    zero.address = vec![0; 20];
    let mut precompile = change(0, 11, Some(1), Some(2));
    precompile.address = hex::decode("0000000000000000000000000000000000000001").unwrap();
    let mut dead = change(0, 12, Some(1), Some(0));
    dead.address = hex::decode("000000000000000000000000000000000000dead").unwrap();
    b.balance_changes = vec![zero, precompile, dead];
    let events = project(&b).unwrap();
    assert_eq!(events.balances.len(), 3);
    assert_eq!(events.balances[0].address, vec![0; 20]);
    assert_eq!(events.balances[2].amount, "0");
}

#[test]
fn ambiguous_or_discontinuous_producer_data_fails_the_block() {
    let mut b = block();
    b.balance_changes = vec![change(4, 1, Some(0), Some(1)), change(4, 1, Some(1), Some(2))];
    assert!(changes(&b).unwrap_err().to_string().contains("ambiguous"));
    b.balance_changes = vec![change(4, 1, Some(0), Some(1)), change(4, 2, Some(3), Some(4))];
    assert!(changes(&b).unwrap_err().to_string().contains("discontinuous"));
    b.balance_changes = vec![change(4, 0, Some(0), Some(1))];
    assert!(changes(&b).unwrap_err().to_string().contains("ordinal"));
    let mut short = change(4, 1, Some(0), Some(1));
    short.address = vec![4; 19];
    b.balance_changes = vec![short];
    assert!(changes(&b).unwrap_err().to_string().contains("address"));
    let mut wide = change(4, 1, Some(0), Some(1));
    wide.new_value = buffa::MessageField::some(eth::BigInt { bytes: vec![1; 33] });
    b.balance_changes = vec![wide];
    assert!(changes(&b).unwrap_err().to_string().contains("uint256"));
}

#[test]
fn preserves_uint256_max_and_leading_zero_encodings() {
    let mut b = block();
    let mut max = change(4, 1, Some(0), Some(1));
    max.new_value = buffa::MessageField::some(eth::BigInt { bytes: vec![255; 32] });
    let mut padded = change(5, 2, Some(0), Some(1));
    padded.new_value = buffa::MessageField::some(eth::BigInt { bytes: vec![0, 0, 0, 9] });
    b.balance_changes = vec![max, padded];
    let rows = changes(&b).unwrap();
    assert_eq!(
        row(&rows, 4).amount,
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
    );
    assert_eq!(row(&rows, 5).amount, "9");
}

#[test]
fn reverted_frames_failed_transactions_and_system_calls_follow_persistence_rules() {
    let mut b = block();
    let mut reverted = eth::Call {
        index: 1,
        state_reverted: true,
        balance_changes: vec![change(7, 5, Some(0), Some(9))],
        ..Default::default()
    };
    let root = eth::Call {
        index: 0,
        balance_changes: vec![change(4, 3, Some(0), Some(1))],
        ..Default::default()
    };
    let mut ok = tx(root);
    ok.calls.push(std::mem::take(&mut reverted));
    let mut failed = eth::TransactionTrace {
        status: eth::TransactionTraceStatus::Reverted.into(),
        calls: vec![eth::Call {
            state_reverted: true,
            balance_changes: vec![
                eth::BalanceChange {
                    reason: eth::balance_change::Reason::GasBuy.into(),
                    ..change(8, 20, Some(100), Some(90))
                },
                change(9, 21, Some(0), Some(50)),
                eth::BalanceChange {
                    reason: eth::balance_change::Reason::RewardTransactionFee.into(),
                    ..change(10, 22, Some(0), Some(10))
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    failed.from = vec![8; 20];
    b.transaction_traces = vec![ok, failed];
    b.system_calls = vec![
        eth::Call {
            balance_changes: vec![change(11, 40, Some(1), Some(2))],
            ..Default::default()
        },
        eth::Call {
            state_reverted: true,
            balance_changes: vec![change(12, 41, Some(1), Some(2))],
            ..Default::default()
        },
    ];
    b.balance_changes = vec![change(13, 50, Some(0), Some(3))];
    let rows = changes(&b).unwrap();
    let accounts: Vec<u8> = rows.iter().map(|r| r.address[0]).collect();
    assert_eq!(accounts, vec![4, 8, 10, 11, 13]);
    assert_eq!(row(&rows, 8).amount, "90");
}

#[test]
fn value_bearing_calls_without_persisted_records_are_not_balances() {
    let mut b = block();
    b.transaction_traces = vec![tx(eth::Call {
        call_type: eth::CallType::Delegate.into(),
        value: big(5).into(),
        caller: vec![4; 20],
        address: vec![5; 20],
        ..Default::default()
    })];
    assert!(project(&b).unwrap().balances.is_empty());
}

#[test]
fn incomplete_blocks_are_rejected() {
    let mut b = block();
    b.ver = 99;
    b.detail_level = eth::block::DetailLevel::DetaillevelBase.into();
    assert!(project(&b).unwrap_err().to_string().contains("Extended blocks required"));
    b = block();
    b.header = buffa::MessageField::none();
    assert!(project(&b).is_err());
    b = block();
    b.header.as_option_mut().unwrap().number = 1;
    assert!(project(&b).is_err());
    b = block();
    b.hash = vec![1; 31];
    assert!(project(&b).is_err());
    b = block();
    b.number = 0;
    b.header.as_option_mut().unwrap().number = 0;
    b.balance_changes = vec![change(4, 1, Some(0), Some(1))];
    assert!(project(&b).unwrap_err().to_string().contains("genesis"));
    b = block();
    b.transaction_traces = vec![eth::TransactionTrace::default()];
    assert!(project(&b).is_err());
}

#[test]
fn block_version_does_not_gate_valid_or_ambiguous_balance_records() {
    let mut b = block();
    b.transaction_traces = vec![tx(eth::Call {
        balance_changes: vec![change(4, 10, Some(100), Some(80))],
        ..Default::default()
    })];
    b.balance_changes = vec![change(4, 20, Some(80), Some(90))];
    let expected = project(&b).unwrap();
    assert_eq!(expected.balances.len(), 1);
    assert_eq!(expected.balances[0].amount, "90");
    for version in [3, 4, 5, 99] {
        b.ver = version;
        assert_eq!(project(&b).unwrap(), expected, "version {version}");
        b.balance_changes[0].ordinal = 10;
        assert!(project(&b).unwrap_err().to_string().contains("ambiguous"), "version {version}");
        b.balance_changes[0].ordinal = 20;
    }
}

#[test]
fn empty_block_emits_no_rows_and_native_bytes_decode_as_events_without_a_contract() {
    assert!(project(&block()).unwrap().balances.is_empty());
    let native = balances_pb::NativeEvents {
        balances: vec![balances_pb::NativeBalance {
            address: vec![4; 20],
            amount: "7".into(),
        }],
    };
    let bytes = native.encode_to_vec();
    // NativeEvents.balances (tag 1, length), then NativeBalance.address (tag 2):
    // field 1 of a balance is reserved, so there are no contract bytes.
    assert_eq!(&bytes[..4], &[0x0a, 25, 0x12, 20]);
    // The same bytes are `Events` with an absent contract, as before v0.7.0.
    let shared = balances_pb::Events {
        balances: vec![balances_pb::Balance {
            contract: None,
            address: vec![4; 20],
            amount: "7".into(),
        }],
    };
    assert_eq!(shared.encode_to_vec(), bytes);
    assert_eq!(balances_pb::Events::decode_from_slice(bytes.as_slice()).unwrap(), shared);
    // An empty contract is a present field 1, which only ERC-20 rows carry.
    let erc20 = balances_pb::Balance {
        contract: Some(vec![]),
        ..shared.balances[0].clone()
    };
    assert_eq!(erc20.encode_to_vec()[..2], [0x0a, 0]);
}

#[test]
fn records_expose_scope_and_reason_for_the_reason_matrix() {
    let b = eth::Block::decode_from_slice(FIXTURE_BLOCK).unwrap();
    let records = records(&b).unwrap();
    assert!(records.len() > 82);
    let system = hex::decode(SYSTEM_FEE_ADDRESS).unwrap();
    // The BSC v5 producer records the block-level reset under the ordinary
    // fee-reward reason; the fixture carries no REWARD_FEE_RESET record.
    assert!(records
        .iter()
        .any(|r| r.scope == persist::Scope::Block && r.address == system && r.reason == eth::balance_change::Reason::RewardTransactionFee as i32));
    assert!(records
        .iter()
        .any(|r| r.scope == persist::Scope::Tx && r.reason == eth::balance_change::Reason::Transfer as i32));
    let mut matrix: BTreeMap<(String, i32), u64> = BTreeMap::new();
    for r in &records {
        *matrix.entry((format!("{:?}", r.scope), r.reason)).or_default() += 1;
    }
    println!("{matrix:?}");
    assert!(records.iter().all(|r| r.address.len() == 20 && r.ordinal > 0));
}

#[test]
fn failed_transactions_with_unpinned_reasons_fail_closed() {
    let mut b = block();
    let failed = |reason: i32| eth::TransactionTrace {
        status: eth::TransactionTraceStatus::Failed.into(),
        from: vec![8; 20],
        calls: vec![eth::Call {
            state_reverted: true,
            balance_changes: vec![
                eth::BalanceChange {
                    reason: eth::balance_change::Reason::GasBuy.into(),
                    ..change(8, 20, Some(100), Some(90))
                },
                eth::BalanceChange {
                    reason: reason.into(),
                    ..change(9, 21, Some(0), Some(50))
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    // Reverted transfer-like effects are pinned: dropped, gas kept.
    for reason in [
        eth::balance_change::Reason::Transfer,
        eth::balance_change::Reason::TouchAccount,
        eth::balance_change::Reason::Burn,
        eth::balance_change::Reason::SuicideWithdraw,
    ] {
        b.transaction_traces = vec![failed(reason as i32)];
        let rows = changes(&b).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].amount, "90");
    }
    // A blob fee (17) outside a blob transaction, OP-stack mints (18), reverts
    // (19), unknown future reasons and block-level reasons inside a failed root
    // call have no fixture: the block is rejected rather than reduced without
    // the record.
    for reason in [
        17,
        18,
        19,
        20,
        99,
        eth::balance_change::Reason::RewardFeeReset as i32,
        eth::balance_change::Reason::Withdrawal as i32,
    ] {
        b.transaction_traces = vec![failed(reason)];
        assert!(changes(&b).unwrap_err().to_string().contains("pinned persistence"), "reason {reason}");
    }
    // A failed blob transaction pays its blob fee: the record persists with gas.
    let mut blob = failed(17);
    blob.r#type = eth::transaction_trace::Type::TrxTypeBlob.into();
    b.transaction_traces = vec![blob.clone()];
    let rows = changes(&b).unwrap();
    assert_eq!((row(&rows, 8).amount.as_str(), row(&rows, 9).amount.as_str()), ("90", "50"));
    // Other unpinned reasons still fail a blob transaction.
    blob.calls[0].balance_changes[1].reason = 18.into();
    b.transaction_traces = vec![blob];
    assert!(changes(&b).unwrap_err().to_string().contains("pinned persistence"));
    // The same reasons in a successful transaction are ordinary persisted state.
    let mut ok = failed(17);
    ok.status = eth::TransactionTraceStatus::Succeeded.into();
    ok.calls[0].state_reverted = false;
    b.transaction_traces = vec![ok];
    assert_eq!(changes(&b).unwrap().len(), 2);
}

/// WETH9-style wrapping on captured WBNB transactions: the holder's WBNB is an
/// ERC-20 balance and the wrapper contract's BNB is native account state. The
/// wrapper's native balance moves by the event's amount; the holder gets no
/// second native balance for it.
mod wrapper_backing {
    use super::*;
    use substreams::scalar::BigInt;

    const WBNB: &str = "bb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c";
    const DEPOSIT: &str = "e1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c";
    const WITHDRAWAL: &str = "7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65";

    /// The holder and amount of the block's one persisted WBNB `topic0` log.
    fn wrapped(block: &eth::Block, topic0: &str) -> (Vec<u8>, BigInt) {
        let logs: Vec<_> = block
            .transaction_traces
            .iter()
            .filter(|tx| tx.status == eth::TransactionTraceStatus::Succeeded)
            .flat_map(|tx| tx.calls.iter().filter(|c| !c.state_reverted))
            .flat_map(|c| &c.logs)
            .filter(|l| hex::encode(&l.address) == WBNB && l.topics.first().is_some_and(|t| hex::encode(t) == topic0))
            .collect();
        assert_eq!(logs.len(), 1);
        (logs[0].topics[1][12..].to_vec(), BigInt::from_unsigned_bytes_be(&logs[0].data))
    }

    fn delta(old: &str, new: &str) -> BigInt {
        new.parse::<BigInt>().unwrap() - old.parse::<BigInt>().unwrap()
    }

    #[test]
    fn deposit_moves_native_bnb_into_the_wrapper() {
        let block = eth::Block::decode_from_slice(
            include_bytes!("../../../erc20/balances/tests/fixtures/wbnb-mutations/122288015-tx18-deposit-nested-no-transfer.pb").as_slice(),
        )
        .unwrap();
        let (holder, wad) = wrapped(&block, DEPOSIT);
        assert_eq!(wad, BigInt::from(100971252078042364u64));
        let native = changes(&block).unwrap();
        let wrapper = native.iter().find(|r| hex::encode(&r.address) == WBNB).unwrap();
        assert_eq!(delta(&wrapper.old_amount, &wrapper.amount), wad);
        // The holder's own native balance nets to zero (received, then
        // forwarded the BNB it wrapped).
        let holder = native.iter().find(|r| r.address == holder).unwrap();
        assert_eq!((holder.old_amount.as_str(), holder.records), (holder.amount.as_str(), 2));
    }

    #[test]
    fn withdrawal_releases_native_bnb_from_the_wrapper() {
        let block = eth::Block::decode_from_slice(
            include_bytes!("../../../erc20/balances/tests/fixtures/wbnb-mutations/122288035-tx9-withdrawal-no-transfer.pb").as_slice(),
        )
        .unwrap();
        let (_, wad) = wrapped(&block, WITHDRAWAL);
        assert_eq!(wad, BigInt::from(69434307925719935u64));
        let native = changes(&block).unwrap();
        let wrapper = native.iter().find(|r| hex::encode(&r.address) == WBNB).unwrap();
        assert_eq!(delta(&wrapper.old_amount, &wrapper.amount), -wad);
    }

    #[test]
    fn reverted_deposit_leaves_no_wrapper_native_row() {
        let block = eth::Block::decode_from_slice(
            include_bytes!("../../../erc20/balances/tests/fixtures/wbnb-mutations/122288021-tx35-reverted-transaction-deposit.pb").as_slice(),
        )
        .unwrap();
        let native = changes(&block).unwrap();
        assert!(!native.iter().any(|r| hex::encode(&r.address) == WBNB));
        // Only the sender's gas debit and the system fee credit persist.
        assert_eq!(native.len(), 2);
    }
}

fn hex_big(v: &Value) -> Option<eth::BigInt> {
    let bytes = hex::decode(v.as_str().unwrap()).unwrap();
    (!bytes.is_empty()).then_some(eth::BigInt { bytes })
}
fn captured_change(r: &Value) -> eth::BalanceChange {
    eth::BalanceChange {
        address: hex::decode(r["address"].as_str().unwrap()).unwrap(),
        old_value: hex_big(&r["old_value"]).into(),
        new_value: hex_big(&r["new_value"]).into(),
        ordinal: r["ordinal"].as_u64().unwrap(),
        reason: (r["reason"].as_i64().unwrap() as i32).into(),
    }
}
fn captured_call(c: &Value) -> eth::Call {
    eth::Call {
        index: c["index"].as_u64().unwrap() as u32,
        parent_index: c["parent_index"].as_u64().unwrap() as u32,
        depth: c["depth"].as_u64().unwrap() as u32,
        call_type: (c["call_type"].as_i64().unwrap() as i32).into(),
        caller: hex::decode(c["caller"].as_str().unwrap()).unwrap(),
        address: hex::decode(c["address"].as_str().unwrap()).unwrap(),
        value: hex_big(&c["value"]).into(),
        state_reverted: c["state_reverted"].as_bool().unwrap(),
        suicide: c["suicide"].as_bool().unwrap_or(false),
        balance_changes: c["balance_changes"].as_array().unwrap().iter().map(captured_change).collect(),
        ..Default::default()
    }
}
/// The captured transaction of block 127,411,449 whose CALLCODE (call 3) moves
/// value from 0x1a84c890… to itself.
fn captured_self_transfer() -> (eth::Block, Vec<u8>) {
    let f: Value = serde_json::from_str(SELF_TRANSFER).unwrap();
    let calls = f["calls"].as_array().unwrap().iter().map(captured_call).collect();
    let mut b = block();
    b.transaction_traces = vec![eth::TransactionTrace {
        status: eth::TransactionTraceStatus::Succeeded.into(),
        calls,
        ..Default::default()
    }];
    (b, hex::decode("1a84c8905acf4a53048f55535f449e09d51e108e").unwrap())
}

#[test]
fn captured_callcode_self_transfer_pair_is_dropped_and_the_account_chains_across_it() {
    let (b, account) = captured_self_transfer();
    // The pair is persisted (a succeeded transaction, non-reverted calls)...
    assert_eq!(records(&b).unwrap().iter().filter(|r| r.address == account).count(), 4);
    // ...but only the call's value moved through the account: in, then back out.
    let rows = changes(&b).unwrap();
    let row = rows.iter().find(|r| r.address == account).unwrap();
    assert_eq!((row.old_amount.as_str(), row.amount.as_str()), ("0", "0"));
    assert_eq!((row.first_ordinal, row.ordinal, row.records), (5967, 5976, 2));
    // The sender's gas, transfer and refund records are untouched.
    let sender = hex::decode("396df4bcc23da0d53956db81738908e31ce94e0d").unwrap();
    assert_eq!(rows.iter().find(|r| r.address == sender).unwrap().records, 4);
}

#[test]
fn the_self_transfer_shape_is_the_only_continuity_exception() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    // A CALLCODE is only a self-transfer of its own value.
    let (mut b, _) = captured_self_transfer();
    b.transaction_traces[0].calls[2].value = big(1).into();
    assert!(fails(&b));
    // A CALL to another account is a transfer, not a self-transfer.
    let (mut b, _) = captured_self_transfer();
    b.transaction_traces[0].calls[2].call_type = eth::CallType::CALL.into();
    assert!(fails(&b));
    // A DELEGATECALL moves no value.
    let (mut b, _) = captured_self_transfer();
    b.transaction_traces[0].calls[2].call_type = eth::CallType::DELEGATE.into();
    assert!(fails(&b));
    // The two records must be adjacent.
    let (mut b, _) = captured_self_transfer();
    b.transaction_traces[0].calls[2].balance_changes[1].ordinal = 5973;
    assert!(fails(&b));
    // Both must start from the same balance.
    let (mut b, _) = captured_self_transfer();
    b.transaction_traces[0].calls[2].balance_changes[1].old_value = big(1).into();
    assert!(fails(&b));
    // And be transfers.
    let (mut b, _) = captured_self_transfer();
    b.transaction_traces[0].calls[2].balance_changes[1].reason = eth::balance_change::Reason::GasRefund.into();
    assert!(fails(&b));
}

#[test]
fn a_call_to_itself_with_value_is_the_same_self_transfer() {
    let mut b = block();
    b.transaction_traces = vec![tx(eth::Call {
        call_type: eth::CallType::CALL.into(),
        caller: vec![7; 20],
        address: vec![7; 20],
        value: big(3).into(),
        balance_changes: vec![
            change(7, 10, Some(5), Some(2)),
            change(7, 11, Some(5), Some(8)),
            change(7, 12, Some(5), Some(9)),
        ],
        ..Default::default()
    })];
    let rows = changes(&b).unwrap();
    assert_eq!(
        (row(&rows, 7).old_amount.as_str(), row(&rows, 7).amount.as_str(), row(&rows, 7).records),
        ("5", "9", 1)
    );
    // Credit first, then debit, is the same pair.
    b.transaction_traces[0].calls[0].balance_changes = vec![change(7, 10, Some(5), Some(8)), change(7, 11, Some(5), Some(2))];
    assert!(changes(&b).unwrap().is_empty());
    // In a failed transaction the call's records do not persist at all.
    b.transaction_traces[0].status = eth::TransactionTraceStatus::Reverted.into();
    assert!(changes(&b).unwrap().is_empty());
}

/// Blocks 363, 958, 4104, 12,700,000 and 25,401,003 (`Block.ver` 3) as
/// captured from BSC Firehose: the first three whole, the other two from the
/// fee pool's last transaction fee on, with every block-level record and every
/// record of the coinbase.
fn captured_fee_pool_payouts() -> Vec<eth::Block> {
    let f: Value = serde_json::from_str(FEE_POOL_PAYOUTS).unwrap();
    f["blocks"].as_array().unwrap().iter().map(captured_block).collect()
}
/// A captured block of a fixture: its number, `Block.ver`, coinbase,
/// block-level records and transactions; a transaction without a `type` is
/// legacy.
fn captured_block(c: &Value) -> eth::Block {
    let number = c["number"].as_u64().unwrap();
    let mut b = block();
    b.ver = c["ver"].as_i64().unwrap() as i32;
    b.number = number;
    b.header = buffa::MessageField::some(eth::BlockHeader {
        number,
        coinbase: hex::decode(c["coinbase"].as_str().unwrap()).unwrap(),
        ..b.header.as_option().unwrap().clone()
    });
    b.balance_changes = c["block_balance_changes"].as_array().unwrap().iter().map(captured_change).collect();
    b.transaction_traces = c["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| eth::TransactionTrace {
            index: t["index"].as_u64().unwrap() as u32,
            hash: hex::decode(t["hash"].as_str().unwrap()).unwrap(),
            r#type: (t["type"].as_i64().unwrap_or(0) as i32).into(),
            begin_ordinal: t["begin_ordinal"].as_u64().unwrap(),
            end_ordinal: t["end_ordinal"].as_u64().unwrap(),
            status: (t["status"].as_i64().unwrap() as i32).into(),
            calls: t["calls"].as_array().unwrap().iter().map(captured_call).collect(),
            ..Default::default()
        })
        .collect();
    b
}
fn coinbase(b: &eth::Block) -> Vec<u8> {
    b.header.as_option().unwrap().coinbase.clone()
}
fn fee(address: &[u8], ordinal: u64, old: u64, new: u64) -> eth::BalanceChange {
    eth::BalanceChange {
        address: address.to_vec(),
        reason: eth::balance_change::Reason::RewardTransactionFee.into(),
        ..change(0, ordinal, Some(old), Some(new))
    }
}

#[test]
fn captured_ver3_fee_pool_payouts_are_restored_and_chain() {
    let pool = hex::decode(SYSTEM_FEE_ADDRESS).unwrap();
    // Per block: the pool's balance before its payouts, and the system reward
    // the coinbase receives and sends on (1/16 of it).
    let expected = [
        (363, "1765700000000000", "110356250000000"),
        (958, "11392820000000000", "712051250000000"),
        (4104, "2741610000000000", "171350625000000"),
        (12700000, "524137595951089443", "32758599746943090"),
        (25401003, "65907155340083753", "4119197208755234"),
    ];
    let blocks = captured_fee_pool_payouts();
    assert_eq!(blocks.len(), expected.len());
    for (b, (number, before, reward)) in blocks.iter().zip(expected) {
        assert_eq!((b.number, b.ver), (number, 3));
        // The producer recorded the first payout's pool debit as `n -> n`, and
        // the shared rules drop it as a no-op.
        let zero = b.balance_changes.iter().find(|c| c.address == pool && c.old_value == c.new_value).unwrap();
        assert!(records(b).unwrap().iter().all(|r| r.ordinal != zero.ordinal));
        // Restored: the pool from its balance before the payouts to `n`.
        let payouts = fee_pool_payouts(b);
        assert_eq!(payouts.len(), 1, "block {number}");
        assert_eq!(payouts[0].ordinal, zero.ordinal);
        let restored = (
            amount(&value(payouts[0].old_value.as_deref()).unwrap()),
            amount(&value(payouts[0].new_value.as_deref()).unwrap()),
        );
        let n = (BigInt::from_str(before).unwrap() - BigInt::from_str(reward).unwrap()).to_string();
        assert_eq!(restored, (before.to_string(), n));
        // The pool chains through it and pays out everything; the coinbase
        // passes both payouts on: two fee credits, two transfers.
        let rows = changes(b).unwrap();
        let p = rows.iter().find(|r| r.address == pool).unwrap();
        assert_eq!(
            (p.amount.as_str(), p.ordinal),
            ("0", b.balance_changes.iter().filter(|c| c.address == pool).map(|c| c.ordinal).max().unwrap())
        );
        let c = rows.iter().find(|r| r.address == coinbase(b)).unwrap();
        assert_eq!((c.amount.as_str(), c.records), (c.old_amount.as_str(), 4));
    }
    // Block 363 is whole: every account of the block.
    let rows = changes(&blocks[0]).unwrap();
    let got: Vec<(String, &str, &str, u32)> = rows
        .iter()
        .map(|r| (hex::encode(&r.address), r.old_amount.as_str(), r.amount.as_str(), r.records))
        .collect();
    let want = [
        ("0000000000000000000000000000000000001000", "0", "1655343750000000", 1),
        ("0000000000000000000000000000000000001002", "0", "110356250000000", 1),
        ("0000000000000000000000000000000000001006", "0", "100000000000000000000", 1),
        ("446aa6e0dc65690403df3f127750da1322941f3e", "500000000000000000000", "399998234300000000000", 3),
        ("6488aa4d1955ee33403f8ccb1d4de5fb97c7ade2", "0", "0", 4),
        (SYSTEM_FEE_ADDRESS, "0", "0", 3),
    ];
    assert_eq!(got, want.map(|(a, o, n, r)| (a.to_string(), o, n, r)));
}

#[test]
fn the_fee_pool_payout_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    let pool = hex::decode(SYSTEM_FEE_ADDRESS).unwrap();
    let block_363 = || captured_fee_pool_payouts().swap_remove(0);
    let block_958 = || captured_fee_pool_payouts().swap_remove(1);
    // Block-level records of block 363: the pool's `n -> n` at ordinal 17, the
    // coinbase's credit at 18, the pool's last payout at 27 and its credit at
    // 28. Block 958 skips an ordinal: 109, 111, 120, 121; its system reward
    // transaction begins at 112.
    assert_eq!(block_363().balance_changes.iter().map(|c| c.ordinal).collect::<Vec<_>>(), [17, 18, 27, 28]);
    assert_eq!(block_958().balance_changes.iter().map(|c| c.ordinal).collect::<Vec<_>>(), [109, 111, 120, 121]);
    assert_eq!(block_958().transaction_traces[2].begin_ordinal, 112);
    // The credit must be the next block-level record...
    let mut b = block_958();
    b.balance_changes.push(change(4, 110, Some(0), Some(1)));
    assert!(fails(&b));
    // ...before any transaction starts...
    let mut b = block_958();
    b.transaction_traces[2].begin_ordinal = 110;
    assert!(fails(&b));
    // ...and a fee credit...
    let mut b = block_363();
    b.balance_changes[1].reason = eth::balance_change::Reason::Transfer.into();
    assert!(fails(&b));
    // ...of the block's coinbase.
    let mut b = block_363();
    let header = b.header.as_option().unwrap().clone();
    b.header = buffa::MessageField::some(eth::BlockHeader {
        coinbase: vec![9; 20],
        ..header
    });
    assert!(fails(&b));
    // The pool's record must be the `n -> n` one.
    let mut b = block_363();
    b.balance_changes[0].old_value = big(1).into();
    assert!(fails(&b));
    // Restored, the debit still has to chain: the pool's previous balance must
    // be `n + r`.
    let mut b = block_363();
    let credit = b.transaction_traces[0].calls[0].balance_changes.iter_mut().find(|c| c.address == pool).unwrap();
    credit.new_value = big(1765700000000016).into();
    assert!(fails(&b));

    // The credit must be 1/16 of the pool. Here n + r is the pool's balance
    // either way, so only that rule tells the shapes apart.
    let base = vec![0xc; 20];
    let mut b = block();
    b.header = buffa::MessageField::some(eth::BlockHeader {
        coinbase: base.clone(),
        ..b.header.as_option().unwrap().clone()
    });
    b.transaction_traces = vec![tx(eth::Call {
        balance_changes: vec![fee(&pool, 5, 0, 32)],
        ..Default::default()
    })];
    b.balance_changes = vec![fee(&pool, 10, 30, 30), fee(&base, 11, 0, 2), fee(&pool, 20, 30, 0), fee(&base, 21, 2, 32)];
    let rows = changes(&b).unwrap();
    assert_eq!(
        (
            row_of(&rows, &pool).old_amount.as_str(),
            row_of(&rows, &pool).amount.as_str(),
            row_of(&rows, &pool).records
        ),
        ("0", "0", 3)
    );
    assert_eq!(row_of(&rows, &base).amount, "32");
    b.balance_changes = vec![fee(&pool, 10, 29, 29), fee(&base, 11, 0, 3), fee(&pool, 20, 29, 0), fee(&base, 21, 3, 32)];
    assert!(fails(&b));
}
fn row_of<'a>(rows: &'a [Change], address: &[u8]) -> &'a Change {
    rows.iter().find(|r| r.address == address).unwrap()
}

/// BSC block 0 as captured from Firehose: one synthetic genesis transaction
/// whose call allocates three balances (its code changes are omitted).
fn captured_genesis() -> eth::Block {
    let f: Value = serde_json::from_str(GENESIS).unwrap();
    let bytes = |key: &str| hex::decode(f[key].as_str().unwrap()).unwrap();
    let mut b = block();
    b.ver = f["block_ver"].as_i64().unwrap() as i32;
    b.number = 0;
    b.hash = bytes("block_hash");
    b.header = buffa::MessageField::some(eth::BlockHeader {
        number: 0,
        parent_hash: bytes("parent_hash"),
        state_root: bytes("state_root"),
        coinbase: bytes("coinbase"),
        ..Default::default()
    });
    b.transaction_traces = vec![eth::TransactionTrace {
        hash: bytes("tx_hash"),
        status: (f["tx_status"].as_i64().unwrap() as i32).into(),
        from: bytes("tx_from"),
        to: bytes("tx_to"),
        calls: f["calls"].as_array().unwrap().iter().map(captured_call).collect(),
        ..Default::default()
    }];
    b
}

#[test]
fn captured_bsc_genesis_allocations_are_the_rows_of_block_0() {
    let events = project(&captured_genesis()).unwrap();
    let rows: Vec<(String, &str)> = events.balances.iter().map(|r| (hex::encode(&r.address), r.amount.as_str())).collect();
    assert_eq!(
        rows,
        [
            ("0000000000000000000000000000000000001004".to_string(), "176405560900000000000000000"),
            ("446aa6e0dc65690403df3f127750da1322941f3e".to_string(), "500000000000000000000"),
            ("b005741528b86f5952469d80a8614591e3c5b632".to_string(), "500000000000000000000"),
        ]
    );
}

#[test]
fn the_genesis_block_holds_only_allocations_from_zero() {
    fn refused(b: &eth::Block) -> bool {
        project(b).unwrap_err().to_string().contains("not a genesis allocation")
    }
    // Another reason...
    let mut b = captured_genesis();
    b.transaction_traces[0].calls[0].balance_changes[0].reason = eth::balance_change::Reason::Transfer.into();
    assert!(refused(&b));
    // ...an allocation that does not start from zero...
    let mut b = captured_genesis();
    b.transaction_traces[0].calls[0].balance_changes[0].old_value = big(1).into();
    assert!(refused(&b));
    // ...or any other record in block 0 refuses it.
    let mut b = captured_genesis();
    b.balance_changes = vec![change(4, 30, Some(0), Some(1))];
    assert!(refused(&b));
}

/// Block 42,357,369 as captured from BSC Firehose: the failed blob transaction
/// `0xbe965c35…` and every later transaction whole, and every block-level
/// record.
fn captured_failed_blob_fee() -> eth::Block {
    captured_block(&serde_json::from_str(FAILED_BLOB_FEE).unwrap())
}

#[test]
fn captured_failed_blob_transaction_pays_its_blob_fee_to_the_fee_pool() {
    let b = captured_failed_blob_fee();
    let pool = hex::decode(SYSTEM_FEE_ADDRESS).unwrap();
    let blob_fee = eth::balance_change::Reason::RewardBlobFee as i32;
    // The blob fee persists like the failed transaction's gas.
    let records = records(&b).unwrap();
    let fee = records.iter().find(|r| r.reason == blob_fee).unwrap();
    assert_eq!(
        (fee.address.as_slice(), fee.ordinal, fee.scope),
        (pool.as_slice(), 7575, persist::Scope::TxFailedPersistent)
    );
    let rows = changes(&b).unwrap();
    // The pool: the transaction fee, the blob fee of 131,072 wei, then the
    // block-level payout of the whole pool to the coinbase.
    let row = row_of(&rows, &pool);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
        ("22684120973748330", "0", 7574, 7583, 3)
    );
    // The coinbase receives the pool, blob fee included, and sends it on.
    let validator = row_of(&rows, &coinbase(&b));
    assert_eq!(
        (validator.old_amount.as_str(), validator.amount.as_str(), validator.records),
        ("10000000000000000", "10000000000000000", 2)
    );
    // The sender keeps only its gas effects.
    let sender = row_of(&rows, &hex::decode("b4f1e5db68512f06d378b5e02e7ebbe7d2c705ef").unwrap());
    assert_eq!(
        (sender.old_amount.as_str(), sender.amount.as_str(), sender.records),
        ("35498221136158448", "35477008136027376", 2)
    );
    // Without the blob fee the pool's payout does not chain.
    let mut without = b.clone();
    without.transaction_traces[0].calls[0].balance_changes.retain(|c| c.reason.to_i32() != blob_fee);
    assert!(changes(&without).unwrap_err().to_string().contains("discontinuous"));
    // Only a blob transaction pays a blob fee.
    let mut not_blob = b.clone();
    not_blob.transaction_traces[0].r#type = eth::transaction_trace::Type::TrxTypeDynamicFee.into();
    assert!(changes(&not_blob).unwrap_err().to_string().contains("pinned persistence"));
}

/// Ethereum block 20,249,174 as captured from Firehose: transaction 87
/// (`0x16bb56d4…`) whole. Its call 23, a DELEGATECALL from `0x0000000000bb3d7b…`,
/// self-destructs that account after Cancun (EIP-6780).
fn captured_selfdestruct() -> eth::Block {
    captured_block(&serde_json::from_str(SELFDESTRUCT_6780).unwrap())
}
const DESTROYED: &str = "0000000000bb3d7b871e07631585786819c923f5";
const BENEFICIARY: &str = "475f8f47efc5e459e7f23b6082fd64a5bb6a215f";

#[test]
fn captured_eip6780_selfdestruct_debit_is_restored_and_chains() {
    let b = captured_selfdestruct();
    let (destroyed, beneficiary) = (hex::decode(DESTROYED).unwrap(), hex::decode(BENEFICIARY).unwrap());
    // The producer records only the beneficiary's credit of the whole balance.
    let suicide = b.transaction_traces[0].calls.iter().find(|c| c.suicide).unwrap();
    assert_eq!((suicide.index, suicide.call_type.as_known()), (23, Some(eth::CallType::DELEGATE)));
    assert!(!records(&b).unwrap().iter().any(|r| r.address == destroyed && r.ordinal == 9099));
    let rows = changes(&b).unwrap();
    // The account chains through the restored debit (0.809968634248451551 ETH
    // to zero at ordinal 9099) into the next credit, which starts from zero.
    let row = row_of(&rows, &destroyed);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
        ("0", "200000000000000000", 8956, 9149, 11)
    );
    let row = row_of(&rows, &beneficiary);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.records),
        ("200000000000000000", "9968634248451551", 2)
    );
}

#[test]
fn the_selfdestruct_debit_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    fn suicide(b: &mut eth::Block) -> &mut eth::Call {
        b.transaction_traces[0].calls.iter_mut().find(|c| c.index == 23).unwrap()
    }
    // Only a self-destructing call.
    let mut b = captured_selfdestruct();
    suicide(&mut b).suicide = false;
    assert!(fails(&b));
    // Only the beneficiary's self-destruct credit.
    let mut b = captured_selfdestruct();
    suicide(&mut b).balance_changes[0].reason = eth::balance_change::Reason::Transfer.into();
    assert!(fails(&b));
    // The destroyed account is the DELEGATECALL's caller, not the code address.
    let mut b = captured_selfdestruct();
    suicide(&mut b).call_type = eth::CallType::CALL.into();
    assert!(fails(&b));
    // A credit of another amount does not chain.
    let mut b = captured_selfdestruct();
    suicide(&mut b).balance_changes[0].new_value = big(1_009_968_634_248_451_550).into();
    assert!(changes(&b).is_err());
    // A credit to the destroyed account itself is not a transfer away.
    let mut b = captured_selfdestruct();
    suicide(&mut b).balance_changes[0].address = hex::decode(DESTROYED).unwrap();
    assert!(changes(&b).is_err());
    // A reverted self-destruct moves nothing: its credit does not persist either.
    let mut b = captured_selfdestruct();
    suicide(&mut b).state_reverted = true;
    assert!(fails(&b));
    // When the producer records the debit itself, it is used, not doubled.
    let mut b = captured_selfdestruct();
    let withdraw = eth::BalanceChange {
        address: hex::decode(DESTROYED).unwrap(),
        reason: eth::balance_change::Reason::SuicideWithdraw.into(),
        ..change(0, 9098, Some(809_968_634_248_451_551), Some(0))
    };
    suicide(&mut b).balance_changes.insert(0, withdraw);
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &hex::decode(DESTROYED).unwrap());
    assert_eq!((row.amount.as_str(), row.records), ("200000000000000000", 11));
}

#[test]
fn a_block_over_the_default_element_budget_decodes() {
    // An empty transaction trace is 2 bytes on the wire and its full size in
    // the decoded block, so this small block exceeds buffa's default budget.
    let count = buffa::DEFAULT_ELEMENT_MEMORY_LIMIT / std::mem::size_of::<eth::TransactionTrace>() + 1;
    let block = eth::Block {
        transaction_traces: vec![eth::TransactionTrace::default(); count],
        ..Default::default()
    };
    let bytes = block.encode_to_vec();
    assert!(matches!(
        eth::Block::decode_from_slice(&bytes),
        Err(buffa::DecodeError::ElementMemoryLimitExceeded)
    ));
    assert_eq!(decode_block(&bytes).unwrap().transaction_traces.len(), count);
}
