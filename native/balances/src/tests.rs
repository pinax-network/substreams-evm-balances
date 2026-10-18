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
const SELFDESTRUCT_TO_SELF: &str = include_str!("../tests/fixtures/eth-19481732-selfdestruct-to-self.json");
const SELFDESTRUCT_CREATED_CREDIT: &str = include_str!("../tests/fixtures/eth-25741008-selfdestruct-created-credit.json");
const SELFDESTRUCT_CREATION_TX: &str = include_str!("../tests/fixtures/eth-25747303-selfdestruct-creation-tx.json");
const SELFDESTRUCT_FOUR_CREDITS: &str = include_str!("../tests/fixtures/eth-25869452-selfdestruct-four-credits.json");
const POLYGON_FAILED_BASE_FEE: &str = include_str!("../tests/fixtures/polygon-95537024-failed-base-fee.json");
const SELFDESTRUCT_TRANSFER_DURING_EXECUTION: &str = include_str!("../tests/fixtures/eth-25920498-selfdestruct-transfer-during-execution.json");
const ARBITRUM_FAILED_FEE: &str = include_str!("../tests/fixtures/arbitrum-150000072-failed-fee-transfers.json");
const ARBITRUM_FAILED_FEE_TIP: &str = include_str!("../tests/fixtures/arbitrum-514857274-failed-fee-tip.json");
const POLYGON_SELFDESTRUCT_CREATED: &str = include_str!("../tests/fixtures/polygon-70000124-selfdestruct-created.json");
const SELFDESTRUCT_CREATED_ROOT: &str = include_str!("../tests/fixtures/eth-19500178-selfdestruct-created-root.json");
const HYPEREVM_SELF_TRANSFER: &str = include_str!("../tests/fixtures/hyperevm-44000001-self-transfer.json");
const POLYGON_STATE_SYNC: &str = include_str!("../tests/fixtures/polygon-70044000-state-sync.json");
const POLYGON_STATE_SYNC_DEPOSITS: &str = include_str!("../tests/fixtures/polygon-20007040-state-sync.json");
const ARBITRUM_STALE_WITHDRAW: &str = include_str!("../tests/fixtures/arbitrum-250005039-stale-selfdestruct-withdraw.json");
const ARBITRUM_STALE_WITHDRAW_NEW_BENEFICIARY: &str = include_str!("../tests/fixtures/arbitrum-200029376-stale-selfdestruct-withdraw.json");
const ARBITRUM_STALE_WITHDRAW_NESTED: &str = include_str!("../tests/fixtures/arbitrum-450014091-stale-selfdestruct-withdraw.json");
const ARBITRUM_STALE_WITHDRAW_BEFORE_CANCUN: &str = include_str!("../tests/fixtures/arbitrum-187009069-stale-selfdestruct-withdraw.json");
const BASE_FAILED_DEPOSIT: &str = include_str!("../tests/fixtures/base-5044695-failed-deposit-mint.json");
const BASE_FAILED_DEPOSIT_REVERTED_CALL: &str = include_str!("../tests/fixtures/base-7092606-failed-deposit-mint-reverted-call.json");
const BASE_FEE_VAULT_CREDIT: &str = include_str!("../tests/fixtures/base-50024530-fee-vault-stale-credit.json");
const BASE_FEE_VAULT_CREDITS: &str = include_str!("../tests/fixtures/base-50326929-fee-vault-stale-credits.json");
const ARBITRUM_FAILED_RETRY: &str = include_str!("../tests/fixtures/arbitrum-52067707-failed-retry.json");
const ARBITRUM_FAILED_RETRY_VALUE: &str = include_str!("../tests/fixtures/arbitrum-314259052-failed-retry-value.json");
const ARBITRUM_FAILED_RETRY_OUT_OF_GAS: &str = include_str!("../tests/fixtures/arbitrum-511131266-failed-retry-out-of-gas.json");

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
    // A blob fee (17) outside a blob transaction, an INCREASE_MINT (18) outside
    // a failed deposit's exact shape (here: no deposit), reverts (19), unknown
    // future reasons and block-level reasons inside a failed root call have no
    // fixture: the block is rejected rather than reduced without the record.
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
    b.transaction_traces = vec![blob.clone()];
    assert!(changes(&b).unwrap_err().to_string().contains("pinned persistence"));
    // A failed deposit's mint persists only in its exact shape (see
    // `the_failed_deposit_mint_shape_is_exact`). Here the mint (ordinal 21) is
    // a positive credit between the transaction's begin (19) and the
    // execution's (30), and the gas purchase moves out of that interval to
    // ordinal 31, during the execution, so only the credited account decides:
    // account 9 is not the sender (account 8), and the block is refused...
    let mut deposit = blob;
    deposit.r#type = eth::transaction_trace::Type::TrxTypeOptimismDeposit.into();
    (deposit.begin_ordinal, deposit.end_ordinal) = (19, 33);
    (deposit.calls[0].begin_ordinal, deposit.calls[0].end_ordinal) = (30, 32);
    deposit.calls[0].balance_changes[0].ordinal = 31;
    b.transaction_traces = vec![deposit.clone()];
    assert!(changes(&b).unwrap_err().to_string().contains("pinned persistence"));
    // ...while the same mint of the sender, from 50 to 100 wei before its
    // gas purchase from 100 to 90 wei, persists with it...
    deposit.calls[0].balance_changes[1] = eth::BalanceChange {
        reason: eth::balance_change::Reason::IncreaseMint.into(),
        ..change(8, 21, Some(50), Some(100))
    };
    b.transaction_traces = vec![deposit.clone()];
    let rows = changes(&b).unwrap();
    assert_eq!(rows.len(), 1);
    let sender = row(&rows, 8);
    assert_eq!(
        (
            sender.old_amount.as_str(),
            sender.amount.as_str(),
            sender.first_ordinal,
            sender.ordinal,
            sender.records
        ),
        ("50", "90", 21, 31, 2)
    );
    // ...until the gas purchase moves into the interval, before or after the
    // mint: a deposit buys no gas.
    for ordinal in [20, 29] {
        deposit.calls[0].balance_changes[0].ordinal = ordinal;
        b.transaction_traces = vec![deposit.clone()];
        assert!(changes(&b).unwrap_err().to_string().contains("pinned persistence"), "gas purchase at {ordinal}");
    }
    // The same reasons in a successful transaction are ordinary persisted state.
    let mut ok = failed(17);
    ok.status = eth::TransactionTraceStatus::Succeeded.into();
    ok.calls[0].state_reverted = false;
    b.transaction_traces = vec![ok];
    assert_eq!(changes(&b).unwrap().len(), 2);
}

/// A failed transaction in the ordinal layout of the producers of `Block.ver`
/// 4 and 5: the transaction from 10 to 40, the sender's (account 8) `GAS_BUY`
/// at 12, the root call from 14 to 30 with the reverted value transfer to
/// account 9 at 16 and 17, then the sender's `GAS_REFUND` at 32 and the fee
/// credit of account 10 at 33.
fn failed_with_execution_window() -> eth::Block {
    use eth::balance_change::Reason;
    let with = |reason: Reason, c: eth::BalanceChange| eth::BalanceChange { reason: reason.into(), ..c };
    let mut b = block();
    b.transaction_traces = vec![eth::TransactionTrace {
        status: eth::TransactionTraceStatus::Reverted.into(),
        from: vec![8; 20],
        to: vec![9; 20],
        begin_ordinal: 10,
        end_ordinal: 40,
        gas_used: 3,
        calls: vec![eth::Call {
            state_reverted: true,
            begin_ordinal: 14,
            end_ordinal: 30,
            balance_changes: vec![
                with(Reason::GasBuy, change(8, 12, Some(100), Some(90))),
                change(8, 16, Some(90), Some(80)),
                change(9, 17, Some(0), Some(10)),
                with(Reason::GasRefund, change(8, 32, Some(90), Some(95))),
                with(Reason::RewardTransactionFee, change(10, 33, Some(0), Some(5))),
            ],
            ..Default::default()
        }],
        ..Default::default()
    }];
    b
}

#[test]
fn a_failed_transaction_s_record_outside_its_execution_fails_unless_pinned() {
    use eth::balance_change::Reason;
    const AFTER: &str = "after its execution without pinned persistence semantics";
    const BEFORE: &str = "before its execution without pinned persistence semantics";
    fn refused(b: &eth::Block, message: &str) -> bool {
        changes(b).unwrap_err().to_string().contains(message)
    }
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[0].calls[0]
    }
    fn with(b: &eth::Block, c: eth::BalanceChange) -> eth::Block {
        let mut b = b.clone();
        root(&mut b).balance_changes.push(c);
        b
    }
    // The rows without account 11, which only the added records touch.
    fn unmoved(b: &eth::Block) -> bool {
        let rows = changes(b).unwrap();
        rows.len() == 2 && (row(&rows, 8).old_amount.as_str(), row(&rows, 8).amount.as_str(), row(&rows, 10).amount.as_str()) == ("100", "95", "5")
    }
    for status in [eth::TransactionTraceStatus::Failed, eth::TransactionTraceStatus::Reverted] {
        let mut b = failed_with_execution_window();
        b.transaction_traces[0].status = status.into();
        // The gas records persist and the reverted transfer does not.
        assert!(unmoved(&b));
        // A credit after the execution of an account without another record
        // in the block would chain unseen: the shared rules drop it...
        let credit = change(11, 34, Some(0), Some(7));
        let after = with(&b, credit.clone());
        assert!(!records(&after).unwrap().iter().any(|r| r.address == vec![11; 20]));
        // ...so the block fails, also past the transaction's end, for a debit
        // of the sender after its refund and for every reason that reverts
        // with the execution frame, or with a value past uint256.
        assert!(refused(&after, AFTER), "{status:?}");
        assert!(refused(&with(&b, change(11, 45, Some(0), Some(7))), AFTER));
        assert!(refused(&with(&b, change(8, 34, Some(95), Some(94))), AFTER));
        for reason in [
            Reason::Transfer,
            Reason::TouchAccount,
            Reason::SuicideRefund,
            Reason::CallBalanceOverride,
            Reason::SuicideWithdraw,
            Reason::Burn,
        ] {
            let c = eth::BalanceChange {
                reason: reason.into(),
                ..credit.clone()
            };
            assert!(refused(&with(&b, c), AFTER), "{reason:?}");
        }
        let mut wide = credit.clone();
        wide.new_value = buffa::MessageField::some(eth::BigInt { bytes: vec![1; 33] });
        assert!(refused(&with(&b, wide), AFTER));
        // Before the execution: after the transaction's begin, or before it.
        for ordinal in [13, 11, 5] {
            assert!(refused(&with(&b, change(11, ordinal, Some(0), Some(7))), BEFORE), "ordinal {ordinal}");
        }
        // During the execution, its begin and end ordinals included, and at
        // ordinal 0, the record reverts with the frame; a no-op moves nothing.
        for ordinal in [0, 14, 20, 30] {
            assert!(unmoved(&with(&b, change(11, ordinal, Some(0), Some(7)))), "ordinal {ordinal}");
        }
        assert!(unmoved(&with(&b, change(11, 34, Some(7), Some(7)))));
        // A pinned record after the execution persists: Polygon's base-fee
        // BURN of gas_used * base_fee (3 * 2 wei). Another amount fails.
        let mut fee = b.clone();
        fee.header.as_option_mut().unwrap().base_fee_per_gas = big(2).into();
        let burn = |new| eth::BalanceChange {
            reason: Reason::Burn.into(),
            ..change(11, 34, Some(0), Some(new))
        };
        let rows = changes(&with(&fee, burn(6))).unwrap();
        assert_eq!((rows.len(), row(&rows, 11).amount.as_str()), (3, "6"));
        assert!(refused(&with(&fee, burn(7)), AFTER));
        // Only the exact record persists: not another of the account at the
        // same ordinal and reason with other values, nor an equal block-level
        // record.
        let twin = eth::BalanceChange {
            reason: Reason::Burn.into(),
            ..change(11, 34, Some(6), Some(13))
        };
        assert!(refused(&with(&with(&fee, burn(6)), twin), AFTER));
        let mut block_level = after.clone();
        block_level.balance_changes = vec![credit.clone()];
        assert!(refused(&block_level, AFTER));
        // The producers of `Block.ver` 3 record the root call's begin as 0, so
        // no window lies before the execution.
        let mut ver3 = with(&b, change(11, 13, Some(0), Some(7)));
        root(&mut ver3).begin_ordinal = 0;
        assert!(unmoved(&ver3));
        // A root call without an end ordinal cannot place a record that does
        // not persist, the reverted transfer included...
        let mut open = with(&b, credit.clone());
        root(&mut open).end_ordinal = 0;
        assert!(refused(&open, "missing end ordinal cannot place"));
        let mut open = b.clone();
        root(&mut open).end_ordinal = 0;
        assert!(refused(&open, "missing end ordinal cannot place"));
        // ...unless its records all persist...
        root(&mut open).balance_changes.retain(persist::is_gas_reason);
        assert!(unmoved(&open));
        // ...or the transaction has no ordinals either, as synthetic traces.
        let mut bare = with(&b, credit.clone());
        root(&mut bare).end_ordinal = 0;
        (bare.transaction_traces[0].begin_ordinal, bare.transaction_traces[0].end_ordinal) = (0, 0);
        assert!(unmoved(&bare));
        // A succeeded transaction persists the record.
        let mut ok = with(&b, credit.clone());
        ok.transaction_traces[0].status = eth::TransactionTraceStatus::Succeeded.into();
        root(&mut ok).state_reverted = false;
        root(&mut ok).balance_changes.retain(|c| c.ordinal != 16 && c.ordinal != 17);
        assert_eq!(row(&changes(&ok).unwrap(), 11).amount, "7");
    }
}

/// The records of a block's failed transactions' root calls that move a
/// balance outside the execution, as counts after it, before it and in a root
/// call without an end ordinal (a fixture captured without call ordinals);
/// each persists as recorded.
fn persisted_outside_execution(b: &eth::Block) -> [usize; 3] {
    let records = records(b).unwrap();
    let mut counts = [0; 3];
    for tx in b.transaction_traces.iter().filter(|t| persist::is_failed(t)) {
        let root = &tx.calls[0];
        for c in root.balance_changes.iter().filter(|c| value_change(c).is_none_or(|m| !m.is_zero())) {
            let window = if root.end_ordinal == 0 {
                2
            } else if c.ordinal > root.end_ordinal {
                0
            } else if c.ordinal > 0 && c.ordinal < root.begin_ordinal {
                1
            } else {
                continue;
            };
            let kept = records.iter().any(|r| {
                r.scope == persist::Scope::TxFailedPersistent
                    && r.address == c.address
                    && r.ordinal == c.ordinal
                    && r.new_value.as_deref() == c.new_value.as_option().map(|v| v.bytes.as_slice())
            });
            assert!(kept, "block {} tx {} ordinal {}", b.number, tx.index, c.ordinal);
            counts[window] += 1;
        }
    }
    counts
}

#[test]
fn captured_failed_transactions_persist_every_record_outside_their_execution() {
    use eth::balance_change::Reason;
    // Every captured failed-transaction shape: BNB Chain's blob fee and gas
    // records, Polygon's base-fee burn, Arbitrum's fee transfers, tip,
    // submission and retry settlements and SetCode transactions, and the
    // OP-stack deposit mints of Base, Optimism and Unichain.
    let fixtures = |f: &str| captured_block(&serde_json::from_str(f).unwrap());
    let mut blocks = vec![
        eth::Block::decode_from_slice(FIXTURE_BLOCK).unwrap(),
        captured_failed_blob_fee(),
        captured_polygon_failed_base_fee(),
        captured_arbitrum_failed_fee(),
        captured_arbitrum_failed_fee_tip(),
        captured_arbitrum_failed_submission_without_records(),
        captured_arbitrum_failed_submission_mint(),
        captured_arbitrum_failed_submission_call_value(),
        captured_arbitrum_failed_retry(),
        captured_arbitrum_failed_retry_value(),
        captured_arbitrum_failed_retry_out_of_gas(),
        captured_base_failed_deposit(),
        captured_base_failed_deposit_reverted_call(),
        fixtures(OPTIMISM_FAILED_DEPOSIT),
        fixtures(OPTIMISM_FAILED_DEPOSIT_REVERTED_CALL),
        fixtures(OPTIMISM_V5_FAILED_DEPOSIT),
        fixtures(UNICHAIN_FAILED_DEPOSIT_REVERTED_CALL),
    ];
    blocks.extend(captured_arbitrum_failed_setcode());
    let mut totals = [0; 3];
    for b in &blocks {
        assert!(changes(b).is_ok(), "block {}", b.number);
        let counts = persisted_outside_execution(b);
        (0..3).for_each(|i| totals[i] += counts[i]);
    }
    // 44 records after the execution (gas refunds and fee credits, Arbitrum's
    // fee transfers and retry settlements, Polygon's burn), 8 before it (gas
    // purchases and deposit mints) and the 4 records of BNB Chain's failed
    // blob transaction, captured without call ordinals.
    assert_eq!(totals, [44, 8, 4]);

    // Unpinned, the same records fail the block where they would have chained
    // unseen. Arbitrum block 150,000,072 up to transaction 7, whose fee
    // credits at 454 and 455 are its accounts' last: one wei more no longer
    // adds up to the gas paid...
    let mut b = captured_arbitrum_failed_fee();
    b.transaction_traces[4].calls[0].balance_changes[2].new_value = wei(2_866_154_730_594_200_000_001).into();
    assert!(changes(&b).unwrap_err().to_string().contains("discontinuous"));
    b.transaction_traces.truncate(5);
    assert!(!records(&b).unwrap().iter().any(|r| r.ordinal == 454 || r.ordinal == 455));
    assert!(changes(&b).unwrap_err().to_string().contains("after its execution without pinned persistence"));
    // ...Polygon block 95,537,024 up to transaction 4, whose base-fee burn at
    // 111 moves one wei more than gas_used * base_fee...
    let mut b = captured_polygon_failed_base_fee();
    b.transaction_traces[1].calls[0].balance_changes[2].new_value = wei(62_430_004_462_142_801_726_820_036).into();
    assert!(changes(&b).unwrap_err().to_string().contains("discontinuous"));
    b.transaction_traces.truncate(2);
    assert!(changes(&b).unwrap_err().to_string().contains("after its execution without pinned persistence"));
    // ...and Base block 5,044,695, whose failed deposit's mint at 25, before
    // the execution, is recorded as a TRANSFER.
    let mut b = captured_base_failed_deposit();
    b.transaction_traces[0].calls[0].balance_changes[0].reason = Reason::Transfer.into();
    assert!(records(&b).unwrap().is_empty());
    assert!(changes(&b).unwrap_err().to_string().contains("before its execution without pinned persistence"));
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
        begin_ordinal: c["begin_ordinal"].as_u64().unwrap_or(0),
        end_ordinal: c["end_ordinal"].as_u64().unwrap_or(0),
        balance_changes: c["balance_changes"].as_array().unwrap().iter().map(captured_change).collect(),
        // Only the fixtures that need them keep account creations.
        account_creations: c["account_creations"].as_array().map_or_else(Vec::new, |creations| {
            creations
                .iter()
                .map(|a| eth::AccountCreation {
                    account: hex::decode(a["account"].as_str().unwrap()).unwrap(),
                    ordinal: a["ordinal"].as_u64().unwrap(),
                })
                .collect()
        }),
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

/// HyperEVM block 44,000,001 as captured from Firehose: transaction 9
/// (`0xbda5325e…`) whole. Its call 5 is a CALL of `0x20f6ee51…` to itself with
/// 0.00006 HYPE.
fn captured_hyperevm_self_transfer() -> eth::Block {
    captured_block(&serde_json::from_str(HYPEREVM_SELF_TRANSFER).unwrap())
}
const HYPEREVM_SELF: &str = "20f6ee51340adeed01a59b0e65cb3703f3dc860c";
fn hyperevm_self_call(b: &mut eth::Block) -> &mut eth::Call {
    b.transaction_traces[0].calls.iter_mut().find(|c| c.index == 5).unwrap()
}

#[test]
fn captured_hyperevm_self_transfer_pair_is_dropped_and_the_account_chains_across_it() {
    let mut b = captured_hyperevm_self_transfer();
    let account = hex::decode(HYPEREVM_SELF).unwrap();
    let call = hyperevm_self_call(&mut b);
    assert_eq!((call.caller == account, call.address == account, call.balance_changes.len()), (true, true, 4));
    // The pair ending at the balance (ordinals 454 and 455) is dropped; the
    // debit and credit after it (456, 457) chain.
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &account);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
        ("1250703678780216487", "1250763678780216487", 450, 468, 4)
    );
}

#[test]
fn the_hyperevm_self_transfer_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    // Both records end at the same balance.
    let mut b = captured_hyperevm_self_transfer();
    hyperevm_self_call(&mut b).balance_changes[1].new_value = wei(1_310_703_678_780_216_488).into();
    assert!(fails(&b));
    // From the balance plus and minus the call's value.
    let mut b = captured_hyperevm_self_transfer();
    hyperevm_self_call(&mut b).balance_changes[0].old_value = wei(1_310_763_678_780_216_488).into();
    assert!(fails(&b));
    let mut b = captured_hyperevm_self_transfer();
    hyperevm_self_call(&mut b).value = wei(60_000_000_000_001).into();
    assert!(fails(&b));
    // At consecutive ordinals.
    let mut b = captured_hyperevm_self_transfer();
    hyperevm_self_call(&mut b).balance_changes[0].ordinal = 453;
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
/// A captured block of a fixture: its number, `Block.ver`, coinbase, mix
/// hash (kept since v0.7.9; Arbitrum's carries the ArbOS version),
/// block-level records and transactions; a transaction without a `type` is
/// legacy, and one without a `from`, `to` or `value` has none.
fn captured_block(c: &Value) -> eth::Block {
    let account = |v: &Value| v.as_str().map(|a| hex::decode(a).unwrap()).unwrap_or_default();
    let number = c["number"].as_u64().unwrap();
    let mut b = block();
    b.ver = c["ver"].as_i64().unwrap() as i32;
    b.number = number;
    b.header = buffa::MessageField::some(eth::BlockHeader {
        number,
        coinbase: hex::decode(c["coinbase"].as_str().unwrap()).unwrap(),
        mix_hash: account(&c["mix_hash"]),
        base_fee_per_gas: c["base_fee_per_gas"].as_str().and_then(|_| hex_big(&c["base_fee_per_gas"])).into(),
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
            gas_limit: t["gas_limit"].as_u64().unwrap_or(0),
            gas_used: t["gas_used"].as_u64().unwrap_or(0),
            gas_price: t["gas_price"].as_str().and_then(|_| hex_big(&t["gas_price"])).into(),
            from: account(&t["from"]),
            to: account(&t["to"]),
            value: t["value"].as_str().and_then(|_| hex_big(&t["value"])).into(),
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

/// Ethereum block 19,481,732 as captured from Firehose: transaction 131
/// (`0x833a78e6…`) whole. It sends 1 wei to `0x6550f9a4…`, whose call 2, a
/// CALL to itself, self-destructs to itself after Cancun (EIP-6780).
fn captured_selfdestruct_to_self() -> eth::Block {
    captured_block(&serde_json::from_str(SELFDESTRUCT_TO_SELF).unwrap())
}
const TO_SELF: &str = "6550f9a4bd878a384625f62ad5aab1fe7c3412de";

#[test]
fn captured_eip6780_selfdestruct_to_self_keeps_the_balance() {
    let b = captured_selfdestruct_to_self();
    let account = hex::decode(TO_SELF).unwrap();
    // The producer records a refund from zero and a withdraw to zero.
    let reasons: Vec<_> = records(&b)
        .unwrap()
        .iter()
        .filter(|r| r.address == account)
        .map(|r| (r.ordinal, r.reason))
        .collect();
    use eth::balance_change::Reason;
    assert_eq!(
        reasons,
        [
            (5151, Reason::Transfer as i32),
            (5155, Reason::SuicideRefund as i32),
            (5156, Reason::SuicideWithdraw as i32)
        ]
    );
    // The account keeps the 1 wei it received (an explorer reads 1 wei after
    // this block).
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &account);
    assert_eq!((row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records), ("0", "1", 5151, 1));
}

#[test]
fn the_selfdestruct_to_self_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    fn suicide(b: &mut eth::Block) -> &mut eth::Call {
        b.transaction_traces[0].calls.iter_mut().find(|c| c.index == 2).unwrap()
    }
    // Only a self-destructing call.
    let mut b = captured_selfdestruct_to_self();
    suicide(&mut b).suicide = false;
    assert!(fails(&b));
    // Not for an account created in the transaction, which is destroyed.
    let mut b = captured_selfdestruct_to_self();
    b.transaction_traces[0].calls[0].call_type = eth::CallType::CREATE.into();
    assert!(fails(&b));
    // Only the refund and the withdraw, at consecutive ordinals.
    let mut b = captured_selfdestruct_to_self();
    suicide(&mut b).balance_changes[1].reason = eth::balance_change::Reason::Transfer.into();
    assert!(fails(&b));
    let mut b = captured_selfdestruct_to_self();
    suicide(&mut b).balance_changes[1].ordinal = 5157;
    assert!(fails(&b));
    // The destroyed account is the call's address, not a DELEGATECALL's caller
    // here: as a DELEGATECALL, the records are not the destroyed account's.
    let mut b = captured_selfdestruct_to_self();
    let call = suicide(&mut b);
    call.call_type = eth::CallType::DELEGATE.into();
    call.caller = hex::decode(BENEFICIARY).unwrap();
    assert!(fails(&b));
    // A refund from the prior balance is the legacy self-destruct, which burns:
    // its records chain and are kept.
    let mut b = captured_selfdestruct_to_self();
    let call = suicide(&mut b);
    call.balance_changes[0].old_value = big(1).into();
    call.balance_changes[0].new_value = big(2).into();
    call.balance_changes[1].old_value = big(2).into();
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &hex::decode(TO_SELF).unwrap());
    assert_eq!((row.amount.as_str(), row.records), ("0", 3));
}

/// Ethereum block 25,741,008 as captured from Firehose: transaction 465
/// (`0xd6299894…`) whole. Its call 2, a CREATE at `0x1af6cfe5…`, which already
/// holds 0.159 ETH, self-destructs in its constructor to the sender
/// `0xc6acbee4…` after Cancun (EIP-6780).
fn captured_selfdestruct_created() -> eth::Block {
    captured_block(&serde_json::from_str(SELFDESTRUCT_CREATED_CREDIT).unwrap())
}
const CREATED: &str = "1af6cfe55b7b83445077477fa3487709dbc1b0c8";
const SENDER: &str = "c6acbee42e9e323140c1ed060c2f6ea9cc3b4b75";
fn wei(v: u128) -> Option<eth::BigInt> {
    Some(eth::BigInt {
        bytes: v.to_be_bytes().to_vec(),
    })
}

#[test]
fn captured_eip6780_selfdestruct_credit_carries_into_the_beneficiary_s_later_records() {
    let b = captured_selfdestruct_created();
    let sender = hex::decode(SENDER).unwrap();
    // The gas refund starts from the balance before the self-destruct credit.
    let mut values: Vec<_> = records(&b)
        .unwrap()
        .iter()
        .filter(|r| r.address == sender)
        .map(|r| (r.ordinal, amount(&value(r.old_value.as_deref()).unwrap())))
        .collect();
    values.sort();
    assert_eq!(
        values,
        [
            (9565, "78870813424372515183".to_string()),
            (9576, "78870637669729415183".to_string()),
            (9580, "78870637669729415183".to_string())
        ]
    );
    // The sender keeps the credit (an explorer reads 79.030030017051665919 ETH
    // after this block), and the created account ends empty.
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &sender);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
        ("78870813424372515183", "79030030017051665919", 9580, 3)
    );
    let row = row_of(&rows, &hex::decode(CREATED).unwrap());
    assert_eq!((row.old_amount.as_str(), row.amount.as_str()), ("159344587755534742", "0"));
}

#[test]
fn the_selfdestruct_credit_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    fn suicide(b: &mut eth::Block) -> &mut eth::Call {
        b.transaction_traces[0].calls.iter_mut().find(|c| c.index == 2).unwrap()
    }
    fn gas_refund(b: &mut eth::Block) -> &mut eth::BalanceChange {
        b.transaction_traces[0].calls[0].balance_changes.iter_mut().find(|c| c.ordinal == 9580).unwrap()
    }
    let sender_amount = |b: &eth::Block| row_of(&changes(b).unwrap(), &hex::decode(SENDER).unwrap()).amount.clone();
    // Only a self-destructing call.
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).suicide = false;
    assert!(fails(&b));
    // Only an account created in the transaction.
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).call_type = eth::CallType::CALL.into();
    assert!(fails(&b));
    // A SUICIDE_WITHDRAW of the destroyed account, and the refund at the next
    // ordinal.
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).balance_changes[0].reason = eth::balance_change::Reason::Transfer.into();
    assert!(fails(&b));
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).balance_changes[1].ordinal = 9577;
    assert!(fails(&b));
    // Only a refund of the withdrawn balance, even when the gas refund is short
    // by exactly the withdrawn amount.
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).balance_changes[1].new_value = wei(79_029_982_257_484_949_924).into();
    gas_refund(&mut b).old_value = wei(78_870_637_669_729_415_182).into();
    gas_refund(&mut b).new_value = wei(78_870_685_429_296_131_176).into();
    assert!(fails(&b));
    // A reverted self-destruct moves nothing: the gas refund chains from the
    // gas purchase.
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).state_reverted = true;
    assert_eq!(sender_amount(&b), "78870685429296131177");
    // A later record that already includes the credit is not shifted again.
    let mut b = captured_selfdestruct_created();
    gas_refund(&mut b).old_value = wei(79_029_982_257_484_949_925).into();
    gas_refund(&mut b).new_value = wei(79_030_030_017_051_665_919).into();
    assert_eq!(sender_amount(&b), "79030030017051665919");
    // A record during the execution starts from the credited balance and is
    // not shifted; the gas refund after the execution is (block 25,920,498).
    let payment = |old: u128| eth::BalanceChange {
        address: hex::decode(SENDER).unwrap(),
        old_value: wei(old).into(),
        new_value: wei(old - 5).into(),
        ordinal: 9578,
        reason: eth::balance_change::Reason::Transfer.into(),
    };
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls[0].balance_changes.insert(1, payment(79_029_982_257_484_949_925));
    gas_refund(&mut b).old_value = wei(78_870_637_669_729_415_178).into();
    gas_refund(&mut b).new_value = wei(78_870_685_429_296_131_172).into();
    assert_eq!(sender_amount(&b), "79030030017051665914");
    // A record during the execution that leaves out the credit is not shifted.
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls[0].balance_changes.insert(1, payment(78_870_637_669_729_415_183));
    gas_refund(&mut b).old_value = wei(78_870_637_669_729_415_178).into();
    gas_refund(&mut b).new_value = wei(78_870_685_429_296_131_172).into();
    assert!(fails(&b));
    // Only a gas refund or a fee credit after the execution is shifted, and
    // each must move the amount the transaction implies: the refund
    // (100,000 - 72,826) * 1,757,546,431 wei, the fee 72,826 * (1,757,546,431
    // - 94,001,672) wei.
    let mut b = captured_selfdestruct_created();
    gas_refund(&mut b).reason = eth::balance_change::Reason::Transfer.into();
    assert!(fails(&b));
    let mut b = captured_selfdestruct_created();
    gas_refund(&mut b).reason = eth::balance_change::Reason::RewardTransactionFee.into();
    assert!(fails(&b));
    // A gas refund whose new value alone includes the credit is not shifted.
    let mut b = captured_selfdestruct_created();
    gas_refund(&mut b).new_value = wei(79_030_030_017_051_665_919).into();
    assert!(fails(&b));
    // Every record after the execution is shifted: here the sender is also paid
    // the fee.
    let after = |reason: eth::balance_change::Reason, moved: u128| eth::BalanceChange {
        address: hex::decode(SENDER).unwrap(),
        old_value: wei(78_870_685_429_296_131_177).into(),
        new_value: wei(78_870_685_429_296_131_177 + moved).into(),
        ordinal: 9582,
        reason: reason.into(),
    };
    let fee = 121_149_310_618_934;
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls[0]
        .balance_changes
        .push(after(eth::balance_change::Reason::RewardTransactionFee, fee));
    assert_eq!(sender_amount(&b), "79030151166362284853");
    for reason in [eth::balance_change::Reason::Transfer, eth::balance_change::Reason::SuicideRefund] {
        let mut b = captured_selfdestruct_created();
        b.transaction_traces[0].calls[0].balance_changes.push(after(reason, fee));
        assert!(fails(&b));
    }
    // The credit happens during the execution.
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls[0].end_ordinal = 9570;
    assert!(fails(&b));
    // A refund recorded after the transaction does not shift the gas refund.
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).balance_changes[0].ordinal = 9584;
    suicide(&mut b).balance_changes[1].ordinal = 9585;
    suicide(&mut b).balance_changes[1].old_value = wei(78_870_685_429_296_131_177).into();
    suicide(&mut b).balance_changes[1].new_value = wei(79_030_030_017_051_665_919).into();
    gas_refund(&mut b).old_value = wei(78_711_293_081_973_880_441).into();
    gas_refund(&mut b).new_value = wei(78_711_340_841_540_596_435).into();
    assert!(fails(&b));
    // The execution ends with the root call: a first call that is not at depth
    // 0 gives no end, and no shift.
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls[0].depth = 1;
    assert!(fails(&b));
    // Records after the transaction are not shifted: a later transaction's
    // record must start from the credited balance, even a gas refund of the
    // same amount.
    let next = |reason: eth::balance_change::Reason, old: u128, new: u128| eth::TransactionTrace {
        index: 466,
        status: eth::TransactionTraceStatus::Succeeded.into(),
        begin_ordinal: 9584,
        end_ordinal: 9590,
        calls: vec![eth::Call {
            balance_changes: vec![eth::BalanceChange {
                address: hex::decode(SENDER).unwrap(),
                old_value: wei(old).into(),
                new_value: wei(new).into(),
                ordinal: 9585,
                reason: reason.into(),
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut b = captured_selfdestruct_created();
    b.transaction_traces.push(next(
        eth::balance_change::Reason::GasRefund,
        78_870_685_429_296_131_177,
        78_870_733_188_862_847_171,
    ));
    assert!(fails(&b));
    let mut b = captured_selfdestruct_created();
    b.transaction_traces.push(next(
        eth::balance_change::Reason::GasBuy,
        79_030_030_017_051_665_919,
        79_030_030_017_051_665_918,
    ));
    assert_eq!(sender_amount(&b), "79030030017051665918");
    // Credits add up per transaction: a later transaction's own credit shifts
    // only its own gas refund.
    let sender = hex::decode(SENDER).unwrap();
    let record = |address: &[u8], old: u128, new: Option<u128>, ordinal: u64, reason: eth::balance_change::Reason| eth::BalanceChange {
        address: address.to_vec(),
        old_value: wei(old).into(),
        new_value: new.and_then(wei).into(),
        ordinal,
        reason: reason.into(),
    };
    let mut b = captured_selfdestruct_created();
    b.transaction_traces.push(eth::TransactionTrace {
        index: 466,
        status: eth::TransactionTraceStatus::Succeeded.into(),
        begin_ordinal: 9584,
        end_ordinal: 9600,
        gas_limit: 5,
        gas_used: 0,
        gas_price: wei(1).into(),
        calls: vec![
            eth::Call {
                index: 1,
                end_ordinal: 9595,
                balance_changes: vec![
                    record(
                        &sender,
                        79_030_030_017_051_665_919,
                        Some(79_030_030_017_051_665_900),
                        9585,
                        eth::balance_change::Reason::GasBuy,
                    ),
                    record(
                        &sender,
                        79_030_030_017_051_665_900,
                        Some(79_030_030_017_051_665_905),
                        9597,
                        eth::balance_change::Reason::GasRefund,
                    ),
                ],
                ..Default::default()
            },
            eth::Call {
                index: 2,
                parent_index: 1,
                depth: 1,
                call_type: eth::CallType::CREATE.into(),
                address: vec![0x33; 20],
                suicide: true,
                balance_changes: vec![
                    record(&[0x33; 20], 7, None, 9590, eth::balance_change::Reason::SuicideWithdraw),
                    record(
                        &sender,
                        79_030_030_017_051_665_900,
                        Some(79_030_030_017_051_665_907),
                        9591,
                        eth::balance_change::Reason::SuicideRefund,
                    ),
                ],
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    let rows = changes(&b).unwrap();
    assert_eq!(row_of(&rows, &sender).amount, "79030030017051665912");
    assert_eq!(row_of(&rows, &[0x33; 20]).amount, "0");
    let other = hex::decode("00000000000000000000000000000000000000aa").unwrap();
    // The withdraw must be the destroyed account's, and end at zero.
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).balance_changes[0].address = other.clone();
    assert!(fails(&b));
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).balance_changes[0].new_value = wei(1).into();
    assert!(fails(&b));
    // The credit must be a SUICIDE_REFUND.
    let mut b = captured_selfdestruct_created();
    suicide(&mut b).balance_changes[1].reason = eth::balance_change::Reason::Transfer.into();
    assert!(fails(&b));
    // A credit back to the destroyed account is not this shape.
    let mut b = captured_selfdestruct_created();
    let d = suicide(&mut b).balance_changes[0].old_value.clone();
    suicide(&mut b).balance_changes[1].address = hex::decode(CREATED).unwrap();
    suicide(&mut b).balance_changes[1].old_value = wei(0).into();
    suicide(&mut b).balance_changes[1].new_value = d;
    b.transaction_traces[0].calls[0].balance_changes.push(eth::BalanceChange {
        address: hex::decode(CREATED).unwrap(),
        old_value: wei(0).into(),
        new_value: wei(fee).into(),
        ordinal: 9582,
        reason: eth::balance_change::Reason::RewardTransactionFee.into(),
    });
    assert!(fails(&b));
    // A DELEGATECALL self-destruct destroys its caller, here the created
    // account.
    let mut b = captured_selfdestruct_created();
    let moved = std::mem::take(&mut suicide(&mut b).balance_changes);
    suicide(&mut b).suicide = false;
    b.transaction_traces[0].calls.push(eth::Call {
        index: 5,
        parent_index: 2,
        depth: 2,
        call_type: eth::CallType::DELEGATE.into(),
        caller: hex::decode(CREATED).unwrap(),
        address: other.clone(),
        suicide: true,
        balance_changes: moved,
        ..Default::default()
    });
    assert_eq!(sender_amount(&b), "79030030017051665919");
    // The beneficiary's own withdraw to zero during the execution is not
    // shifted, so a withdraw that leaves out the credit fails the block,
    // whether the producer recorded it...
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls[0].balance_changes.push(eth::BalanceChange {
        address: hex::decode(SENDER).unwrap(),
        old_value: wei(78_870_637_669_729_415_183).into(),
        new_value: None.into(),
        ordinal: 9578,
        reason: eth::balance_change::Reason::SuicideWithdraw.into(),
    });
    gas_refund(&mut b).old_value = None.into();
    gas_refund(&mut b).new_value = wei(47_759_566_715_994).into();
    assert!(fails(&b));
    // ...or `selfdestruct_debits` restores it from a credit to another account.
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls.push(eth::Call {
        index: 5,
        parent_index: 1,
        depth: 1,
        call_type: eth::CallType::CALL.into(),
        caller: hex::decode(SENDER).unwrap(),
        address: hex::decode(SENDER).unwrap(),
        suicide: true,
        balance_changes: vec![eth::BalanceChange {
            address: other.clone(),
            old_value: None.into(),
            new_value: wei(78_870_637_669_729_415_183).into(),
            ordinal: 9578,
            reason: eth::balance_change::Reason::SuicideRefund.into(),
        }],
        ..Default::default()
    });
    gas_refund(&mut b).old_value = None.into();
    gas_refund(&mut b).new_value = wei(47_759_566_715_994).into();
    assert!(fails(&b));
    // Two such credits to one beneficiary add up: the second refund chains from
    // the first, and the gas refund is shifted by both (blocks 25,750,184 and
    // 25,869,452).
    let second = |refund_old: u128| eth::Call {
        index: 5,
        parent_index: 1,
        depth: 1,
        call_type: eth::CallType::CREATE.into(),
        caller: hex::decode(SENDER).unwrap(),
        address: vec![0x22; 20],
        suicide: true,
        balance_changes: vec![
            eth::BalanceChange {
                address: vec![0x22; 20],
                old_value: wei(5).into(),
                new_value: None.into(),
                ordinal: 9577,
                reason: eth::balance_change::Reason::SuicideWithdraw.into(),
            },
            eth::BalanceChange {
                address: hex::decode(SENDER).unwrap(),
                old_value: wei(refund_old).into(),
                new_value: wei(refund_old + 5).into(),
                ordinal: 9578,
                reason: eth::balance_change::Reason::SuicideRefund.into(),
            },
        ],
        ..Default::default()
    };
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls.push(second(79_029_982_257_484_949_925));
    assert_eq!(sender_amount(&b), "79030030017051665924");
    // A second refund that leaves out the first credit is not this shape.
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls.push(second(78_870_637_669_729_415_183));
    assert!(fails(&b));
    // The shifts see the restored debits: a restored debit of the beneficiary
    // is its last balance during the execution, so the gas refund, which
    // starts from it, is not shifted.
    let d: u128 = 159_344_587_755_534_742;
    let mut b = captured_selfdestruct_created();
    b.transaction_traces[0].calls[0].balance_changes[0].new_value = None.into();
    suicide(&mut b).balance_changes[1].old_value = None.into();
    suicide(&mut b).balance_changes[1].new_value = wei(d).into();
    b.transaction_traces[0].calls.push(eth::Call {
        index: 5,
        parent_index: 1,
        depth: 1,
        call_type: eth::CallType::CALL.into(),
        caller: sender.clone(),
        address: sender.clone(),
        suicide: true,
        balance_changes: vec![record(&other, 0, Some(d), 9578, eth::balance_change::Reason::SuicideRefund)],
        ..Default::default()
    });
    gas_refund(&mut b).old_value = None.into();
    gas_refund(&mut b).new_value = wei(47_759_566_715_994).into();
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &sender);
    assert_eq!((row.amount.as_str(), row.records), ("47759566715994", 4));
}

/// Ethereum block 25,747,303 as captured from Firehose: transaction 298
/// (`0x97a06638…`) whole, a contract-creation transaction. Its root call, the
/// CREATE of `0x5c2ff263…`, receives 0.000433 ETH from WETH (call 129) and
/// self-destructs to the sender `0x3128a77f…`; the root call also holds the
/// transaction's gas and fee records.
fn captured_selfdestruct_creation_tx() -> eth::Block {
    captured_block(&serde_json::from_str(SELFDESTRUCT_CREATION_TX).unwrap())
}
const CREATION_SENDER: &str = "3128a77f4a845b7db9f70c579a070175485b0baa";
const CREATION_CREATED: &str = "5c2ff26379ea2e53d958c1279e78e920e12b541d";

#[test]
fn captured_selfdestruct_credit_in_a_creation_transaction_s_root_call_carries_over() {
    let b = captured_selfdestruct_creation_tx();
    let root = &b.transaction_traces[0].calls[0];
    assert!(root.suicide && root.call_type.as_known() == Some(eth::CallType::CREATE));
    assert_eq!(root.balance_changes.len(), 5);
    // The sender keeps the credit (an explorer reads 0.018709964767809995 ETH
    // after this block), and the created account ends empty.
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &hex::decode(CREATION_SENDER).unwrap());
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
        ("18390237746296718", "18709964767809995", 11342, 3)
    );
    let row = row_of(&rows, &hex::decode(CREATION_CREATED).unwrap());
    assert_eq!((row.old_amount.as_str(), row.amount.as_str(), row.records), ("0", "0", 2));
}

#[test]
fn the_selfdestruct_credit_in_a_root_call_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[0].calls[0]
    }
    // Only an account created in the transaction.
    let mut b = captured_selfdestruct_creation_tx();
    root(&mut b).call_type = eth::CallType::CALL.into();
    assert!(fails(&b));
    // Exactly one withdraw and one refund in the call, whether the other comes
    // before or after the pair.
    for reason in [eth::balance_change::Reason::SuicideWithdraw, eth::balance_change::Reason::SuicideRefund] {
        for at_start in [true, false] {
            let mut b = captured_selfdestruct_creation_tx();
            let changes = &mut root(&mut b).balance_changes;
            let at = if at_start { 0 } else { changes.len() };
            changes.insert(
                at,
                eth::BalanceChange {
                    address: vec![0x22; 20],
                    old_value: wei(5).into(),
                    new_value: wei(6).into(),
                    ordinal: 11337,
                    reason: reason.into(),
                },
            );
            assert!(fails(&b), "extra {reason:?}, at start {at_start}");
        }
    }
    // The withdraw is the destroyed account's.
    let mut b = captured_selfdestruct_creation_tx();
    root(&mut b).balance_changes[1].address = vec![0x22; 20];
    assert!(fails(&b));
    // The refund follows the withdraw at the next ordinal.
    let mut b = captured_selfdestruct_creation_tx();
    root(&mut b).balance_changes[2].ordinal = 11341;
    assert!(fails(&b));
}

/// Ethereum block 25,869,452 as captured from Firehose: transaction 583
/// (`0x90f5b5c9…`) whole. Four contracts created by its calls 2, 10, 13 and
/// 16 self-destruct in their constructors to the sender `0x9263e7ba…`; each
/// refund chains from the previous one, and the gas refund leaves out all four.
fn captured_selfdestruct_four_credits() -> eth::Block {
    captured_block(&serde_json::from_str(SELFDESTRUCT_FOUR_CREDITS).unwrap())
}

#[test]
fn captured_selfdestruct_credits_add_up_in_the_gas_refund() {
    let b = captured_selfdestruct_four_credits();
    let sender = hex::decode("9263e7bac77c875f8c37973f0b39828b06fb1ceb").unwrap();
    // An explorer reads 5.994904450340433164 ETH after this block: the gas
    // refund's value plus the four credits (5.860378012225575 ETH).
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &sender);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
        ("134561298875857088", "5994904450340433164", 18104, 6)
    );
}

/// Ethereum block 25,920,498 as captured from Firehose: transaction 260
/// (`0x06fc62ea…`) whole. Call 15 creates `0xf5371697…` with value and
/// self-destructs to the sender `0x213f6e00…`; call 16 then moves value from
/// the sender, starting from the credited balance, and the gas refund leaves
/// out the credit.
fn captured_selfdestruct_transfer_during_execution() -> eth::Block {
    captured_block(&serde_json::from_str(SELFDESTRUCT_TRANSFER_DURING_EXECUTION).unwrap())
}

#[test]
fn captured_records_during_the_execution_keep_the_credit() {
    let b = captured_selfdestruct_transfer_during_execution();
    let sender = hex::decode("213f6e00a32593c0af669fe2e902f7d369422c73").unwrap();
    // An explorer reads 0.000047111859977024 ETH after this block. The
    // reverted transfer of call 14 does not persist.
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &sender);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
        ("43714971853610", "47111859977024", 11845, 4)
    );
}

/// Polygon block 95,537,024 as captured from Firehose: transactions 3, 4 and 5
/// whole. Each credits its base fee (`gas_used * base_fee`) to `0x3ef57def…`
/// with a `BURN` record in its root call; transaction 4 (`0xad18a4f2…`)
/// reverted.
fn captured_polygon_failed_base_fee() -> eth::Block {
    captured_block(&serde_json::from_str(POLYGON_FAILED_BASE_FEE).unwrap())
}
const POLYGON_BURN: &str = "3ef57def668054dd750bd260526105c4eeef104f";

#[test]
fn captured_polygon_failed_transaction_pays_its_base_fee() {
    let b = captured_polygon_failed_base_fee();
    let burn = hex::decode(POLYGON_BURN).unwrap();
    assert_eq!(b.transaction_traces[1].status.as_known(), Some(eth::TransactionTraceStatus::Reverted));
    let failed: Vec<_> = records(&b)
        .unwrap()
        .into_iter()
        .filter(|r| r.scope == persist::Scope::TxFailedPersistent && r.address == burn)
        .map(|r| r.ordinal)
        .collect();
    assert_eq!(failed, [111]);
    // The account chains through the reverted transaction's credit.
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &burn);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
        ("62430004335053818128433431", "62430004476293688362566195", 124, 3)
    );
}

#[test]
fn the_failed_base_fee_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    fn burn(b: &mut eth::Block) -> &mut eth::BalanceChange {
        b.transaction_traces[1].calls[0].balance_changes.iter_mut().find(|c| c.ordinal == 111).unwrap()
    }
    // Only a credit of exactly gas_used * base_fee.
    let mut b = captured_polygon_failed_base_fee();
    let new = BigInt::from_unsigned_bytes_be(&burn(&mut b).new_value.as_option().unwrap().bytes) + BigInt::from(1);
    burn(&mut b).new_value = Some(eth::BigInt { bytes: new.to_bytes_be().1 }).into();
    assert!(fails(&b));
    // Only with a base fee.
    let mut b = captured_polygon_failed_base_fee();
    if let Some(header) = b.header.as_option_mut() {
        header.base_fee_per_gas = None.into();
    }
    assert!(fails(&b));
    // Only a BURN record: another reason of the same amount reverts. (A
    // TRANSFER of it is a fee transfer, see `failed_fee_transfers`.)
    let mut b = captured_polygon_failed_base_fee();
    burn(&mut b).reason = eth::balance_change::Reason::CallBalanceOverride.into();
    assert!(fails(&b));
    // Only in the root call.
    let mut b = captured_polygon_failed_base_fee();
    let record = b.transaction_traces[1].calls[0].balance_changes.remove(2);
    b.transaction_traces[1].calls[1].balance_changes.push(record);
    assert!(fails(&b));
}

/// Arbitrum One block 150,000,072 as captured from Firehose: transactions 3
/// to 8 whole. Each pays its fee after the execution with `TRANSFER` credits
/// to `0xbf5041fc…` and `0xa4b0…00f6`; transactions 4 to 7 reverted.
fn captured_arbitrum_failed_fee() -> eth::Block {
    captured_block(&serde_json::from_str(ARBITRUM_FAILED_FEE).unwrap())
}
/// Arbitrum One block 514,857,274 as captured from Firehose: transactions 8,
/// 9 and 10 whole. Transaction 9 (`0xb972284a…`) reverted, and its tip goes
/// to `0x32e7af5a…` as a `REWARD_TRANSACTION_FEE`.
fn captured_arbitrum_failed_fee_tip() -> eth::Block {
    captured_block(&serde_json::from_str(ARBITRUM_FAILED_FEE_TIP).unwrap())
}
/// Arbitrum One's fee accounts, as `ArbOwnerPublic` reads them from block
/// 97,021,351 on: the infrastructure and the network fee account.
const INFRA_FEE: &str = "bf5041fc07e1c866d15c749156657b8eed0fb649";
const L1_PRICER: &str = "a4b00000000000000000000000000000000000f6";
const NETWORK_FEE: &str = "32e7af5a8151934f3787d0cd59eb6edd0a736b1d";

#[test]
fn captured_arbitrum_failed_transactions_pay_their_fee_transfers() {
    let b = captured_arbitrum_failed_fee();
    let (infra, pricer) = (hex::decode(INFRA_FEE).unwrap(), hex::decode(L1_PRICER).unwrap());
    assert!((1..=4).all(|i| b.transaction_traces[i].status.as_known() == Some(eth::TransactionTraceStatus::Reverted)));
    let failed: Vec<_> = records(&b)
        .unwrap()
        .into_iter()
        .filter(|r| r.scope == persist::Scope::TxFailedPersistent && r.reason == eth::balance_change::Reason::Transfer as i32)
        .map(|r| (r.address == infra, r.ordinal))
        .collect();
    let expected = [
        (true, 299),
        (false, 300),
        (true, 335),
        (false, 336),
        (true, 362),
        (false, 363),
        (true, 454),
        (false, 455),
    ];
    assert_eq!(failed, expected);
    // Both accounts chain through the reverted transactions' credits.
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &infra);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
        ("2866154671453100000000", "2866154732694200000000", 469, 6)
    );
    let row = row_of(&rows, &pricer);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
        ("3761012920399978752", "3761524477299978752", 470, 6)
    );
}

#[test]
fn captured_arbitrum_failed_transaction_pays_its_tip_and_fee_transfers() {
    let b = captured_arbitrum_failed_fee_tip();
    assert_eq!(b.transaction_traces[1].status.as_known(), Some(eth::TransactionTraceStatus::Reverted));
    let rows = changes(&b).unwrap();
    let expected = [
        (NETWORK_FEE, "2717795606989368354", "2717795707110810994", 2),
        (INFRA_FEE, "4338170651860007000", "4338174776900007000", 3),
        (L1_PRICER, "3514566862128762270", "3514566930530355470", 3),
    ];
    for (account, old, new, records) in expected {
        let row = row_of(&rows, &hex::decode(account).unwrap());
        assert_eq!((row.old_amount.as_str(), row.amount.as_str(), row.records), (old, new, records));
    }
    // The tip counts towards the fee: without it the transfers revert.
    let mut b = captured_arbitrum_failed_fee_tip();
    let tip = &mut b.transaction_traces[1].calls[0].balance_changes[2];
    assert_eq!(tip.reason.as_known(), Some(eth::balance_change::Reason::RewardTransactionFee));
    tip.new_value = wei(2_717_795_707_110_810_995).into();
    assert!(changes(&b).unwrap_err().to_string().contains("discontinuous"));
}

#[test]
fn the_failed_fee_transfer_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    // Transaction 4: GAS_BUY, GAS_REFUND, then the two fee transfers.
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[1].calls[0]
    }
    let payer = hex::decode("9e7a59b3124a482e9f046c00910d22d62c21b272").unwrap();
    assert_eq!(captured_arbitrum_failed_fee().transaction_traces[1].calls[0].balance_changes[0].address, payer);
    // Only credits adding up to the gas paid.
    let mut b = captured_arbitrum_failed_fee();
    root(&mut b).balance_changes[2].new_value = wei(2_866_154_704_846_300_000_001).into();
    assert!(fails(&b));
    // Only after the execution.
    let mut b = captured_arbitrum_failed_fee();
    root(&mut b).end_ordinal = 300;
    assert!(fails(&b));
    // Only credits of other accounts than the payer.
    let mut b = captured_arbitrum_failed_fee();
    root(&mut b).balance_changes[3].address = payer.clone();
    assert!(fails(&b));
    // Only TRANSFER and REWARD_TRANSACTION_FEE credits after the execution.
    let mut b = captured_arbitrum_failed_fee();
    root(&mut b).balance_changes[3].reason = eth::balance_change::Reason::Burn.into();
    assert!(fails(&b));
    // Only one GAS_BUY.
    let mut b = captured_arbitrum_failed_fee();
    let buy = eth::BalanceChange {
        address: payer.clone(),
        reason: eth::balance_change::Reason::GasBuy.into(),
        ..change(0, 282, Some(128_197_911_827_510_000), Some(128_197_911_827_510_000))
    };
    root(&mut b).balance_changes.insert(1, buy);
    assert!(fails(&b));
    // Only the payer's refund.
    let mut b = captured_arbitrum_failed_fee();
    root(&mut b).balance_changes[1].address = hex::decode(SENDER).unwrap();
    assert!(fails(&b));
}

/// Arbitrum One blocks 514,523,401, 514,497,707 and 514,709,255 (`Block.ver`
/// 3) as captured from Firehose, each with its transaction 1 whole, which
/// succeeded: the deposit `0x4c841c87…` (type 100) of 0.0001 ETH from
/// `0x3607a3f5…` to `0x24f6a3f5…`, the retryable submission `0x9c77cfe9…`
/// (type 105) of `0xb4b8b6f8…`, and the batch posting report `0x414a3691…`
/// (type 106), which pays `0xe6ec2174…` from the L1 pricer pool.
fn captured_arbitrum_arbos_transactions() -> [eth::Block; 3] {
    [
        include_str!("../tests/fixtures/arbitrum-514523401-deposit.json"),
        include_str!("../tests/fixtures/arbitrum-514497707-submit-retryable.json"),
        include_str!("../tests/fixtures/arbitrum-514709255-batch-posting-report.json"),
    ]
    .map(|f| captured_block(&serde_json::from_str(f).unwrap()))
}
/// Arbitrum One block 22,207,957 as captured from Firehose: transaction 1
/// (`0xff56fb78…`) whole, the retryable submission of L1 message 1 from
/// `0x84aac5e6…` with no deposit, which failed without a balance record.
fn captured_arbitrum_failed_submission_without_records() -> eth::Block {
    captured_block(&serde_json::from_str(include_str!("../tests/fixtures/arbitrum-22207957-failed-submit-retryable.json")).unwrap())
}
/// Arbitrum One block 71,044,381 as captured from Firehose: transaction 1
/// (`0x6afc76c4…`) whole, the retryable submission of L1 message 632,385 from
/// `0x26196e02…`, which failed below its maximum submission fee and keeps its
/// deposit's mint of 17,400,000,000,000 wei.
fn captured_arbitrum_failed_submission_mint() -> eth::Block {
    captured_block(&serde_json::from_str(include_str!("../tests/fixtures/arbitrum-71044381-failed-submit-retryable-mint.json")).unwrap())
}
const SUBMISSION_SENDER: &str = "26196e0245446ca06dbe2b4de9fc3ee160d51523";
/// Arbitrum One block 333,543,134 as captured from Firehose: transaction 1
/// (`0x8113baed…`) whole, the retryable submission of L1 message 1,962,555
/// from `0x06fe418a…` with a deposit of 60,000,000,000,000 wei, which failed
/// on its call value of 0.05 ETH and keeps its deposit's mint, its submission
/// fee to the network fee account and the fee's refund.
fn captured_arbitrum_failed_submission_call_value() -> eth::Block {
    captured_block(&serde_json::from_str(include_str!("../tests/fixtures/arbitrum-333543134-failed-submit-retryable-call-value.json")).unwrap())
}
const CALL_VALUE_SENDER: &str = "06fe418a9bde897f9720c1bd62a1115e7914c0fe";
fn refused_as_kept_by_arbos(b: &eth::Block) -> bool {
    changes(b).unwrap_err().to_string().contains("balance records that ArbOS keeps")
}
fn failed_first_transaction(mut b: eth::Block) -> eth::Block {
    b.transaction_traces[0].status = eth::TransactionTraceStatus::Failed.into();
    b
}

#[test]
fn captured_failed_arbitrum_deposit_submission_and_internal_transaction_fail_the_block() {
    use eth::transaction_trace::Type;
    let [deposit, submission, report] = captured_arbitrum_arbos_transactions();
    // Succeeded, every record persists, at depth 0 and 1: the deposit's mint
    // to its sender and its transfer to the recipient...
    assert_rows(
        &changes(&deposit).unwrap(),
        &[
            ("3607a3f57549520cb3b2d1c691d2933aecfa73fc", "0", "0", 21, 2),
            ("24f6a3f57549520cb3b2d1c691d2933aecfa62eb", "0", "100000000000000", 23, 1),
        ],
    );
    // ...the submission's deposit mint, its fees and its refunds...
    assert_rows(
        &changes(&submission).unwrap(),
        &[
            (RETRY_SENDER, "1552251920881315441", "1552251920881315441", 74, 6),
            (NETWORK_FEE, "24049424609291339035", "24049425312023696635", 71, 2),
            (INFRA_FEE, "32657830598880007000", "32657832532860007000", 67, 1),
            ("7cc207f46b30d03aad67871ade8f6333cf7a3731", "0", "11846753849120", 75, 2),
        ],
    );
    // ...and the batch poster's payment from the L1 pricer pool.
    assert_rows(
        &changes(&report).unwrap(),
        &[
            (L1_PRICER, "3514546805402598569", "3514527727413777969", 26, 1),
            ("e6ec2174539a849f9f3ec973c66b333ed08c0c18", "4323072961863150", "4342150950683750", 27, 1),
        ],
    );
    // Failed, ArbOS would keep every move, which the shared rules drop.
    let kinds = [
        Type::TrxTypeArbitrumDeposit,
        Type::TrxTypeArbitrumSubmitRetryable,
        Type::TrxTypeArbitrumInternal,
    ];
    for (b, kind) in [deposit, submission, report].into_iter().zip(kinds) {
        assert_eq!(b.transaction_traces[0].r#type.as_known(), Some(kind));
        for status in [eth::TransactionTraceStatus::Failed, eth::TransactionTraceStatus::Reverted] {
            let mut b = b.clone();
            b.transaction_traces[0].status = status.into();
            assert!(refused_as_kept_by_arbos(&b), "{kind:?} {status:?}");
        }
    }
    // The submission `0x38b8e37a…` of block 52,067,707 records its moves in
    // the root call.
    let b = failed_first_transaction(captured_arbitrum_failed_retry());
    assert_eq!(b.transaction_traces[0].r#type.as_known(), Some(Type::TrxTypeArbitrumSubmitRetryable));
    assert!(refused_as_kept_by_arbos(&b));
    // A failed submission without a balance record keeps nothing.
    let b = captured_arbitrum_failed_submission_without_records();
    assert_eq!(
        (b.transaction_traces[0].r#type.as_known(), b.transaction_traces[0].status.as_known()),
        (Some(Type::TrxTypeArbitrumSubmitRetryable), Some(eth::TransactionTraceStatus::Failed))
    );
    assert!(changes(&b).unwrap().is_empty());
}

#[test]
fn the_failed_arbos_transaction_guard_is_exact() {
    use eth::transaction_trace::Type;
    let [deposit, submission, report] = captured_arbitrum_arbos_transactions().map(failed_first_transaction);
    fn calls(b: &mut eth::Block) -> &mut Vec<eth::Call> {
        &mut b.transaction_traces[0].calls
    }
    // A move in any call fails the block: the deposit's mint alone in the
    // root call...
    let mut b = deposit.clone();
    calls(&mut b)[1].balance_changes.clear();
    assert!(refused_as_kept_by_arbos(&b));
    // ...its transfer alone in the depth-1 call...
    let mut b = deposit.clone();
    calls(&mut b)[0].balance_changes.clear();
    assert!(refused_as_kept_by_arbos(&b));
    // ...or one record in a depth-2 call.
    let mut b = submission.clone();
    let record = calls(&mut b)[1].balance_changes.remove(0);
    calls(&mut b).iter_mut().for_each(|c| c.balance_changes.clear());
    assert_eq!(calls(&mut b)[2].depth, 2);
    calls(&mut b)[2].balance_changes.push(record);
    assert!(refused_as_kept_by_arbos(&b));
    // A gas record outside the root call fails too...
    let mut b = report.clone();
    calls(&mut b)[1].balance_changes.truncate(1);
    calls(&mut b)[1].balance_changes[0].reason = eth::balance_change::Reason::GasBuy.into();
    assert!(refused_as_kept_by_arbos(&b));
    // ...as does a value past `uint256`, new or old.
    let mut b = report.clone();
    calls(&mut b)[1].balance_changes.truncate(1);
    calls(&mut b)[1].balance_changes[0].new_value = Some(eth::BigInt { bytes: vec![1; 33] }).into();
    assert!(refused_as_kept_by_arbos(&b));
    let mut b = report.clone();
    calls(&mut b)[1].balance_changes.truncate(1);
    let c = &mut calls(&mut b)[1].balance_changes[0];
    (c.old_value, c.new_value) = (Some(eth::BigInt { bytes: vec![1; 33] }).into(), None.into());
    assert!(refused_as_kept_by_arbos(&b));
    // The shared rules persist a gas record of the root call.
    let mut b = report.clone();
    let mut gas = calls(&mut b)[1].balance_changes.remove(0);
    gas.reason = eth::balance_change::Reason::GasBuy.into();
    calls(&mut b).iter_mut().for_each(|c| c.balance_changes.clear());
    calls(&mut b)[0].balance_changes.push(gas);
    assert_rows(&changes(&b).unwrap(), &[(L1_PRICER, "3514546805402598569", "3514527727413777969", 26, 1)]);
    // Records that move nothing keep nothing.
    let mut b = deposit.clone();
    for c in calls(&mut b).iter_mut().flat_map(|c| c.balance_changes.iter_mut()) {
        c.new_value = c.old_value.clone();
    }
    assert!(changes(&b).unwrap().is_empty());
    // A succeeded transaction of these types is not affected.
    let mut b = submission.clone();
    b.transaction_traces[0].status = eth::TransactionTraceStatus::Succeeded.into();
    assert_eq!(changes(&b).unwrap().len(), 4);
    // Failed transactions that run in the EVM are not affected either: the
    // reverted type-2 transactions of block 150,000,072 pay the same fee
    // transfers as `ArbitrumUnsignedTx`, `ArbitrumContractTx` or
    // `ArbitrumLegacyTx`, but as a deposit, submission or internal
    // transaction they fail the block.
    let expected = changes(&captured_arbitrum_failed_fee()).unwrap();
    for kind in [Type::TrxTypeArbitrumUnsigned, Type::TrxTypeArbitrumContract, Type::TrxTypeArbitrumLegacy] {
        let mut b = captured_arbitrum_failed_fee();
        b.transaction_traces.iter_mut().for_each(|t| t.r#type = kind.into());
        assert_eq!(changes(&b).unwrap(), expected, "{kind:?}");
    }
    for kind in [
        Type::TrxTypeArbitrumDeposit,
        Type::TrxTypeArbitrumSubmitRetryable,
        Type::TrxTypeArbitrumInternal,
    ] {
        let mut b = captured_arbitrum_failed_fee();
        b.transaction_traces.iter_mut().for_each(|t| t.r#type = kind.into());
        assert!(refused_as_kept_by_arbos(&b), "{kind:?}");
    }
}

#[test]
fn captured_arbitrum_failed_submission_keeps_its_deposit_mint() {
    let b = captured_arbitrum_failed_submission_mint();
    let tx = &b.transaction_traces[0];
    assert_eq!(
        (tx.r#type.as_known(), tx.status.as_known(), tx.gas_used),
        (
            Some(eth::transaction_trace::Type::TrxTypeArbitrumSubmitRetryable),
            Some(eth::TransactionTraceStatus::Failed),
            0
        )
    );
    let kept: Vec<_> = records(&b).unwrap().into_iter().map(|r| (r.scope, r.ordinal)).collect();
    assert_eq!(kept, [(persist::Scope::TxFailedPersistent, 7)]);
    // eth_getBalance reads 0 before the block and the deposit after it.
    assert_rows(&changes(&b).unwrap(), &[(SUBMISSION_SENDER, "0", "17400000000000", 7, 1)]);
}

#[test]
fn the_failed_submission_settlement_shape_is_exact() {
    fn mint(b: &mut eth::Block) -> &mut eth::BalanceChange {
        &mut b.transaction_traces[0].calls[0].balance_changes[0]
    }
    let refused = |edit: &dyn Fn(&mut eth::Block)| {
        let mut b = captured_arbitrum_failed_submission_mint();
        edit(&mut b);
        refused_as_kept_by_arbos(&b)
    };
    // A FAILED submission without gas used...
    assert!(refused(&|b| b.transaction_traces[0].status = eth::TransactionTraceStatus::Reverted.into()));
    assert!(refused(&|b| b.transaction_traces[0].gas_used = 1));
    assert!(refused(
        &|b| b.transaction_traces[0].r#type = eth::transaction_trace::Type::TrxTypeArbitrumDeposit.into()
    ));
    assert!(refused(
        &|b| b.transaction_traces[0].r#type = eth::transaction_trace::Type::TrxTypeArbitrumInternal.into()
    ));
    // ...whose root call is its only call...
    assert!(refused(&|b| b.transaction_traces[0].calls[0].depth = 1));
    assert!(refused(&|b| {
        let call = eth::Call {
            depth: 1,
            ..b.transaction_traces[0].calls[0].clone()
        };
        b.transaction_traces[0].calls[0].balance_changes.clear();
        b.transaction_traces[0].calls.push(call);
    }));
    assert!(refused(&|b| {
        let call = eth::Call {
            depth: 1,
            balance_changes: Vec::new(),
            ..b.transaction_traces[0].calls[0].clone()
        };
        b.transaction_traces[0].calls.push(call);
    }));
    // ...holds a TRANSFER credit of the sender...
    assert!(refused(&|b| mint(b).reason = eth::balance_change::Reason::Burn.into()));
    assert!(refused(&|b| mint(b).address = vec![9; 20]));
    assert!(refused(&|b| {
        let m = mint(b);
        (m.old_value, m.new_value) = (m.new_value.clone(), m.old_value.clone());
    }));
    // ...within the transaction...
    assert!(refused(&|b| mint(b).ordinal = 5));
    assert!(refused(&|b| mint(b).ordinal = 8));
    // ...and no other record that moves a balance.
    let other = change(9, 6, Some(1), Some(0));
    assert!(refused(&|b| b.transaction_traces[0].calls[0].balance_changes.push(other.clone())));
    // Not even a gas record of the root call, which a submission never buys.
    assert!(refused(&|b| {
        let gas = eth::BalanceChange {
            address: hex::decode(SUBMISSION_SENDER).unwrap(),
            reason: eth::balance_change::Reason::GasBuy.into(),
            ..change(0, 8, Some(17_400_000_000_000), Some(0))
        };
        b.transaction_traces[0].calls[0].end_ordinal = 9;
        b.transaction_traces[0].calls[0].balance_changes.push(gas);
    }));
    // A record that moves nothing does not count.
    let mut b = captured_arbitrum_failed_submission_mint();
    b.transaction_traces[0].calls[0].balance_changes.push(change(9, 6, Some(1), Some(1)));
    assert_rows(&changes(&b).unwrap(), &[(SUBMISSION_SENDER, "0", "17400000000000", 7, 1)]);
    // The captured shape itself, and nothing else, persists.
    let mut b = captured_arbitrum_failed_submission_mint();
    mint(&mut b).ordinal = 6;
    assert_rows(&changes(&b).unwrap(), &[(SUBMISSION_SENDER, "0", "17400000000000", 6, 1)]);
}

#[test]
fn captured_arbitrum_failed_submission_keeps_its_fee_refund() {
    let b = captured_arbitrum_failed_submission_call_value();
    let tx = &b.transaction_traces[0];
    assert_eq!(
        (tx.r#type.as_known(), tx.status.as_known(), tx.gas_used),
        (
            Some(eth::transaction_trace::Type::TrxTypeArbitrumSubmitRetryable),
            Some(eth::TransactionTraceStatus::Failed),
            0
        )
    );
    // The mint, the submission fee and its refund.
    let kept: Vec<_> = records(&b).unwrap().into_iter().map(|r| (r.scope, r.ordinal)).collect();
    assert_eq!(kept, (8..=12).map(|o| (persist::Scope::TxFailedPersistent, o)).collect::<Vec<_>>());
    // eth_getBalance reads 0 before the block and the deposit after it; the
    // network fee account gives the fee back.
    assert_rows(
        &changes(&b).unwrap(),
        &[
            (CALL_VALUE_SENDER, "0", "60000000000000", 12, 3),
            (NETWORK_FEE, "58032418371264867579", "58032418371264867579", 11, 2),
        ],
    );
}

#[test]
fn the_failed_submission_fee_refund_shape_is_exact() {
    fn record(b: &mut eth::Block, ordinal: u64) -> &mut eth::BalanceChange {
        b.transaction_traces[0].calls[0]
            .balance_changes
            .iter_mut()
            .find(|c| c.ordinal == ordinal)
            .unwrap()
    }
    let refused = |edit: &dyn Fn(&mut eth::Block)| {
        let mut b = captured_arbitrum_failed_submission_call_value();
        edit(&mut b);
        refused_as_kept_by_arbos(&b)
    };
    // Records 8 to 12: the mint, the fee from the sender to `0x32e7af5a…`,
    // and back.
    let network = hex::decode(NETWORK_FEE).unwrap();
    assert_eq!(record(&mut captured_arbitrum_failed_submission_call_value(), 10).address, network);
    // The fee goes to another account than the sender...
    assert!(refused(&|b| {
        let sender = hex::decode(CALL_VALUE_SENDER).unwrap();
        let c = record(b, 10);
        (c.address, c.old_value, c.new_value) = (sender.clone(), wei(57_995_630_662_928).into(), wei(60_000_000_000_000).into());
        let c = record(b, 11);
        (c.address, c.old_value, c.new_value) = (sender, wei(60_000_000_000_000).into(), wei(57_995_630_662_928).into());
    }));
    // ...which pays it back...
    assert!(refused(&|b| record(b, 11).address = vec![9; 20]));
    // ...to the sender...
    assert!(refused(&|b| record(b, 12).address = vec![9; 20]));
    // ...in the same amount...
    assert!(refused(&|b| record(b, 10).new_value = wei(58_032_420_375_634_204_652).into()));
    assert!(refused(&|b| record(b, 11).old_value = wei(58_032_420_375_634_204_652).into()));
    assert!(refused(&|b| record(b, 12).new_value = wei(60_000_000_000_001).into()));
    // ...the sender's debit too: not 1 wei more, although the sender then
    // chains, 1 wei short, nor of another account...
    assert!(refused(&|b| {
        record(b, 9).new_value = wei(57_995_630_662_927).into();
        let c = record(b, 12);
        (c.old_value, c.new_value) = (wei(57_995_630_662_927).into(), wei(59_999_999_999_999).into());
    }));
    assert!(refused(&|b| {
        let c = record(b, 9);
        (c.address, c.old_value, c.new_value) = (vec![9; 20], wei(2_004_369_337_072).into(), wei(0).into());
        let c = record(b, 12);
        (c.old_value, c.new_value) = (wei(60_000_000_000_000).into(), wei(62_004_369_337_072).into());
    }));
    // ...in this order...
    assert!(refused(&|b| {
        record(b, 10).ordinal = 0;
        record(b, 11).ordinal = 10;
        record(b, 0).ordinal = 11;
    }));
    // ...a positive fee...
    assert!(refused(&|b| {
        for ordinal in 9..=12 {
            let c = record(b, ordinal);
            (c.old_value, c.new_value) = (c.new_value.clone(), c.old_value.clone());
        }
    }));
    // ...as TRANSFER records...
    assert!(refused(&|b| record(b, 11).reason = eth::balance_change::Reason::Burn.into()));
    // ...after the mint, and nothing else: no excess refund.
    assert!(refused(&|b| b.transaction_traces[0].calls[0].balance_changes.retain(|c| c.ordinal != 8)));
    assert!(refused(&|b| b.transaction_traces[0].calls[0].balance_changes.retain(|c| c.ordinal != 12)));
    assert!(refused(&|b| {
        let root = &mut b.transaction_traces[0].calls[0];
        root.end_ordinal = 15;
        root.balance_changes.push(change(9, 13, Some(1), Some(0)));
        root.balance_changes.push(change(8, 14, Some(0), Some(1)));
    }));
}

/// Arbitrum One blocks 350,048,039 and 514,791,024 (`Block.ver` 3) with their
/// reverted SetCode transactions `0xfed9d2ec…` (tx 4) and `0xdb5f4993…`
/// (tx 2), unmodified TransactionTrace messages fetched from Firehose. Their
/// root calls record `begin_ordinal` 0; the shared rules recover the boundary
/// of the authorization records (`evm_persist`).
fn captured_arbitrum_failed_setcode() -> [eth::Block; 2] {
    let traces: [(u64, &[u8]); 2] = [
        (
            350_048_039,
            include_bytes!("../../../erc20/balances/tests/fixtures/arbitrum-350048039-failed-setcode.pb"),
        ),
        (
            514_791_024,
            include_bytes!("../../../erc20/balances/tests/fixtures/arbitrum-514791024-failed-setcode.pb"),
        ),
    ];
    traces.map(|(number, bytes)| {
        let mut b = block();
        b.ver = 3;
        b.number = number;
        if let Some(header) = b.header.as_option_mut() {
            header.number = number;
        }
        b.transaction_traces = vec![eth::TransactionTrace::decode_from_slice(bytes).unwrap()];
        b
    })
}

#[test]
fn captured_arbitrum_failed_setcode_transactions_pay_gas_tip_and_fee_transfers() {
    let blocks = captured_arbitrum_failed_setcode();
    // Per block: the payer's and the fee accounts' rows, as (account,
    // old_amount, amount, ordinal, records).
    type Row = (&'static str, &'static str, &'static str, u64, u32);
    let expected: [&[Row]; 2] = [
        &[
            ("cd21f0d3ad097308c39e2b7914779787d22e831c", "8855685946491925", "8772934602543925", 316, 2),
            (INFRA_FEE, "418556627570000000", "418558532470000000", 317, 1),
            (NETWORK_FEE, "18584201982743401932", "18584277949774421932", 318, 1),
            (L1_PRICER, "3521629320310242706", "3521634199723170706", 319, 1),
        ],
        &[
            ("1a222f9b072aed5e9e815849de5a11b0fe3958ab", "5462582349527090", "5423721603338874", 134, 2),
            (NETWORK_FEE, "2169864327467472140", "2169901324892989628", 137, 2),
            (INFRA_FEE, "3241769450560007000", "3241771202400007000", 136, 1),
            (L1_PRICER, "3515061104658630616", "3515061216139301344", 138, 1),
        ],
    ];
    for (b, expected) in blocks.iter().zip(expected) {
        let tx = &b.transaction_traces[0];
        assert_eq!(tx.status.as_known(), Some(eth::TransactionTraceStatus::Reverted));
        assert_eq!(
            (tx.r#type.as_known(), tx.calls[0].begin_ordinal),
            (Some(eth::transaction_trace::Type::TrxTypeSetCode), 0)
        );
        let rows = changes(b).unwrap();
        assert_eq!(rows.len(), expected.len(), "block {}", b.number);
        for (account, old, new, ordinal, records) in expected {
            let row = row_of(&rows, &hex::decode(account).unwrap());
            assert_eq!(
                (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
                (*old, *new, *ordinal, *records),
                "block {} account {account}",
                b.number
            );
        }
    }
}

#[test]
fn the_failed_setcode_authorization_boundary_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("no authorization/execution ordinal boundary")
    }
    // Block 350,048,039: the authorization's nonce at 192, its delegation at
    // 193; no record takes 194, the root call's unrecorded begin.
    let [b, _] = captured_arbitrum_failed_setcode();
    assert!(changes(&b).is_ok());
    // A record at the boundary.
    let mut t = b.clone();
    t.transaction_traces[0].calls[0].gas_changes.push(eth::GasChange {
        ordinal: 194,
        ..Default::default()
    });
    assert!(fails(&t));
    // Another delegation than the authorization's.
    let mut t = b.clone();
    t.transaction_traces[0].set_code_authorizations[0].address = vec![0x11; 20];
    assert!(fails(&t));
    // An authorization without its records.
    let mut t = b.clone();
    t.transaction_traces[0].calls[0].nonce_changes.truncate(1);
    assert!(fails(&t));
}

/// Arbitrum One block 52,067,707 as captured from Firehose: transactions 1
/// and 2 whole. Transaction 1 submits ticket `0x38b8e37a…` from
/// `0xb4b8b6f8…`, the aliased L1 token gateway; transaction 2
/// (`0x533a62f8…`), its auto-redeem, an `ArbitrumRetryTx`, reverted.
fn captured_arbitrum_failed_retry() -> eth::Block {
    captured_block(&serde_json::from_str(ARBITRUM_FAILED_RETRY).unwrap())
}
/// Arbitrum One block 314,259,052 as captured from Firehose: transactions 5
/// and 6 whole. In transaction 5 `0x2cd28cda…` redeems ticket `0xfd623450…`;
/// transaction 6 (`0xe0359a51…`), the `ArbitrumRetryTx` it schedules with
/// 0.006025454 ETH of call value for `0x4996f81f…`, reverted.
fn captured_arbitrum_failed_retry_value() -> eth::Block {
    captured_block(&serde_json::from_str(ARBITRUM_FAILED_RETRY_VALUE).unwrap())
}
/// Arbitrum One block 511,131,266 (ArbOS 61) as captured from Firehose:
/// transactions 1 and 2 whole. Transaction 2 (`0x52221587…`), the
/// auto-redeem of ticket `0x7317cd66…` with 0.001855930887358923 ETH of
/// call value, ran out of gas.
fn captured_arbitrum_failed_retry_out_of_gas() -> eth::Block {
    captured_block(&serde_json::from_str(ARBITRUM_FAILED_RETRY_OUT_OF_GAS).unwrap())
}
const RETRY_SENDER: &str = "b4b8b6f88361f48403514059f1f16c8e78d61ffd";
const RETRY_VALUE_SENDER: &str = "4996f81fb2c72493b101ff3fd175861ccf036732";
const RETRY_ESCROW: &str = "f1642a4cf33849dd84841bdbc919dc8e953396ca";
fn assert_rows(rows: &[Change], expected: &[(&str, &str, &str, u64, u32)]) {
    for (account, old, new, ordinal, records) in expected {
        let row = row_of(rows, &hex::decode(account).unwrap());
        assert_eq!(
            (row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records),
            (*old, *new, *ordinal, *records),
            "{account}"
        );
    }
}

#[test]
fn captured_arbitrum_failed_retry_persists_its_settlement() {
    let b = captured_arbitrum_failed_retry();
    let retry = &b.transaction_traces[1];
    assert_eq!(
        (retry.r#type.as_known(), retry.status.as_known()),
        (
            Some(eth::transaction_trace::Type::TrxTypeArbitrumRetry),
            Some(eth::TransactionTraceStatus::Reverted)
        )
    );
    // The prepaid gas before the GAS_BUY, then the refund's burn and its
    // payment from the network fee account after the execution.
    let settled: Vec<_> = records(&b)
        .unwrap()
        .into_iter()
        .filter(|r| r.scope == persist::Scope::TxFailedPersistent && r.reason == eth::balance_change::Reason::Transfer as i32)
        .map(|r| r.ordinal)
        .collect();
    assert_eq!(settled, [51, 104, 105, 106]);
    // The sender, the network fee account `0xd345e41a…` and the refund address
    // chain through it; eth_getBalance after this block reads each amount.
    assert_rows(
        &changes(&b).unwrap(),
        &[
            (RETRY_SENDER, "1144742593324624", "1144742593324624", 104, 9),
            (
                "d345e41ae2cb00311956aa7109fc801ae8c81a52",
                "919240114772524704088",
                "919240220428078778328",
                105,
                3,
            ),
            ("65a8f07bd9a8598e1b5b6c0a88f4779dbc077675", "145742716882252577", "146044608097841377", 106, 3),
        ],
    );
}

#[test]
fn captured_arbitrum_failed_manual_redeem_returns_its_call_value_to_the_escrow() {
    let b = captured_arbitrum_failed_retry_value();
    let rows = changes(&b).unwrap();
    // eth_getBalance after this block reads each amount: the sender ends
    // empty, the escrow keeps the call value, and the infrastructure and
    // network fee accounts pay the refund address, the redeemer.
    assert_rows(
        &rows,
        &[
            (RETRY_VALUE_SENDER, "0", "0", 286, 6),
            (RETRY_ESCROW, "6025454000000000", "6025454000000000", 287, 2),
            ("2cd28cda6825c4967372478e87d004637b73f996", "14042830579263631", "14024644507731631", 285, 3),
            (INFRA_FEE, "19115353805570000000", "19115355012940000000", 282, 2),
            (NETWORK_FEE, "54091162600531997824", "54091174746432723824", 284, 2),
        ],
    );
    // The reverted call value transfer to `0x2eaf07a9…` (ordinals 270 and 271)
    // does not persist.
    assert!(!rows.iter().any(|r| hex::encode(&r.address) == "2eaf07a964c6601c4faefd6d8969df0b84f65e55"));
    // Without the settlement every account still chains, but the sender keeps
    // the call value and the refund, as native v0.7.8 reported. Its records
    // after the execution now fail the block instead
    // (`validate_failed_records_outside_execution`).
    let mut other = b.clone();
    other.transaction_traces[1].r#type = eth::transaction_trace::Type::TrxTypeDynamicFee.into();
    assert!(!records(&other).unwrap().iter().any(|r| hex::encode(&r.address) == RETRY_ESCROW));
    assert!(changes(&other)
        .unwrap_err()
        .to_string()
        .contains("after its execution without pinned persistence"));
}

#[test]
fn captured_arbitrum_retry_out_of_gas_returns_its_call_value_to_the_escrow() {
    let b = captured_arbitrum_failed_retry_out_of_gas();
    let retry = &b.transaction_traces[1];
    assert_eq!(retry.status.as_known(), Some(eth::TransactionTraceStatus::Failed));
    assert_eq!(retry.gas_used, retry.gas_limit);
    // No gas is left to refund. eth_getBalance after this block reads each
    // amount: the sender ends where it started, the escrow keeps the call
    // value, and the reverted transfer of it to `0xfec0017a…` does not
    // persist.
    assert_rows(
        &changes(&b).unwrap(),
        &[
            ("2342deb6f5749ef6ce6943a275a1d3e7486f5fbf", "6055862479220798", "6055862479220798", 74, 11),
            ("e7b383691b3490dd587632c66f49ace20ad54402", "0", "1855930887358923", 75, 3),
            ("fec0017a3a2362426316cffb3d8fae13c85996b0", "0", "2522032520000", 54, 2),
        ],
    );
}

#[test]
fn the_failed_retry_settlement_shape_is_exact() {
    fn refused(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("exact ArbOS settlement")
    }
    fn record(b: &mut eth::Block, ordinal: u64) -> &mut eth::BalanceChange {
        b.transaction_traces[1].calls[0]
            .balance_changes
            .iter_mut()
            .find(|c| c.ordinal == ordinal)
            .unwrap()
    }
    fn set(b: &mut eth::Block, ordinal: u64, old: u128, new: u128) {
        let c = record(b, ordinal);
        (c.old_value, c.new_value) = (wei(old).into(), wei(new).into());
    }
    let sender = hex::decode(RETRY_SENDER).unwrap();
    let other = vec![9; 20];
    // Block 52,067,707: the sender's balance before the retry, the prepaid gas
    // `g * b`, the refund `l * b`, and the network fee account's and the
    // refund address's balances around the payment at ordinals 105 and 106.
    // Where a mutation below moves an amount or an account, it keeps every
    // account chaining, so only the shape refuses it.
    let (start, prepaid, refund) = (1_144_742_593_324_624_u128, 7_892_600_000_000_u128, 115_400_000_000_u128);
    let (fee_before, fee_after) = (919_240_220_543_478_778_328_u128, 919_240_220_428_078_778_328_u128);
    let (paid_before, paid_after) = (146_044_492_697_841_377_u128, 146_044_608_097_841_377_u128);
    // The prepaid gas is the gas limit at the block's base fee.
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].gas_limit += 1;
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    b.header.as_option_mut().unwrap().base_fee_per_gas = wei(100_000_001).into();
    assert!(refused(&b));
    // It is minted to the sender before the GAS_BUY, as a TRANSFER.
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 51).ordinal = 53;
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 51).reason = eth::balance_change::Reason::Burn.into();
    assert!(refused(&b));
    // Not to another account, nor of another amount.
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 51).address = other.clone();
    set(&mut b, 51, 0, prepaid);
    set(&mut b, 52, start, start - prepaid);
    set(&mut b, 103, start - prepaid, start - prepaid + refund);
    set(&mut b, 104, start - prepaid + refund, start - prepaid);
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    set(&mut b, 51, start, start + prepaid + 1);
    set(&mut b, 52, start + prepaid + 1, start + 1);
    set(&mut b, 103, start + 1, start + 1 + refund);
    set(&mut b, 104, start + 1 + refund, start + 1);
    assert!(refused(&b));
    // Exactly one GAS_BUY, the sender's, of the prepaid gas: not of another
    // account...
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 52).address = other.clone();
    set(&mut b, 52, prepaid, 0);
    set(&mut b, 103, start + prepaid, start + prepaid + refund);
    set(&mut b, 104, start + prepaid + refund, start + prepaid);
    assert!(refused(&b));
    // ...nor at another gas price than the base fee.
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 52).new_value = wei(1_144_742_593_324_625).into();
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    let buy = record(&mut b, 52).clone();
    b.transaction_traces[1].calls[0].balance_changes.push(eth::BalanceChange {
        ordinal: 60,
        old_value: wei(1).into(),
        new_value: None.into(),
        ..buy
    });
    assert!(refused(&b));
    // The gas left is refunded and burnt: the refund of another gas left...
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].gas_used -= 1;
    assert!(refused(&b));
    // ...no record after the execution, or the GAS_REFUND without its burn and
    // the payments, although the sender chains through the mint either way...
    for last in [102, 103] {
        let mut b = captured_arbitrum_failed_retry();
        b.transaction_traces[1].calls[0].balance_changes.retain(|c| c.ordinal <= last);
        assert!(refused(&b));
    }
    // ...a GAS_REFUND of another account, a TRANSFER in its place, or a
    // GAS_REFUND of another amount than the burn...
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 103).address = other.clone();
    set(&mut b, 103, 0, refund);
    set(&mut b, 104, start, start - refund);
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 103).reason = eth::balance_change::Reason::Transfer.into();
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    set(&mut b, 103, start, start + refund + 1);
    set(&mut b, 104, start + refund + 1, start + 1);
    assert!(refused(&b));
    // ...a GAS_REFUND at the root call's end ordinal, not after the
    // execution...
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 103).ordinal = 102;
    assert!(refused(&b));
    // ...a burn of another reason, of another account or of another amount...
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 104).reason = eth::balance_change::Reason::Burn.into();
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 104).address = other.clone();
    set(&mut b, 104, refund, 0);
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    set(&mut b, 104, start + refund, start - 1);
    assert!(refused(&b));
    // ...a payment of another amount than the debit of the fee account, or a
    // debit of another amount than the payment...
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 106).new_value = wei(146_044_608_097_841_378).into();
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    set(&mut b, 105, fee_before, fee_after - 1);
    assert!(refused(&b));
    // ...a debit or a payment of another reason...
    for ordinal in [105, 106] {
        let mut b = captured_arbitrum_failed_retry();
        record(&mut b, ordinal).reason = eth::balance_change::Reason::Burn.into();
        assert!(refused(&b));
    }
    // ...a fee account paying itself...
    let mut b = captured_arbitrum_failed_retry();
    let fee = record(&mut b, 105).address.clone();
    record(&mut b, 106).address = fee;
    set(&mut b, 106, fee_after, fee_before);
    assert!(refused(&b));
    // ...a reversed pair, a credit of the fee account and a debit of the
    // refund address, even when a larger payment makes up for it...
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].end_ordinal = 110;
    set(&mut b, 105, fee_before, fee_after - 1_000);
    set(&mut b, 106, paid_before, paid_after + 1_000);
    let (debit, credit) = (record(&mut b, 105).clone(), record(&mut b, 106).clone());
    b.transaction_traces[1].calls[0].balance_changes.extend([
        eth::BalanceChange {
            old_value: wei(fee_after - 1_000).into(),
            new_value: wei(fee_after).into(),
            ordinal: 107,
            ..debit
        },
        eth::BalanceChange {
            old_value: wei(paid_after + 1_000).into(),
            new_value: wei(paid_after).into(),
            ordinal: 108,
            ..credit
        },
    ]);
    assert!(refused(&b));
    // ...a multi-dimensional gas refund, one more payment of the network fee
    // account to the refund address after its refund of the gas left (see
    // `failed_retry_settlements`)...
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].end_ordinal = 110;
    let (debit, credit) = (record(&mut b, 105).clone(), record(&mut b, 106).clone());
    b.transaction_traces[1].calls[0].balance_changes.extend([
        eth::BalanceChange {
            old_value: wei(fee_after).into(),
            new_value: wei(fee_after - 1_000).into(),
            ordinal: 107,
            ..debit
        },
        eth::BalanceChange {
            old_value: wei(paid_after).into(),
            new_value: wei(paid_after + 1_000).into(),
            ordinal: 108,
            ..credit
        },
    ]);
    assert!(refused(&b));
    // ...payments that add up to another amount than the refund, more or less
    // (here nothing, as when a fee account cannot pay its refund and Nitro
    // skips it)...
    let mut b = captured_arbitrum_failed_retry();
    record(&mut b, 105).new_value = wei(919_240_220_428_078_778_327).into();
    record(&mut b, 106).new_value = wei(146_044_608_097_841_378).into();
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].calls[0].balance_changes.retain(|c| c.ordinal < 105);
    assert!(refused(&b));
    // ...or another record after the execution fails the block.
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].calls[0].balance_changes.push(eth::BalanceChange {
        address: vec![9; 20],
        old_value: None.into(),
        new_value: wei(1).into(),
        ordinal: 107,
        reason: eth::balance_change::Reason::RewardTransactionFee.into(),
    });
    assert!(refused(&b));
    // A payment to the sender, past the ticket's maximum refund, settles too.
    let mut b = captured_arbitrum_failed_retry();
    let payment = record(&mut b, 106);
    payment.address = sender.clone();
    payment.old_value = wei(1_144_742_593_324_624).into();
    payment.new_value = wei(1_144_857_993_324_624).into();
    let rows = changes(&b).unwrap();
    assert_eq!(
        (row_of(&rows, &sender).amount.as_str(), row_of(&rows, &sender).records),
        ("1144857993324624", 10)
    );
    // A failed retry without balance records settles nothing.
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].calls[0].balance_changes.clear();
    assert_eq!(row_of(&changes(&b).unwrap(), &sender).ordinal, 44);
    // A succeeded retry persists every record anyway, here the same ones.
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].status = eth::TransactionTraceStatus::Succeeded.into();
    b.transaction_traces[1].calls.iter_mut().for_each(|c| c.state_reverted = false);
    assert_eq!(changes(&b).unwrap(), changes(&captured_arbitrum_failed_retry()).unwrap());
    // No-op records are left out of the shape: a no-op TRANSFER of the sender
    // after the payment settles the same rows.
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].calls[0].balance_changes.push(eth::BalanceChange {
        address: sender.clone(),
        old_value: wei(start).into(),
        new_value: wei(start).into(),
        ordinal: 107,
        reason: eth::balance_change::Reason::Transfer.into(),
    });
    assert_eq!(changes(&b).unwrap(), changes(&captured_arbitrum_failed_retry()).unwrap());
    // A value past uint256, new or old, fails the block, beside the settlement
    // (here in the reverted execution) or as the retry's only record.
    let past_uint256 = Some(eth::BigInt { bytes: vec![1; 33] });
    for (old_value, new_value) in [(None, past_uint256.clone()), (past_uint256, None)] {
        let oversize = eth::BalanceChange {
            address: other.clone(),
            old_value: old_value.into(),
            new_value: new_value.into(),
            ordinal: 60,
            reason: eth::balance_change::Reason::Transfer.into(),
        };
        let mut b = captured_arbitrum_failed_retry();
        b.transaction_traces[1].calls[0].balance_changes.push(oversize.clone());
        assert!(refused(&b));
        let mut b = captured_arbitrum_failed_retry();
        b.transaction_traces[1].calls[0].balance_changes = vec![oversize];
        assert!(refused(&b));
    }
    // A call value without the escrow's moves fails the block.
    let mut b = captured_arbitrum_failed_retry();
    b.transaction_traces[1].value = wei(1).into();
    assert!(refused(&b));

    // With call value (block 314,259,052): the sender's balance moves from 0
    // to `v` and back, and the escrow's from `v` to 0 and back. From the
    // escrow to the sender before the prepaid gas...
    let value = 6_025_454_000_000_000_u128;
    let value_sender = hex::decode(RETRY_VALUE_SENDER).unwrap();
    let mut b = captured_arbitrum_failed_retry_value();
    b.transaction_traces[1].value = wei(6_025_454_000_000_001).into();
    assert!(refused(&b));
    // ...as TRANSFER records: a debit of `v` of another account than the
    // sender, which is not its own escrow...
    let mut b = captured_arbitrum_failed_retry_value();
    record(&mut b, 263).reason = eth::balance_change::Reason::Burn.into();
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry_value();
    set(&mut b, 263, value + 1, 0);
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry_value();
    record(&mut b, 263).address = value_sender.clone();
    record(&mut b, 287).address = value_sender.clone();
    assert!(refused(&b));
    // ...and a credit of `v` of the sender...
    let mut b = captured_arbitrum_failed_retry_value();
    record(&mut b, 264).reason = eth::balance_change::Reason::Burn.into();
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry_value();
    record(&mut b, 264).address = other.clone();
    assert!(refused(&b));
    let (value_prepaid, value_refund) = (14_105_668_920_000_u128, 9_747_444_132_000_u128);
    let mut b = captured_arbitrum_failed_retry_value();
    set(&mut b, 264, 0, value + 1);
    set(&mut b, 265, value + 1, value + 1 + value_prepaid);
    set(&mut b, 266, value + 1 + value_prepaid, value + 1);
    set(&mut b, 280, value + 1, value + 1 + value_refund);
    set(&mut b, 281, value + 1 + value_refund, value + 1);
    set(&mut b, 286, value + 1, 1);
    assert!(refused(&b));
    // ...before the prepaid gas...
    let mut b = captured_arbitrum_failed_retry_value();
    let moves = &mut b.transaction_traces[1].calls[0].balance_changes;
    let at = |moves: &[eth::BalanceChange], ordinal| moves.iter().position(|c| c.ordinal == ordinal).unwrap();
    let (credit, mint) = (at(moves, 264), at(moves, 265));
    (moves[credit].ordinal, moves[mint].ordinal) = (265, 264);
    assert!(refused(&b));
    // ...one refund address for both fee accounts' payments...
    let mut b = captured_arbitrum_failed_retry_value();
    record(&mut b, 285).address = vec![9; 20];
    assert!(refused(&b));
    // ...which the sender never counts as. Past the ticket's maximum refund,
    // here used up by the infrastructure fee account's payment to the refund
    // address, the network fee account pays the sender...
    let mut b = captured_arbitrum_failed_retry_value();
    record(&mut b, 285).address = value_sender.clone();
    set(&mut b, 285, value, value + 8_866_104_132_000);
    set(&mut b, 286, value + 8_866_104_132_000, 8_866_104_132_000);
    assert_eq!(row_of(&changes(&b).unwrap(), &value_sender).amount, "8866104132000");
    // ...or, when the maximum refund runs out within its payment, splits it:
    // the refund address first, then the sender...
    let mut b = captured_arbitrum_failed_retry_value();
    b.transaction_traces[1].end_ordinal = 291;
    for ordinal in [287, 286] {
        record(&mut b, ordinal).ordinal = ordinal + 2;
    }
    let (network, split, rest) = (54_091_183_612_536_855_824_u128, 8_000_000_000_000_u128, 866_104_132_000_u128);
    set(&mut b, 284, network, network - split);
    set(&mut b, 285, 14_015_778_403_599_631, 14_015_778_403_599_631 + split);
    let (debit, credit) = (record(&mut b, 284).clone(), record(&mut b, 288).clone());
    b.transaction_traces[1].calls[0].balance_changes.extend([
        eth::BalanceChange {
            old_value: wei(network - split).into(),
            new_value: wei(network - split - rest).into(),
            ordinal: 286,
            ..debit
        },
        eth::BalanceChange {
            old_value: wei(value).into(),
            new_value: wei(value + rest).into(),
            ordinal: 287,
            ..credit
        },
    ]);
    set(&mut b, 288, value + rest, rest);
    assert_rows(
        &changes(&b).unwrap(),
        &[
            (RETRY_VALUE_SENDER, "0", "866104132000", 288, 7),
            ("2cd28cda6825c4967372478e87d004637b73f996", "14042830579263631", "14023778403599631", 285, 3),
            (NETWORK_FEE, "54091162600531997824", "54091174746432723824", 286, 3),
        ],
    );
    // ...but no payment to the refund address follows one to the sender (see
    // `failed_retry_settlements`): the infrastructure fee account paying the
    // sender before the network fee account pays the refund address fails the
    // block, although every account chains.
    let mut b = captured_arbitrum_failed_retry_value();
    record(&mut b, 283).address = value_sender.clone();
    set(&mut b, 283, value, value + 881_340_000_000);
    set(&mut b, 285, 14_014_897_063_599_631, 14_023_763_167_731_631);
    set(&mut b, 286, value + 881_340_000_000, 881_340_000_000);
    assert!(refused(&b));
    // ...and back to the escrow at the end...
    let mut b = captured_arbitrum_failed_retry_value();
    record(&mut b, 287).address = vec![9; 20];
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry_value();
    b.transaction_traces[1].calls[0].balance_changes.retain(|c| c.ordinal < 286);
    assert!(refused(&b));
    // ...as TRANSFER records of `v`, with nothing after them.
    for ordinal in [286, 287] {
        let mut b = captured_arbitrum_failed_retry_value();
        record(&mut b, ordinal).reason = eth::balance_change::Reason::Burn.into();
        assert!(refused(&b));
    }
    let mut b = captured_arbitrum_failed_retry_value();
    set(&mut b, 286, value, 1);
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry_value();
    set(&mut b, 287, 0, value + 1);
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry_value();
    b.transaction_traces[1].calls[0].balance_changes.push(eth::BalanceChange {
        address: other.clone(),
        old_value: None.into(),
        new_value: wei(1).into(),
        ordinal: 288,
        reason: eth::balance_change::Reason::Transfer.into(),
    });
    assert!(refused(&b));
    // Without gas left (block 511,131,266) there is no refund to burn, and a
    // gas use past the gas limit fails the block.
    let mut b = captured_arbitrum_failed_retry_out_of_gas();
    b.transaction_traces[1].gas_used -= 1;
    assert!(refused(&b));
    let mut b = captured_arbitrum_failed_retry_out_of_gas();
    b.transaction_traces[1].gas_used += 1;
    assert!(refused(&b));
}

/// Polygon block 70,000,124 (`Block.ver` 3) as captured from Firehose:
/// transactions 0 and 1 whole. Each creates an account with value in its
/// CREATE root call, whose constructor self-destructs it to `0x91cea390…`.
fn captured_polygon_selfdestruct_created() -> eth::Block {
    captured_block(&serde_json::from_str(POLYGON_SELFDESTRUCT_CREATED).unwrap())
}
const POLYGON_DESTROYED: &str = "3e58cebde7b4ea3e70966846d246984fcb3e56aa";

#[test]
fn captured_polygon_selfdestruct_of_a_created_account_debits_it() {
    let b = captured_polygon_selfdestruct_created();
    // The producer records no SUICIDE_WITHDRAW.
    let reasons = b.transaction_traces.iter().flat_map(|t| &t.calls).flat_map(|c| &c.balance_changes);
    assert!(!reasons
        .into_iter()
        .any(|c| c.reason.as_known() == Some(eth::balance_change::Reason::SuicideWithdraw)));
    let rows = changes(&b).unwrap();
    for (account, ordinal) in [(POLYGON_DESTROYED, 15), ("a136fffaf93f3fbdeab7f856537af8fa08fbb76c", 37)] {
        let row = row_of(&rows, &hex::decode(account).unwrap());
        assert_eq!((row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records), ("0", "0", ordinal, 2));
    }
    let row = row_of(&rows, &hex::decode("91cea3906add2da84b99970a068598d08a7ad81f").unwrap());
    assert_eq!((row.old_amount.as_str(), row.amount.as_str(), row.records), ("0", "252860035266903168", 2));
}

/// Ethereum block 19,500,178 (`Block.ver` 3) as captured from Firehose:
/// transaction 2 (`0xa7aa96be…`) whole. Its CREATE root call creates
/// `0x727f6fc6…` with 0.001 ETH; the account receives value during the
/// execution and self-destructs to the sender in that root call.
#[test]
fn captured_ver3_selfdestruct_in_a_creation_root_call_debits_the_account() {
    let b = captured_block(&serde_json::from_str(SELFDESTRUCT_CREATED_ROOT).unwrap());
    let created = hex::decode("727f6fc68ee3f91aa1119ce86ca6662e80230fb4").unwrap();
    let root = &b.transaction_traces[0].calls[0];
    assert!(root.suicide && root.call_type.as_known() == Some(eth::CallType::CREATE) && root.address == created);
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &created);
    assert_eq!((row.old_amount.as_str(), row.amount.as_str(), row.ordinal, row.records), ("0", "0", 1211, 9));
}

#[test]
fn the_selfdestruct_debit_with_other_records_is_exact() {
    fn destroyed(b: &eth::Block) -> String {
        row_of(&changes(b).unwrap(), &hex::decode(POLYGON_DESTROYED).unwrap()).amount.clone()
    }
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[0].calls[0]
    }
    fn record(address: &str, ordinal: u64, old: u64, new: u64, reason: eth::balance_change::Reason) -> eth::BalanceChange {
        eth::BalanceChange {
            address: hex::decode(address).unwrap(),
            reason: reason.into(),
            ..change(0, ordinal, Some(old), Some(new))
        }
    }
    const D: u64 = 106_671_293_064_353_727;
    // Without the self-destruct, the account keeps the balance it was created with.
    let mut b = captured_polygon_selfdestruct_created();
    root(&mut b).suicide = false;
    assert_eq!(destroyed(&b), D.to_string());
    // When the producer records the debit, it is used, not doubled.
    let mut b = captured_polygon_selfdestruct_created();
    let withdraw = record(POLYGON_DESTROYED, 14, D, 0, eth::balance_change::Reason::SuicideWithdraw);
    root(&mut b).balance_changes.insert(3, withdraw);
    let rows = changes(&b).unwrap();
    let row = row_of(&rows, &hex::decode(POLYGON_DESTROYED).unwrap());
    assert_eq!((row.amount.as_str(), row.records), ("0", 2));
    // Not with a later record of the destroyed account in the call.
    let mut b = captured_polygon_selfdestruct_created();
    let later = record(POLYGON_DESTROYED, 16, D, D + 1, eth::balance_change::Reason::Transfer);
    root(&mut b).balance_changes.insert(4, later);
    assert_eq!(destroyed(&b), (D + 1).to_string());
    // Not with a second SUICIDE_REFUND in the call.
    let mut b = captured_polygon_selfdestruct_created();
    let refund = record(SENDER, 16, 0, 1, eth::balance_change::Reason::SuicideRefund);
    root(&mut b).balance_changes.insert(4, refund);
    assert_eq!(destroyed(&b), D.to_string());
}

/// Arbitrum One block 250,005,039 (`Block.ver` 3) as captured from Firehose:
/// transaction 2 (`0xe293fa3a…`) whole. Its CREATE root call creates
/// `0x06ef189a…` with 1,955,820,000,000 wei, which its constructor
/// self-destructs to `0x0f0cb393…`.
fn captured_arbitrum_stale_withdraw() -> eth::Block {
    captured_block(&serde_json::from_str(ARBITRUM_STALE_WITHDRAW).unwrap())
}
const STALE_DESTROYED: &str = "06ef189a6a359d4ff01a3b27f09ebaca3b150148";
const STALE_BENEFICIARY: &str = "0f0cb39319129ba867227e5aae1abe9e7dd5f861";

#[test]
fn captured_arbitrum_stale_selfdestruct_withdraw_is_dropped_and_the_account_chains_across_it() {
    use eth::balance_change::Reason;
    // The producer's records of the destroyed account in block 250,005,039:
    // the CREATE's value, the debit, a no-op and the debit again from the
    // balance before the first one.
    let b = captured_arbitrum_stale_withdraw();
    let destroyed = hex::decode(STALE_DESTROYED).unwrap();
    let own: Vec<_> = b.transaction_traces[0].calls[0]
        .balance_changes
        .iter()
        .filter(|c| c.address == destroyed)
        .map(|c| {
            let [old, new] = [&c.old_value, &c.new_value].map(|v| amount(&value(v.as_option().map(|v| v.bytes.as_slice())).unwrap()));
            (c.ordinal, old, new, c.reason.to_i32())
        })
        .collect();
    let d = "1955820000000".to_string();
    let zero = "0".to_string();
    assert_eq!(
        own,
        [
            (127, zero.clone(), d.clone(), Reason::Transfer as i32),
            (132, d.clone(), zero.clone(), Reason::SuicideWithdraw as i32),
            (134, zero.clone(), zero.clone(), Reason::SuicideWithdraw as i32),
            (135, d.clone(), zero.clone(), Reason::SuicideWithdraw as i32)
        ]
    );
    // Per block and account: (old amount, amount, first ordinal, ordinal,
    // records). The chain reads each old amount at the block before and each
    // amount after the block. Without the self-destruct flag, each block
    // breaks continuity.
    let cases = [
        (
            ARBITRUM_STALE_WITHDRAW,
            [
                (STALE_DESTROYED, "0", "0", 127, 132, 2),
                (STALE_BENEFICIARY, "131468500000", "2087288500000", 133, 133, 1),
            ],
        ),
        // The credit creates the beneficiary's account at ordinal 18, between
        // the debit (17) and the credit (19).
        (
            ARBITRUM_STALE_WITHDRAW_NEW_BENEFICIARY,
            [
                ("8bedff2953789a02a032c1d6372f78a3932da692", "0", "0", 14, 17, 2),
                ("60657119fa7d2c64862db12b4afd812b70b899e7", "0", "1073210000000", 19, 19, 1),
            ],
        ),
        // A nested CREATE of an account that already holds 0.0581 ETH, to the
        // sender, whose gas refund starts from the credited balance.
        (
            ARBITRUM_STALE_WITHDRAW_NESTED,
            [
                ("19df9b676db4601ea38763ebe588364185f86a91", "58100000000000000", "0", 270, 270, 1),
                (
                    "22face80f43b857141e9752c3bae8c3309fcdd0f",
                    "510062447229718264",
                    "568160970789718264",
                    250,
                    280,
                    3,
                ),
            ],
        ),
        // Before Cancun: the refund (516), the debit (517) and the repeat (518).
        (
            ARBITRUM_STALE_WITHDRAW_BEFORE_CANCUN,
            [
                ("88e6002626290b4c244f239327e58d9235f0f4d6", "0", "0", 513, 517, 2),
                ("b244f017e0a662bba2ba100bc5dec2e8ad52ddd9", "24878000000000", "774878000000000", 516, 516, 1),
            ],
        ),
        // Block 450,028,504, transaction 5 (`0xabef75c6…`): the CREATE root
        // call credits `0xc05da4ea…` 15,786,592,425,450 wei, which its
        // constructor self-destructs to `0xd211dbe9…`, credited from zero.
        (
            include_str!("../tests/fixtures/arbitrum-450028504-stale-selfdestruct-withdraw.json"),
            [
                ("c05da4ea416614af2e2a067efbca74f175c06e20", "0", "0", 185, 188, 2),
                ("d211dbe903af354658238731b75354f522f11bee", "0", "15786592425450", 189, 189, 1),
            ],
        ),
    ];
    for (fixture, expected) in cases {
        let b = captured_block(&serde_json::from_str(fixture).unwrap());
        assert!(changes(&{
            let mut kept = b.clone();
            kept.transaction_traces[0].calls.iter_mut().for_each(|c| c.suicide = false);
            kept
        })
        .unwrap_err()
        .to_string()
        .contains("discontinuous"));
        let rows = changes(&b).unwrap();
        for (account, old, new, first_ordinal, ordinal, records) in expected {
            let row = row_of(&rows, &hex::decode(account).unwrap());
            assert_eq!(
                (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
                (old, new, first_ordinal, ordinal, records),
                "block {} account {account}",
                b.number
            );
        }
    }
}

#[test]
fn the_stale_selfdestruct_withdraw_shape_is_exact() {
    use eth::balance_change::Reason;
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    // Block 250,005,039's root call: records 3 to 6 are the debit (132), the
    // credit (133), the no-op (134) and the repeated debit (135).
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[0].calls[0]
    }
    fn record(address: &str, ordinal: u64, old: u64, new: u64, reason: Reason) -> eth::BalanceChange {
        eth::BalanceChange {
            address: hex::decode(address).unwrap(),
            reason: reason.into(),
            ..change(0, ordinal, Some(old), Some(new))
        }
    }
    const D: u64 = 1_955_820_000_000;
    let destroyed = |b: &eth::Block| {
        let rows = changes(b).unwrap();
        let row = row_of(&rows, &hex::decode(STALE_DESTROYED).unwrap());
        (row.amount.clone(), row.ordinal, row.records)
    };
    assert_eq!(destroyed(&captured_arbitrum_stale_withdraw()), ("0".to_string(), 132, 2));
    // Only a self-destructing call.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).suicide = false;
    assert!(fails(&b));
    // The order with a no-op only of an account created in the transaction.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).call_type = eth::CallType::CALL.into();
    assert!(fails(&b));
    // Only a credit of exactly the debited amount.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[4].new_value = big(131_468_500_000 + D + 1).into();
    assert!(fails(&b));
    // A refund to the account itself is not this shape: its records chain as
    // recorded, and the repeated debit is kept.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[4] = record(STALE_DESTROYED, 133, 0, D, Reason::SuicideRefund);
    assert_eq!(destroyed(&b), ("0".to_string(), 135, 4));
    // Only three withdraws: the no-op must be one, of the account.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[5].reason = Reason::Transfer.into();
    assert!(fails(&b));
    root(&mut b).balance_changes.push(record(STALE_DESTROYED, 141, 0, 0, Reason::SuicideWithdraw));
    assert!(fails(&b));
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[5].address = hex::decode(SENDER).unwrap();
    assert!(fails(&b));
    // No fourth withdraw, and no second refund, in the call.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes.push(record(STALE_DESTROYED, 141, 0, 0, Reason::SuicideWithdraw));
    assert!(fails(&b));
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes.push(record(SENDER, 141, 0, 1, Reason::SuicideRefund));
    assert!(fails(&b));
    // The debit, the no-op and the repeated debit are to zero, the no-op from
    // zero.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[3].new_value = big(1).into();
    assert!(fails(&b));
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[5] = record(STALE_DESTROYED, 134, D, D, Reason::SuicideWithdraw);
    assert!(fails(&b));
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[6].new_value = big(1).into();
    assert!(fails(&b));
    // The repeated debit repeats the first one's old value.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[6].old_value = big(D - 1).into();
    assert!(fails(&b));
    // At consecutive ordinals: the refund right after the debit, the no-op
    // and the repeated debit right after the refund.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[3].ordinal = 131;
    assert!(fails(&b));
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[6].ordinal = 136;
    assert!(fails(&b));
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[5].ordinal = 135;
    root(&mut b).balance_changes[6].ordinal = 136;
    assert!(fails(&b));
    // The record between the debits is the refund, not another credit.
    let mut b = captured_arbitrum_stale_withdraw();
    root(&mut b).balance_changes[4].reason = Reason::Transfer.into();
    root(&mut b).balance_changes.push(record(SENDER, 141, 0, 1, Reason::SuicideRefund));
    assert!(fails(&b));
    // Two ordinals apart only when the call creates the beneficiary's account
    // between them (block 200,029,376).
    let new_beneficiary = || captured_block(&serde_json::from_str(ARBITRUM_STALE_WITHDRAW_NEW_BENEFICIARY).unwrap());
    let mut b = new_beneficiary();
    let creation = root(&mut b).account_creations.iter_mut().find(|a| a.ordinal == 18).unwrap();
    creation.account = hex::decode(SENDER).unwrap();
    assert!(fails(&b));
    let mut b = new_beneficiary();
    root(&mut b).account_creations.retain(|a| a.ordinal != 18);
    assert!(fails(&b));
    // A balance between the debit and the repeated debit makes the latter a
    // real debit, which must chain and is kept: the account ends at zero.
    let mut b = captured_arbitrum_stale_withdraw();
    b.transaction_traces[0].calls.push(eth::Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        balance_changes: vec![record(STALE_DESTROYED, 134, 0, D, Reason::Transfer)],
        ..Default::default()
    });
    assert_eq!(destroyed(&b), ("0".to_string(), 135, 4));
    // Any other balance there breaks continuity.
    b.transaction_traces[0].calls[1].balance_changes[0].new_value = big(D - 1).into();
    assert!(fails(&b));
    // The repeated debit is the account's only record at its ordinal: a
    // duplicate there, here a credit in a child call, is not dropped with
    // it, and the block fails.
    let child = |ordinal: u64, new: u64| eth::Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        balance_changes: vec![record(STALE_DESTROYED, ordinal, 0, new, Reason::Transfer)],
        ..Default::default()
    };
    let mut b = captured_arbitrum_stale_withdraw();
    b.transaction_traces[0].calls.push(child(135, 7));
    assert!(fails(&b));
    // No record of the account follows the repeated debit in the
    // transaction: a later credit keeps the repeat, which breaks continuity.
    let mut b = captured_arbitrum_stale_withdraw();
    b.transaction_traces[0].calls.push(child(136, 5));
    assert!(fails(&b));
    // A record of the account in a later transaction is not part of the
    // shape: it chains from zero.
    let mut b = captured_arbitrum_stale_withdraw();
    b.transaction_traces.push(eth::TransactionTrace {
        index: 3,
        begin_ordinal: 144,
        end_ordinal: 160,
        ..tx(eth::Call {
            index: 1,
            balance_changes: vec![record(STALE_DESTROYED, 150, 0, 5, Reason::Transfer)],
            ..Default::default()
        })
    });
    assert_eq!(destroyed(&b), ("5".to_string(), 150, 3));

    // Before Cancun (block 187,009,069): records 3 to 5 are the refund (516),
    // the debit (517) and the repeat (518).
    let before_cancun = || captured_block(&serde_json::from_str(ARBITRUM_STALE_WITHDRAW_BEFORE_CANCUN).unwrap());
    let account = hex::decode("88e6002626290b4c244f239327e58d9235f0f4d6").unwrap();
    let row = |b: &eth::Block| {
        let rows = changes(b).unwrap();
        let row = row_of(&rows, &account);
        (row.amount.clone(), row.ordinal, row.records)
    };
    assert_eq!(row(&before_cancun()), ("0".to_string(), 517, 2));
    // The debit right after the refund, the repeat right after the debit.
    let mut b = before_cancun();
    root(&mut b).balance_changes[3].ordinal = 515;
    assert!(fails(&b));
    let mut b = before_cancun();
    root(&mut b).balance_changes[5].ordinal = 519;
    assert!(fails(&b));
    // The refund first.
    let mut b = before_cancun();
    root(&mut b).balance_changes[3].ordinal = 517;
    root(&mut b).balance_changes[4].ordinal = 516;
    assert!(fails(&b));
    // No other withdraw.
    let withdraw = |ordinal| record("88e6002626290b4c244f239327e58d9235f0f4d6", ordinal, 0, 0, Reason::SuicideWithdraw);
    let mut b = before_cancun();
    root(&mut b).balance_changes.push(withdraw(519));
    assert!(fails(&b));
    root(&mut b).balance_changes.push(withdraw(520));
    assert!(fails(&b));
    // The debit is a withdraw.
    let mut b = before_cancun();
    root(&mut b).balance_changes[4].reason = Reason::Transfer.into();
    root(&mut b).balance_changes.push(withdraw(519));
    assert!(fails(&b));
}

/// Arbitrum One block 514,819,388 (`Block.ver` 3) as captured from Firehose:
/// transaction 4 (`0xae385f2d…`) whole. Its root call sends
/// 3,495,043,890,571 wei to `0x06bba4e1…`, a contract not created in the
/// transaction (code `0x600035ff`), which self-destructs it to
/// `0xf3359221…` and survives (EIP-6780).
fn captured_arbitrum_stale_withdraw_existing() -> eth::Block {
    captured_block(&serde_json::from_str(include_str!("../tests/fixtures/arbitrum-514819388-stale-selfdestruct-withdraw-existing.json")).unwrap())
}
const STALE_EXISTING: &str = "06bba4e1619ae7ed8332978643399de6cf7de80b";
const STALE_EXISTING_BENEFICIARY: &str = "f33592213d255d01ac35176d18e0bba676a5e9b6";
/// Arbitrum One block 176,115,455 (`Block.ver` 3, before Cancun) as captured
/// from Firehose: transaction 1 (`0xbfc4af67…`) whole. Its call 6 (depth 4)
/// self-destructs `0x9d4eb718…`, which holds 15,785 wei and code before the
/// block and is not created in the transaction, to `0xc881a48a…`.
fn captured_arbitrum_stale_withdraw_existing_before_cancun() -> eth::Block {
    captured_block(&serde_json::from_str(include_str!("../tests/fixtures/arbitrum-176115455-stale-selfdestruct-withdraw-existing.json")).unwrap())
}
const STALE_EXISTING_BEFORE_CANCUN: &str = "9d4eb7189cd57693c3d01f35168715e1e589cea8";

#[test]
fn captured_arbitrum_stale_selfdestruct_withdraws_of_existing_accounts_are_dropped() {
    use eth::balance_change::Reason;
    // Per block: the self-destructing call; the account and the
    // beneficiary; their records in the call as (ordinal, of the account,
    // old, new, reason); the repeat's ordinal; and every row as (account, old
    // amount, amount, first ordinal, ordinal, records). The chain reads the
    // account's, the beneficiary's and the sender's old amounts at the block
    // before and their amounts at the block.
    type Own<'a> = [(u64, bool, &'a str, &'a str, Reason)];
    type Rows<'a> = [(&'a str, &'a str, &'a str, u64, u64, u32)];
    let check = |b: &eth::Block, call: usize, accounts: [&str; 2], own: &Own, repeat: u64, expected: &Rows| {
        // No call of the transaction is a CREATE.
        let calls = &b.transaction_traces[0].calls;
        assert!(calls.iter().all(|c| c.call_type.as_known() != Some(eth::CallType::CREATE)));
        let [destroyed, beneficiary] = accounts.map(|a| hex::decode(a).unwrap());
        let recorded: Vec<_> = calls[call]
            .balance_changes
            .iter()
            .filter(|c| c.address == destroyed || c.address == beneficiary)
            .map(|c| {
                let [old, new] = [&c.old_value, &c.new_value].map(|v| amount(&value(v.as_option().map(|v| v.bytes.as_slice())).unwrap()));
                (c.ordinal, c.address == destroyed, old, new, c.reason.to_i32())
            })
            .collect();
        let own: Vec<_> = own
            .iter()
            .map(|&(o, d, old, new, reason)| (o, d, old.to_string(), new.to_string(), reason as i32))
            .collect();
        assert_eq!(recorded, own, "block {}", b.number);
        // Without the self-destruct flag the repeat breaks continuity.
        let mut kept = b.clone();
        kept.transaction_traces[0].calls[call].suicide = false;
        assert!(changes(&kept).unwrap_err().to_string().contains("discontinuous"));
        // Only the repeat is dropped; the other self-destruct rules do not
        // apply.
        let persisted = records(b).unwrap();
        assert_eq!(stale_selfdestruct_withdraws(b, &persisted), HashSet::from([(destroyed, repeat)]));
        assert!(selfdestruct_debits(b).is_empty());
        assert!(selfdestruct_to_self_pairs(b).is_empty());
        assert!(selfdestruct_credit_shifts(b, &persisted).is_empty());
        let rows = changes(b).unwrap();
        assert_eq!(rows.len(), expected.len());
        for &(account, old, new, first_ordinal, ordinal, records) in expected {
            let row = row_of(&rows, &hex::decode(account).unwrap());
            assert_eq!(
                (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
                (old, new, first_ordinal, ordinal, records),
                "block {} account {account}",
                b.number
            );
        }
    };
    // Since Cancun (block 514,819,388): the call's value, the debit, the
    // refund and the debit again from the balance before the first one.
    let d = "3495043890571";
    check(
        &captured_arbitrum_stale_withdraw_existing(),
        0,
        [STALE_EXISTING, STALE_EXISTING_BENEFICIARY],
        &[
            (170, true, "0", d, Reason::Transfer),
            (172, true, d, "0", Reason::SuicideWithdraw),
            (173, false, "110368952201", "3605412842772", Reason::SuicideRefund),
            (174, true, d, "0", Reason::SuicideWithdraw),
        ],
        174,
        &[
            (STALE_EXISTING, "0", "0", 170, 172, 2),
            (STALE_EXISTING_BENEFICIARY, "110368952201", "3605412842772", 173, 173, 1),
            (
                "49f55da8050ed4b938da68fb1fabf487fe60dead",
                "22744118335173678",
                "22740026651253275",
                164,
                176,
                3,
            ),
            (
                "32e7af5a8151934f3787d0cd59eb6edd0a736b1d",
                "2370381355647832615",
                "2370381355647861589",
                177,
                177,
                1,
            ),
            (
                "bf5041fc07e1c866d15c749156657b8eed0fb649",
                "3733095255360007000",
                "3733095834840007000",
                178,
                178,
                1,
            ),
            (
                "a4b00000000000000000000000000000000000f6",
                "3514730428826339127",
                "3514730445986339985",
                179,
                179,
                1,
            ),
        ],
    );
    // Block 367,502,338, transaction 8 (`0x5ecb2e03…`): a call (depth 1) of
    // `0x6beb920b…`, which holds 5,000,000,000,000 wei and code before the
    // block and keeps its code, self-destructs it to the sender
    // `0x62005500…`, whose gas refund then starts from the credited balance.
    check(
        &captured_block(&serde_json::from_str(include_str!("../tests/fixtures/arbitrum-367502338-stale-selfdestruct-withdraw-existing.json")).unwrap()),
        3,
        ["6beb920bf8f42d753ee61d6a54a0ad219b5505fa", "62005500af4cfb0077ac0090002f630055ba001d"],
        &[
            (451, true, "5000000000000", "0", Reason::SuicideWithdraw),
            (452, false, "27162147567422131", "27167147567422131", Reason::SuicideRefund),
            (453, true, "5000000000000", "0", Reason::SuicideWithdraw),
        ],
        453,
        &[
            ("6beb920bf8f42d753ee61d6a54a0ad219b5505fa", "5000000000000", "0", 451, 451, 1),
            (
                "62005500af4cfb0077ac0090002f630055ba001d",
                "27202147567422131",
                "27206645657422131",
                429,
                467,
                3,
            ),
            (
                "bf5041fc07e1c866d15c749156657b8eed0fb649",
                "25344038395940000000",
                "25344038856830000000",
                468,
                468,
                1,
            ),
            (
                "a4b00000000000000000000000000000000000f6",
                "3514415020224215870",
                "3514415061244215870",
                469,
                469,
                1,
            ),
        ],
    );
    // Before Cancun (block 176,115,455): the refund, the debit and the
    // repeat, as for an account created in the transaction.
    check(
        &captured_arbitrum_stale_withdraw_existing_before_cancun(),
        5,
        [STALE_EXISTING_BEFORE_CANCUN, "c881a48a5c90a751d53cfb0af82a5336b74a1389"],
        &[
            (37, false, "1", "15786", Reason::SuicideRefund),
            (38, true, "15785", "0", Reason::SuicideWithdraw),
            (39, true, "15785", "0", Reason::SuicideWithdraw),
        ],
        39,
        &[
            (STALE_EXISTING_BEFORE_CANCUN, "15785", "0", 38, 38, 1),
            ("c881a48a5c90a751d53cfb0af82a5336b74a1389", "1", "15786", 37, 37, 1),
            ("7374e5a0aa341f8b9c1df17bbd3ea156a5bfb927", "8624563462587122", "8442970562587122", 6, 53, 2),
            (
                "bf5041fc07e1c866d15c749156657b8eed0fb649",
                "690630634027600000000",
                "690630642024200000000",
                54,
                54,
                1,
            ),
            (
                "a4b00000000000000000000000000000000000f6",
                "4933556405096608890",
                "4933730001396608890",
                55,
                55,
                1,
            ),
        ],
    );
}

#[test]
fn the_stale_selfdestruct_withdraw_of_an_existing_account_is_exact() {
    use eth::balance_change::Reason;
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    // Block 514,819,388's root call: records 2 to 5 are the call's value
    // (170), the debit (172), the refund (173) and the repeat (174).
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[0].calls[0]
    }
    fn record(address: &str, ordinal: u64, old: u64, new: u64, reason: Reason) -> eth::BalanceChange {
        eth::BalanceChange {
            address: hex::decode(address).unwrap(),
            reason: reason.into(),
            ..change(0, ordinal, Some(old), Some(new))
        }
    }
    fn child(index: u32, balance_changes: Vec<eth::BalanceChange>) -> eth::Call {
        eth::Call {
            index,
            parent_index: 1,
            depth: 1,
            balance_changes,
            ..Default::default()
        }
    }
    const D: u64 = 3_495_043_890_571;
    const B: u64 = 110_368_952_201;
    let destroyed = |b: &eth::Block| {
        let rows = changes(b).unwrap();
        let row = row_of(&rows, &hex::decode(STALE_EXISTING).unwrap());
        (row.amount.clone(), row.ordinal, row.records)
    };
    let dropped = |b: &eth::Block| stale_selfdestruct_withdraws(b, &records(b).unwrap());
    let captured = captured_arbitrum_stale_withdraw_existing;
    assert_eq!(destroyed(&captured()), ("0".to_string(), 172, 2));
    // The debit first without a no-op only of an account not created in the
    // transaction: a CREATE of it calls for the other two orders.
    let mut b = captured();
    b.transaction_traces[0].calls.push(eth::Call {
        call_type: eth::CallType::CREATE.into(),
        address: hex::decode(STALE_EXISTING).unwrap(),
        ..child(2, vec![])
    });
    assert!(fails(&b));
    // Only of the account the call self-destructs: for a DELEGATECALL, its
    // caller.
    let mut b = captured();
    root(&mut b).call_type = eth::CallType::DELEGATE.into();
    assert!(fails(&b));
    // Only a credit of exactly the debited amount.
    let mut b = captured();
    root(&mut b).balance_changes[4].new_value = big(B + D + 1).into();
    assert!(fails(&b));
    // A refund to the account itself is not this shape: the repeat is not
    // dropped.
    let mut b = captured();
    root(&mut b).balance_changes[4] = record(STALE_EXISTING, 173, 0, D, Reason::SuicideRefund);
    assert!(dropped(&b).is_empty());
    // Exactly two withdraws, both of the account, and one refund in the call.
    let mut b = captured();
    root(&mut b).balance_changes.push(record(STALE_EXISTING, 175, 0, 0, Reason::SuicideWithdraw));
    assert!(fails(&b));
    let mut b = captured();
    root(&mut b).balance_changes[5].address = hex::decode(STALE_EXISTING_BENEFICIARY).unwrap();
    assert!(fails(&b));
    let mut b = captured();
    root(&mut b)
        .balance_changes
        .push(record(STALE_EXISTING_BENEFICIARY, 175, B + D, B + D + 1, Reason::SuicideRefund));
    assert!(fails(&b));
    // The record between the debits is the refund, not another credit.
    let mut b = captured();
    root(&mut b).balance_changes[4].reason = Reason::Transfer.into();
    root(&mut b)
        .balance_changes
        .push(record(STALE_EXISTING_BENEFICIARY, 175, B + D, B + D + 1, Reason::SuicideRefund));
    assert!(fails(&b));
    // The created account's no-op between the refund and the repeat is not
    // part of this shape.
    let mut b = captured();
    root(&mut b).balance_changes[5].ordinal = 175;
    root(&mut b).balance_changes.push(record(STALE_EXISTING, 174, 0, 0, Reason::SuicideWithdraw));
    assert!(fails(&b));
    // The debit and the repeat are to zero, the repeat from the debited
    // amount.
    let mut b = captured();
    root(&mut b).balance_changes[3].new_value = big(1).into();
    assert!(fails(&b));
    let mut b = captured();
    root(&mut b).balance_changes[5].new_value = big(1).into();
    assert!(fails(&b));
    let mut b = captured();
    root(&mut b).balance_changes[5].old_value = big(D - 1).into();
    assert!(fails(&b));
    // At consecutive ordinals, the debit first: the refund right after the
    // debit, or two ordinals later when the call creates the beneficiary's
    // account between them (since v0.7.9), and the repeat right after the
    // refund.
    let mut b = captured();
    root(&mut b).balance_changes[3].ordinal = 171;
    assert!(fails(&b));
    root(&mut b).account_creations.push(eth::AccountCreation {
        account: hex::decode(STALE_EXISTING_BENEFICIARY).unwrap(),
        ordinal: 172,
    });
    assert_eq!(destroyed(&b), ("0".to_string(), 171, 2));
    root(&mut b).account_creations[0].account = hex::decode(STALE_EXISTING).unwrap();
    assert!(fails(&b));
    let mut b = captured();
    root(&mut b).balance_changes[5].ordinal = 175;
    assert!(fails(&b));
    // With the refund first, the records are in the order before Cancun,
    // which is recognized too.
    let mut b = captured();
    root(&mut b).balance_changes[3].ordinal = 173;
    root(&mut b).balance_changes[4].ordinal = 172;
    assert_eq!(destroyed(&b), ("0".to_string(), 173, 2));
    // A balance between the debit and the repeat makes the latter a real
    // debit, which must chain and is kept: the account ends at zero.
    let mut b = captured();
    b.transaction_traces[0]
        .calls
        .push(child(2, vec![record(STALE_EXISTING, 173, 0, D, Reason::Transfer)]));
    assert_eq!(destroyed(&b), ("0".to_string(), 174, 4));
    b.transaction_traces[0].calls[1].balance_changes[0].new_value = big(D - 1).into();
    assert!(fails(&b));
    // The repeat is the account's only record at its ordinal, and its last
    // in the transaction.
    let mut b = captured();
    b.transaction_traces[0]
        .calls
        .push(child(2, vec![record(STALE_EXISTING, 174, 0, 7, Reason::Transfer)]));
    assert!(fails(&b));
    let mut b = captured();
    b.transaction_traces[0]
        .calls
        .push(child(2, vec![record(STALE_EXISTING, 175, 0, 5, Reason::Transfer)]));
    assert!(fails(&b));
    // A record of the account in a later transaction chains from zero.
    let mut b = captured();
    b.transaction_traces.push(eth::TransactionTrace {
        index: 5,
        begin_ordinal: 188,
        end_ordinal: 200,
        ..tx(eth::Call {
            index: 1,
            balance_changes: vec![record(STALE_EXISTING, 190, 0, 5, Reason::Transfer)],
            ..Default::default()
        })
    });
    assert_eq!(destroyed(&b), ("5".to_string(), 190, 3));

    // Before Cancun (block 176,115,455): call 6 holds the refund (37), the
    // debit (38) and the repeat (39).
    let before_cancun = captured_arbitrum_stale_withdraw_existing_before_cancun;
    fn call(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[0].calls[5]
    }
    let account = |b: &eth::Block| {
        let rows = changes(b).unwrap();
        let row = row_of(&rows, &hex::decode(STALE_EXISTING_BEFORE_CANCUN).unwrap());
        (row.amount.clone(), row.ordinal, row.records)
    };
    assert_eq!(account(&before_cancun()), ("0".to_string(), 38, 1));
    // The debit right after the refund, the repeat right after the debit.
    let mut b = before_cancun();
    call(&mut b).balance_changes[0].ordinal = 36;
    assert!(fails(&b));
    let mut b = before_cancun();
    call(&mut b).balance_changes[2].ordinal = 40;
    assert!(fails(&b));
    // Only a credit of exactly the debited amount, of another account.
    let mut b = before_cancun();
    call(&mut b).balance_changes[0].new_value = big(15_787).into();
    assert!(fails(&b));
    let mut b = before_cancun();
    call(&mut b).balance_changes[0] = record(STALE_EXISTING_BEFORE_CANCUN, 37, 0, 15_785, Reason::SuicideRefund);
    assert!(dropped(&b).is_empty());
    // No other withdraw, and no other refund, in the call.
    let mut b = before_cancun();
    call(&mut b)
        .balance_changes
        .push(record(STALE_EXISTING_BEFORE_CANCUN, 40, 0, 0, Reason::SuicideWithdraw));
    assert!(fails(&b));
    let mut b = before_cancun();
    call(&mut b).balance_changes.push(record(SENDER, 40, 0, 1, Reason::SuicideRefund));
    assert!(fails(&b));
}

/// Arbitrum One block 235,153,999 (`Block.ver` 3, ArbOS 20) as captured
/// from Firehose: transactions 0 (ArbOS's internal transaction) and 1
/// (`0xd2061702…`) whole. Transaction 1's root call self-destructs
/// `0x85483824…`, which holds 0.001 ETH and is not created in the
/// transaction, to `0x0bb2668e…`, whose account the call creates between the
/// debit and the refund.
const STALE_WITHDRAW_NEW_BENEFICIARY: &str = include_str!("../tests/fixtures/arbitrum-235153999-stale-selfdestruct-withdraw-new-beneficiary.json");
/// Arbitrum One block 223,944,083 in the same shape: transaction 1
/// (`0xd8ec4935…`) self-destructs `0x006bf202…` to `0x2dafa472…`.
const STALE_WITHDRAW_NEW_BENEFICIARY_2: &str = include_str!("../tests/fixtures/arbitrum-223944083-stale-selfdestruct-withdraw-new-beneficiary.json");

#[test]
fn captured_arbitrum_repeated_withdraw_of_an_existing_account_to_a_new_beneficiary_is_dropped() {
    use eth::balance_change::Reason;
    // Per block: the destroyed account and the beneficiary, d, and every row
    // as (account, old amount, amount, first ordinal, ordinal, records). The
    // RPC-era rows read zero for both destroyed accounts at their blocks and
    // 1,000,000,000,000,000 wei for 0x0bb2668e… at block 235,153,999.
    type Rows<'a> = [(&'a str, &'a str, &'a str, u64, u64, u32)];
    let cases: [(&str, [&str; 2], &str, &Rows); 2] = [
        (
            STALE_WITHDRAW_NEW_BENEFICIARY,
            ["854838245429a63224aa054ac0ed73f78a068338", "0bb2668e26c8e17baa8e0537aac1a68946fc2a69"],
            "1000000000000000",
            &[
                ("854838245429a63224aa054ac0ed73f78a068338", "1000000000000000", "0", 12, 12, 1),
                ("0bb2668e26c8e17baa8e0537aac1a68946fc2a69", "0", "1000000000000000", 14, 14, 1),
                ("7d8b6ad195512b6549b141aa180460d36a3cc16b", "150944438384329786", "150943690974329786", 6, 17, 2),
                (INFRA_FEE, "8665511765200000000", "8665512331430000000", 18, 18, 1),
                (L1_PRICER, "3513415450866377170", "3513415632046377170", 19, 19, 1),
            ],
        ),
        (
            STALE_WITHDRAW_NEW_BENEFICIARY_2,
            ["006bf202e5c4d104c3739e0022011efe8b42e370", "2dafa47247d3fe48676111636dcdcf854cab8517"],
            "2832000000000000",
            &[
                ("006bf202e5c4d104c3739e0022011efe8b42e370", "2832000000000000", "0", 12, 12, 1),
                ("2dafa47247d3fe48676111636dcdcf854cab8517", "0", "2832000000000000", 14, 14, 1),
                ("19a59dbbb50a03afb1521bc6aed41d626653dc63", "38741708827171986", "38741141767171986", 6, 17, 2),
                (INFRA_FEE, "19349429749260000000", "19349430316320000000", 18, 18, 1),
            ],
        ),
    ];
    for (fixture, accounts, d, expected) in cases {
        let b = captured_block(&serde_json::from_str(fixture).unwrap());
        let [destroyed, beneficiary] = accounts.map(|a| hex::decode(a).unwrap());
        let trx = &b.transaction_traces[1];
        assert!(trx.calls.iter().all(|c| c.call_type.as_known() != Some(eth::CallType::CREATE)));
        // The records: the debit (12), the beneficiary's creation (13), its
        // refund (14) and the repeat (15).
        let root = &trx.calls[0];
        assert!(root.suicide);
        let recorded: Vec<_> = root
            .balance_changes
            .iter()
            .filter(|c| c.address == destroyed || c.address == beneficiary)
            .map(|c| {
                let [old, new] = [&c.old_value, &c.new_value].map(|v| amount(&value(v.as_option().map(|v| v.bytes.as_slice())).unwrap()));
                (c.ordinal, c.address == destroyed, old, new, c.reason.to_i32())
            })
            .collect();
        let (d, zero) = (d.to_string(), "0".to_string());
        assert_eq!(
            recorded,
            [
                (12, true, d.clone(), zero.clone(), Reason::SuicideWithdraw as i32),
                (14, false, zero.clone(), d.clone(), Reason::SuicideRefund as i32),
                (15, true, d.clone(), zero.clone(), Reason::SuicideWithdraw as i32)
            ]
        );
        assert_eq!(
            root.account_creations.iter().map(|a| (a.account.clone(), a.ordinal)).collect::<Vec<_>>(),
            [(beneficiary.clone(), 13)]
        );
        // Only the repeat is dropped; native v0.7.8 kept it, and it breaks
        // continuity.
        let persisted = records(&b).unwrap();
        assert_eq!(stale_selfdestruct_withdraws(&b, &persisted), HashSet::from([(destroyed.clone(), 15)]));
        assert!(selfdestruct_debits(&b).is_empty());
        assert!(selfdestruct_to_self_pairs(&b).is_empty());
        let mut kept = b.clone();
        kept.transaction_traces[1].calls[0].suicide = false;
        assert!(changes(&kept).unwrap_err().to_string().contains("discontinuous"));
        let rows = changes(&b).unwrap();
        assert_eq!(rows.len(), expected.len(), "block {}", b.number);
        for &(account, old, new, first_ordinal, ordinal, records) in expected {
            let row = row_of(&rows, &hex::decode(account).unwrap());
            assert_eq!(
                (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
                (old, new, first_ordinal, ordinal, records),
                "block {} account {account}",
                b.number
            );
        }
    }
}

#[test]
fn the_repeated_withdraw_of_an_existing_account_to_a_new_beneficiary_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    // Records 1 to 3 of transaction 1's root call: the debit (12), the
    // refund (14) and the repeat (15); the beneficiary's creation at 13.
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[1].calls[0]
    }
    let captured = || captured_block(&serde_json::from_str(STALE_WITHDRAW_NEW_BENEFICIARY).unwrap());
    let destroyed = hex::decode("854838245429a63224aa054ac0ed73f78a068338").unwrap();
    let ends = |b: &eth::Block| row_of(&changes(b).unwrap(), &destroyed).amount.clone();
    assert_eq!(ends(&captured()), "0");
    // The call creates the beneficiary's account...
    let mut b = captured();
    root(&mut b).account_creations.clear();
    assert!(fails(&b));
    // ...and no other account...
    let mut b = captured();
    root(&mut b).account_creations[0].account = vec![0x11; 20];
    assert!(fails(&b));
    // ...at the ordinal between the debit and the refund.
    for ordinal in [12, 14] {
        let mut b = captured();
        root(&mut b).account_creations[0].ordinal = ordinal;
        assert!(fails(&b), "creation at {ordinal}");
    }
    // A creation in another call of the transaction does not count.
    let mut b = captured();
    let creation = root(&mut b).account_creations.remove(0);
    b.transaction_traces[1].calls.push(eth::Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        account_creations: vec![creation],
        ..Default::default()
    });
    assert!(fails(&b));
    // The refund at most two ordinals after the debit, and the repeat right
    // after the refund.
    let mut b = captured();
    root(&mut b).balance_changes[2].ordinal = 15;
    root(&mut b).balance_changes[3].ordinal = 16;
    root(&mut b).account_creations[0].ordinal = 14;
    assert!(fails(&b));
    let mut b = captured();
    root(&mut b).balance_changes[3].ordinal = 16;
    assert!(fails(&b));
    // Of an account created in the transaction, the shape with a creation
    // gap is the debit, the refund, a no-op and the repeat: these two
    // withdraws are not it.
    let mut b = captured();
    b.transaction_traces[1].calls.push(eth::Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        call_type: eth::CallType::CREATE.into(),
        address: destroyed.clone(),
        ..Default::default()
    });
    assert!(fails(&b));
}

/// Arbitrum One block 161,626,224 (`Block.ver` 3, ArbOS 10) as captured from
/// Firehose: both its transactions whole, ArbOS's internal transaction and
/// `0xd0cc7c02…`, whose root call self-destructs `0xb01f8f52…` to itself.
const LEGACY_SELFDESTRUCT_TO_SELF: &str = include_str!("../tests/fixtures/arbitrum-161626224-legacy-selfdestruct-to-self-root.json");
/// Arbitrum One block 136,808,179 (ArbOS 10): transactions 0 and 5
/// (`0x585194b7…`) whole; call 4 of transaction 5 self-destructs
/// `0xe37e799d…` to itself.
const LEGACY_SELFDESTRUCT_TO_SELF_NESTED: &str = include_str!("../tests/fixtures/arbitrum-136808179-legacy-selfdestruct-to-self.json");
const LEGACY_TO_SELF: &str = "b01f8f528702d411d24c9bb8cc0e2fff779ec013";
/// Sets the ArbOS version in an Arbitrum block's mix hash.
fn with_arbos(mut b: eth::Block, version: u64) -> eth::Block {
    if let Some(header) = b.header.as_option_mut() {
        header.mix_hash[16..24].copy_from_slice(&version.to_be_bytes());
    }
    b
}

#[test]
fn the_arbos_version_is_read_from_arbitrum_blocks_only() {
    let fixture = |f: &str| captured_block(&serde_json::from_str(f).unwrap());
    for (f, version) in [
        (LEGACY_SELFDESTRUCT_TO_SELF_NESTED, 10),
        (LEGACY_SELFDESTRUCT_TO_SELF, 10),
        (STALE_WITHDRAW_NEW_BENEFICIARY_2, 20),
        (STALE_WITHDRAW_NEW_BENEFICIARY, 20),
    ] {
        let b = fixture(f);
        assert_eq!(arbos_version(&b), Some(version), "block {}", b.number);
        assert_eq!(arbos_version(&with_arbos(b, 51)), Some(51));
    }
    // Only with ArbOS's internal transaction first...
    let b = fixture(LEGACY_SELFDESTRUCT_TO_SELF);
    let mut t = b.clone();
    t.transaction_traces.remove(0);
    assert_eq!(arbos_version(&t), None);
    let mut t = b.clone();
    t.transaction_traces.swap(0, 1);
    assert_eq!(arbos_version(&t), None);
    let mut t = b.clone();
    t.transaction_traces[0].from = vec![0x11; 20];
    assert_eq!(arbos_version(&t), None);
    let mut t = b.clone();
    t.transaction_traces[0].r#type = eth::transaction_trace::Type::TrxTypeArbitrumDeposit.into();
    assert_eq!(arbos_version(&t), None);
    // ...and Nitro's mix digest.
    let mut t = b.clone();
    t.header.as_option_mut().unwrap().mix_hash[31] = 1;
    assert_eq!(arbos_version(&t), None);
    let mut t = b.clone();
    t.header.as_option_mut().unwrap().mix_hash.pop();
    assert_eq!(arbos_version(&t), None);
    // Other networks' blocks have none.
    for f in [
        SELFDESTRUCT_TO_SELF,
        BASE_FEE_VAULT_CREDITS,
        POLYGON_SELFDESTRUCT_CREATED,
        HYPEREVM_SELF_TRANSFER,
    ] {
        assert_eq!(arbos_version(&fixture(f)), None);
    }
    assert_eq!(arbos_version(&eth::Block::decode_from_slice(FIXTURE_BLOCK).unwrap()), None);
}

#[test]
fn captured_arbitrum_legacy_selfdestruct_to_itself_burns_the_balance() {
    use eth::balance_change::Reason;
    // Per block: the self-destructing call, the account, d, the rewritten
    // withdraw's ordinal, and every row as (account, old amount, amount,
    // first ordinal, ordinal, records). The RPC-era rows read zero for
    // 0xb01f8f52… at block 161,626,317, after the same sender creates it
    // again without a balance record, and for 0xe37e799d… at block
    // 136,808,944. The sender's GAS_BUY in block 161,626,317 starts from its
    // amount here.
    type Rows<'a> = [(&'a str, &'a str, &'a str, u64, u64, u32)];
    let cases: [(&str, usize, &str, &str, u64, &Rows); 2] = [
        (
            LEGACY_SELFDESTRUCT_TO_SELF,
            0,
            LEGACY_TO_SELF,
            "3304510818628470",
            13,
            &[
                (LEGACY_TO_SELF, "3304510818628470", "0", 12, 13, 2),
                ("11799622f4d98a24514011e8527b969f7488ef47", "137542296535450983", "137498524335450983", 6, 15, 2),
                (INFRA_FEE, "557337082226500000000", "557337084865400000000", 16, 16, 1),
                (L1_PRICER, "6661356271684088330", "6661397404984088330", 17, 17, 1),
            ],
        ),
        (
            LEGACY_SELFDESTRUCT_TO_SELF_NESTED,
            3,
            "e37e799d5077682fa0a244d46e5649f71457bd09",
            "1",
            448,
            &[
                ("e37e799d5077682fa0a244d46e5649f71457bd09", "1", "0", 447, 448, 2),
                (
                    "11799622f4d98a24514011e8527b969f7488ef47",
                    "150801797835450983",
                    "150696165035450983",
                    422,
                    457,
                    2,
                ),
                (INFRA_FEE, "2286587649727400000000", "2286587656302800000000", 458, 458, 1),
                (L1_PRICER, "3132293909802631104", "3132392967202631104", 459, 459, 1),
            ],
        ),
    ];
    for (fixture, call, account, d, withdraw, expected) in cases {
        let b = captured_block(&serde_json::from_str(fixture).unwrap());
        let account = hex::decode(account).unwrap();
        // The refund of the account from d to 2d, then the withdraw from d to
        // zero at the next ordinal.
        let suicide = &b.transaction_traces[1].calls[call];
        assert!(suicide.suicide && suicide.address == account);
        let own: Vec<_> = suicide
            .balance_changes
            .iter()
            .filter(|c| c.address == account)
            .map(|c| {
                let [old, new] =
                    [&c.old_value, &c.new_value].map(|v| BigInt::from_unsigned_bytes_be(&value(v.as_option().map(|v| v.bytes.as_slice())).unwrap()));
                (c.ordinal, old, new, c.reason.to_i32())
            })
            .collect();
        let d = BigInt::from_str(d).unwrap();
        assert_eq!(
            own,
            [
                (withdraw - 1, d.clone(), d.clone() + d.clone(), Reason::SuicideRefund as i32),
                (withdraw, d.clone(), BigInt::from(0), Reason::SuicideWithdraw as i32)
            ]
        );
        // From ArbOS 20 the opcode is EIP-6780's, and the records keep native
        // v0.7.8's refusal, as in a block of another network.
        for refused in [with_arbos(b.clone(), 20), {
            let mut t = b.clone();
            t.transaction_traces.remove(0);
            t
        }] {
            assert!(changes(&refused).unwrap_err().to_string().contains("discontinuous"));
        }
        // No other self-destruct rule takes the records.
        let persisted = records(&b).unwrap();
        assert!(stale_selfdestruct_withdraws(&b, &persisted).is_empty());
        assert!(selfdestruct_to_self_pairs(&b).is_empty());
        assert!(selfdestruct_debits(&b).is_empty());
        let doubled = word(&(d.clone() + d).to_bytes_be().1).unwrap().to_vec();
        assert_eq!(
            legacy_selfdestruct_to_self_rewrites(&b, &persisted, arbos_version(&b)),
            HashMap::from([((account.clone(), withdraw), (doubled, vec![0; 32]))])
        );
        let rows = changes(&b).unwrap();
        assert_eq!(rows.len(), expected.len(), "block {}", b.number);
        for &(account, old, new, first_ordinal, ordinal, records) in expected {
            let row = row_of(&rows, &hex::decode(account).unwrap());
            assert_eq!(
                (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
                (old, new, first_ordinal, ordinal, records),
                "block {} account {account}",
                b.number
            );
        }
    }
}

#[test]
fn the_legacy_selfdestruct_to_itself_shape_is_exact() {
    use eth::balance_change::Reason;
    const D: u64 = 3_304_510_818_628_470;
    fn discontinuous(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    // Records 1 and 2 of transaction 1's root call: the refund (12) and the
    // withdraw (13).
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[1].calls[0]
    }
    fn record(address: &str, ordinal: u64, old: u64, new: u64, reason: Reason) -> eth::BalanceChange {
        eth::BalanceChange {
            address: hex::decode(address).unwrap(),
            reason: reason.into(),
            ..change(0, ordinal, Some(old), Some(new))
        }
    }
    let captured = || captured_block(&serde_json::from_str(LEGACY_SELFDESTRUCT_TO_SELF).unwrap());
    let account = |b: &eth::Block| {
        let rows = changes(b).unwrap();
        let row = row_of(&rows, &hex::decode(LEGACY_TO_SELF).unwrap());
        (row.amount.clone(), row.records)
    };
    assert_eq!(account(&captured()), ("0".to_string(), 2));
    // Before ArbOS 20 only, in an Arbitrum block.
    for version in [0, 19] {
        assert_eq!(account(&with_arbos(captured(), version)), ("0".to_string(), 2));
    }
    for version in [20, 21, 51] {
        assert!(discontinuous(&with_arbos(captured(), version)), "ArbOS {version}");
    }
    // The refund doubles the balance the withdraw starts from.
    let mut b = captured();
    root(&mut b).balance_changes[1].new_value = big(2 * D + 1).into();
    assert!(discontinuous(&b));
    let mut b = captured();
    root(&mut b).balance_changes[1].old_value = big(D - 1).into();
    root(&mut b).balance_changes[1].new_value = big(2 * D - 2).into();
    assert!(discontinuous(&b));
    // The withdraw to zero, at the ordinal right after the refund.
    let mut b = captured();
    root(&mut b).balance_changes[2].new_value = big(1).into();
    assert!(discontinuous(&b));
    let mut b = captured();
    root(&mut b).balance_changes[2].ordinal = 14;
    assert!(discontinuous(&b));
    // A withdraw from the doubled balance already chains, and is not
    // rewritten.
    let mut b = captured();
    root(&mut b).balance_changes[2].old_value = big(2 * D).into();
    assert!(legacy_selfdestruct_to_self_rewrites(&b, &records(&b).unwrap(), arbos_version(&b)).is_empty());
    assert_eq!(account(&b), ("0".to_string(), 2));
    // Exactly one refund and one withdraw in the call, both of the account
    // the call self-destructs.
    let mut b = captured();
    root(&mut b).balance_changes.push(record(LEGACY_TO_SELF, 14, 0, 0, Reason::SuicideWithdraw));
    assert!(discontinuous(&b));
    let mut b = captured();
    root(&mut b).balance_changes.push(record(INFRA_FEE, 14, 0, 0, Reason::SuicideRefund));
    assert!(discontinuous(&b));
    let mut b = captured();
    root(&mut b).call_type = eth::CallType::DELEGATE.into();
    assert!(discontinuous(&b));
    let mut b = captured();
    root(&mut b).suicide = false;
    assert!(discontinuous(&b));
    for reason in [Reason::Transfer, Reason::SuicideWithdraw] {
        let mut b = captured();
        root(&mut b).balance_changes[1].reason = reason.into();
        assert!(discontinuous(&b), "{reason:?}");
    }
    // No record of the account after the withdraw in the transaction.
    let mut b = captured();
    root(&mut b).balance_changes.push(record(LEGACY_TO_SELF, 18, 0, 5, Reason::Transfer));
    assert!(discontinuous(&b));
    // The legacy opcode zeroes an account created in the transaction too.
    let mut b = captured();
    b.transaction_traces[1].calls.push(eth::Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        call_type: eth::CallType::CREATE.into(),
        address: hex::decode(LEGACY_TO_SELF).unwrap(),
        ..Default::default()
    });
    assert_eq!(account(&b), ("0".to_string(), 2));
}

/// Arbitrum One block 514,819,388's self-destruct as if to itself: its
/// refund at ordinal 173 credits `0x06bba4e1…` from zero to `d` instead of
/// the beneficiary `0xf3359221…`. No such self-destruct is captured.
fn arbitrum_selfdestruct_to_itself() -> eth::Block {
    let mut b = captured_arbitrum_stale_withdraw_existing();
    b.transaction_traces[0].calls[0].balance_changes[4] = eth::BalanceChange {
        address: hex::decode(STALE_EXISTING).unwrap(),
        reason: eth::balance_change::Reason::SuicideRefund.into(),
        ..change(0, 173, Some(0), Some(3_495_043_890_571))
    };
    b
}
fn refused_as_selfdestruct_to_itself(b: &eth::Block) -> bool {
    changes(b).is_err_and(|e| e.to_string().contains("unrecognized self-destruct to itself"))
}

#[test]
fn an_arbitrum_selfdestruct_to_itself_of_an_existing_account_fails_the_block() {
    use eth::balance_change::Reason;
    let b = arbitrum_selfdestruct_to_itself();
    let account = hex::decode(STALE_EXISTING).unwrap();
    // The account's records: the call's value, then Arbitrum's order for an
    // account not created in the transaction, the debit, the refund (here
    // of the account itself, from zero) and the repeat.
    let own: Vec<_> = records(&b)
        .unwrap()
        .iter()
        .filter(|r| r.address == account)
        .map(|r| {
            let [old, new] = [&r.old_value, &r.new_value].map(|v| amount(&value(v.as_deref()).unwrap()));
            (r.ordinal, old, new, r.reason)
        })
        .collect();
    let (d, zero) = ("3495043890571".to_string(), "0".to_string());
    assert_eq!(
        own,
        [
            (170, zero.clone(), d.clone(), Reason::Transfer as i32),
            (172, d.clone(), zero.clone(), Reason::SuicideWithdraw as i32),
            (173, zero.clone(), d.clone(), Reason::SuicideRefund as i32),
            (174, d.clone(), zero.clone(), Reason::SuicideWithdraw as i32)
        ]
    );
    // These records chain and end at zero, while since Cancun (EIP-6780) the
    // account keeps `d`: without the self-destruct flag the block passes
    // with the account at zero, and no other self-destruct rule takes them.
    let mut unflagged = b.clone();
    unflagged.transaction_traces[0].calls[0].suicide = false;
    let rows = changes(&unflagged).unwrap();
    let row = row_of(&rows, &account);
    assert_eq!((row.amount.as_str(), row.records), ("0", 4));
    let persisted = records(&b).unwrap();
    assert!(stale_selfdestruct_withdraws(&b, &persisted).is_empty());
    assert!(selfdestruct_to_self_pairs(&b).is_empty());
    assert!(selfdestruct_debits(&b).is_empty());
    assert!(selfdestruct_credit_shifts(&b, &persisted).is_empty());
    // So the block fails, naming the account and the transaction.
    let error = changes(&b).unwrap_err().to_string();
    assert!(
        error.contains(
            "unrecognized self-destruct to itself: 0x06bba4e1619ae7ed8332978643399de6cf7de80b in transaction \
             0xae385f2d908729e19003b207a772e2eab9e3e38c029d8a0863324e6e6c04e2b6"
        ),
        "{error}"
    );
    assert!(project(&b).is_err());
}

#[test]
fn the_selfdestruct_to_itself_refusal_is_exact() {
    use eth::balance_change::Reason;
    const D: u64 = 3_495_043_890_571;
    // Records 2 to 5 of the root call: the call's value (170), the debit
    // (172), the refund (173) and the repeat (174).
    fn root(b: &mut eth::Block) -> &mut eth::Call {
        &mut b.transaction_traces[0].calls[0]
    }
    fn record(ordinal: u64, old: u64, new: u64, reason: Reason) -> eth::BalanceChange {
        eth::BalanceChange {
            address: hex::decode(STALE_EXISTING).unwrap(),
            reason: reason.into(),
            ..change(0, ordinal, Some(old), Some(new))
        }
    }
    let refused = refused_as_selfdestruct_to_itself;
    let discontinuous = |b: &eth::Block| changes(b).is_err_and(|e| e.to_string().contains("discontinuous"));
    let account = |b: &eth::Block| {
        let rows = changes(b).unwrap();
        let row = row_of(&rows, &hex::decode(STALE_EXISTING).unwrap());
        (row.amount.clone(), row.records)
    };
    let to_itself = arbitrum_selfdestruct_to_itself;
    assert!(refused(&to_itself()));
    // The captured self-destructs to another beneficiary, and Ethereum's
    // `Block.ver` 3 records of a self-destruct to itself (the refund from
    // zero, then the withdraw), are not this shape.
    assert_eq!(account(&captured_arbitrum_stale_withdraw_existing()), ("0".to_string(), 2));
    let rows = changes(&captured_selfdestruct_to_self()).unwrap();
    assert_eq!(row_of(&rows, &hex::decode(TO_SELF).unwrap()).amount, "1");
    // Only of an account not created in the transaction: a created account
    // is destroyed, so its records end at its balance, zero.
    let mut b = to_itself();
    b.transaction_traces[0].calls.push(eth::Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        call_type: eth::CallType::CREATE.into(),
        address: hex::decode(STALE_EXISTING).unwrap(),
        ..Default::default()
    });
    assert_eq!(account(&b), ("0".to_string(), 4));
    // Only of a persisted call: not of a reverted call, nor in a failed
    // transaction.
    let mut b = to_itself();
    root(&mut b).state_reverted = true;
    assert!(!refused(&b));
    let mut b = to_itself();
    b.transaction_traces[0].status = eth::TransactionTraceStatus::Failed.into();
    assert!(!refused(&b));
    // Only of the account the call self-destructs: for a DELEGATECALL, its
    // caller.
    let mut b = to_itself();
    root(&mut b).call_type = eth::CallType::DELEGATE.into();
    assert!(!refused(&b));
    root(&mut b).caller = hex::decode(STALE_EXISTING).unwrap();
    root(&mut b).address = hex::decode(STALE_EXISTING_BENEFICIARY).unwrap();
    assert!(refused(&b));
    // The debit first: with the refund first, as before Cancun, the records
    // do not chain.
    let mut b = to_itself();
    root(&mut b).balance_changes[3].ordinal = 173;
    root(&mut b).balance_changes[4].ordinal = 172;
    assert!(!refused(&b) && discontinuous(&b));
    // Exactly the debit, the refund and the repeat: with the debit and the
    // refund only, the account ends at `d`; a further withdraw does not
    // chain.
    let mut b = to_itself();
    root(&mut b).balance_changes.remove(5);
    assert_eq!(account(&b), (D.to_string(), 3));
    let mut b = to_itself();
    root(&mut b).balance_changes.push(record(175, D, 0, Reason::SuicideWithdraw));
    assert!(!refused(&b) && discontinuous(&b));
    // The refund is a SUICIDE_REFUND, and the debit and the repeat
    // SUICIDE_WITHDRAWs.
    for (index, reason) in [
        (3, Reason::SuicideRefund),
        (4, Reason::SuicideWithdraw),
        (5, Reason::SuicideRefund),
        (4, Reason::Transfer),
    ] {
        let mut b = to_itself();
        root(&mut b).balance_changes[index].reason = reason.into();
        assert!(!refused(&b), "record {index}");
    }
    // Only a refund of the account itself: a beneficiary credited from zero
    // (as in block 407,513,390) is the self-destruct to another account,
    // whose repeat is dropped.
    let mut b = captured_arbitrum_stale_withdraw_existing();
    let c = &mut root(&mut b).balance_changes[4];
    (c.old_value, c.new_value) = (big(0).into(), big(D).into());
    assert!(!refused(&b));
    assert_eq!(account(&b), ("0".to_string(), 2));
    // In this order by ordinal: two records at one ordinal are ambiguous.
    for (index, ordinal) in [(4, 172), (5, 173)] {
        let mut b = to_itself();
        root(&mut b).balance_changes[index].ordinal = ordinal;
        assert!(changes(&b).unwrap_err().to_string().contains("ambiguous"), "record {index}");
    }
    // The debit from `d` to zero, the refund from zero to `d` and the repeat
    // from `d` to zero: other values do not chain, or end at another balance.
    for (index, old, new) in [(3, D, 1), (4, 1, D), (4, 0, D + 1), (5, D - 1, 0)] {
        let mut b = to_itself();
        let c = &mut root(&mut b).balance_changes[index];
        (c.old_value, c.new_value) = (big(old).into(), big(new).into());
        assert!(!refused(&b) && discontinuous(&b), "record {index}");
    }
    let mut b = to_itself();
    root(&mut b).balance_changes[5].new_value = big(1).into();
    assert_eq!(account(&b), ("1".to_string(), 4));
    // Gaps between the ordinals and no-ops between the records do not
    // matter: the records end at zero all the same.
    let mut b = to_itself();
    root(&mut b).balance_changes[3].ordinal = 171;
    assert!(refused(&b));
    let mut b = to_itself();
    root(&mut b).balance_changes[5].ordinal = 175;
    root(&mut b).balance_changes.insert(5, record(174, 0, 0, Reason::SuicideWithdraw));
    assert!(refused(&b));
}

/// Base block 5,044,695 (`Block.ver` 4) as captured from Firehose:
/// transaction 1 (`0xe707514e…`) whole, a deposit from and to `0x82c8c1fb…`
/// that fails outside the EVM; its root call holds only the mint.
fn captured_base_failed_deposit() -> eth::Block {
    captured_block(&serde_json::from_str(BASE_FAILED_DEPOSIT).unwrap())
}
/// Base block 7,092,606 (`Block.ver` 4) as captured from Firehose:
/// transaction 1 (`0x3869ca9e…`) whole, a deposit from `0x8117a188…` whose
/// call to `0x00008453…` reverts; its root call holds the mint and the
/// reverted value transfer.
fn captured_base_failed_deposit_reverted_call() -> eth::Block {
    captured_block(&serde_json::from_str(BASE_FAILED_DEPOSIT_REVERTED_CALL).unwrap())
}
const DEPOSIT_SENDER: &str = "82c8c1fb820029bb0cf8337c4248ef45b7ae6927";
const REVERTED_DEPOSIT_SENDER: &str = "8117a1888cd0cbf5c294f3ff179c5d243ee573a5";
const REVERTED_DEPOSIT_RECIPIENT: &str = "00008453e27e8e88f305f13cf27c30d724fdd055";

#[test]
fn captured_failed_deposits_keep_their_mint() {
    use eth::balance_change::Reason;
    let b = captured_base_failed_deposit();
    let tx = &b.transaction_traces[0];
    assert_eq!(
        (tx.r#type.as_known(), tx.status.as_known()),
        (
            Some(eth::transaction_trace::Type::TrxTypeOptimismDeposit),
            Some(eth::TransactionTraceStatus::Failed)
        )
    );
    // The mint persists like a failed transaction's gas.
    let sender = hex::decode(DEPOSIT_SENDER).unwrap();
    let persisted: Vec<_> = records(&b).unwrap().into_iter().map(|r| (r.address, r.ordinal, r.reason, r.scope)).collect();
    assert_eq!(
        persisted,
        [(sender.clone(), 25, Reason::IncreaseMint as i32, persist::Scope::TxFailedPersistent)]
    );
    // The sender is credited the deposit's mint, 2,966,081,819,000,000 wei: the
    // chain reads 18,387,682,604,898 wei before the block and
    // 2,984,469,501,604,898 wei after it.
    let rows = changes(&b).unwrap();
    assert_eq!(rows.len(), 1);
    let row = row_of(&rows, &sender);
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
        ("18387682604898", "2984469501604898", 25, 25, 1)
    );

    // A deposit whose call reverts keeps the mint and loses the transfer: the
    // chain reads 5,634,192,705,691,195 wei for the sender before the block and
    // 6,634,192,705,691,195 wei after it, and zero for the recipient.
    let b = captured_base_failed_deposit_reverted_call();
    assert_eq!(b.transaction_traces[0].calls[0].balance_changes.len(), 3);
    let rows = changes(&b).unwrap();
    assert_eq!(rows.len(), 1);
    let row = row_of(&rows, &hex::decode(REVERTED_DEPOSIT_SENDER).unwrap());
    assert_eq!(
        (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
        ("5634192705691195", "6634192705691195", 25, 25, 1)
    );
    let recipient = hex::decode(REVERTED_DEPOSIT_RECIPIENT).unwrap();
    assert!(!rows.iter().any(|r| r.address == recipient));
}

#[test]
fn the_failed_deposit_mint_shape_is_exact() {
    use eth::balance_change::Reason;
    fn refused(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("pinned persistence")
    }
    fn tx(b: &mut eth::Block) -> &mut eth::TransactionTrace {
        &mut b.transaction_traces[0]
    }
    fn mint(b: &mut eth::Block) -> &mut eth::BalanceChange {
        &mut b.transaction_traces[0].calls[0].balance_changes[0]
    }
    // Only a deposit.
    let mut b = captured_base_failed_deposit();
    tx(&mut b).r#type = eth::transaction_trace::Type::TrxTypeDynamicFee.into();
    assert!(refused(&b));
    // Only a credit of the transaction's sender.
    let mut b = captured_base_failed_deposit();
    tx(&mut b).from = hex::decode(REVERTED_DEPOSIT_SENDER).unwrap();
    assert!(refused(&b));
    let mut b = captured_base_failed_deposit();
    mint(&mut b).address = hex::decode(REVERTED_DEPOSIT_RECIPIENT).unwrap();
    assert!(refused(&b));
    // Only a positive credit.
    let mut b = captured_base_failed_deposit();
    let record = mint(&mut b);
    std::mem::swap(&mut record.old_value, &mut record.new_value);
    assert!(refused(&b));
    // Only before the execution (root call from ordinal 28)...
    let mut b = captured_base_failed_deposit();
    mint(&mut b).ordinal = 28;
    assert!(refused(&b));
    let mut b = captured_base_failed_deposit();
    tx(&mut b).calls[0].begin_ordinal = 25;
    assert!(refused(&b));
    // ...and after the transaction begins (ordinal 24).
    let mut b = captured_base_failed_deposit();
    tx(&mut b).begin_ordinal = 25;
    assert!(refused(&b));
    // Only in a root call at depth 0.
    let mut b = captured_base_failed_deposit();
    tx(&mut b).calls[0].depth = 1;
    assert!(refused(&b));
    // Only one mint in the root call, before the execution or during it.
    for ordinal in [26, 29] {
        let mut b = captured_base_failed_deposit();
        let second = eth::BalanceChange {
            ordinal,
            old_value: wei(2_984_469_501_604_898).into(),
            new_value: wei(2_984_469_501_604_899).into(),
            ..mint(&mut b).clone()
        };
        tx(&mut b).calls[0].balance_changes.push(second);
        assert!(refused(&b), "second mint at {ordinal}");
    }
    // Only with no other record before the execution, even a pinned one: a
    // deposit buys no gas.
    let mut b = captured_base_failed_deposit();
    let buy = eth::BalanceChange {
        reason: Reason::GasBuy.into(),
        ordinal: 26,
        old_value: wei(2_984_469_501_604_898).into(),
        new_value: wei(2_984_469_501_604_897).into(),
        ..mint(&mut b).clone()
    };
    tx(&mut b).calls[0].balance_changes.push(buy);
    assert!(refused(&b));
    let mut b = captured_base_failed_deposit_reverted_call();
    tx(&mut b).calls[0].balance_changes[1].ordinal = 26;
    assert!(refused(&b));
    // A mint during the execution, with another record before it, is not it.
    let mut b = captured_base_failed_deposit_reverted_call();
    tx(&mut b).calls[0].balance_changes[0].ordinal = 29;
    tx(&mut b).calls[0].balance_changes[1].ordinal = 26;
    assert!(refused(&b));
    // The reverted call's transfers are not part of the shape: they revert.
    let mut b = captured_base_failed_deposit_reverted_call();
    tx(&mut b).calls[0].balance_changes.truncate(1);
    assert_eq!(changes(&b).unwrap(), changes(&captured_base_failed_deposit_reverted_call()).unwrap());
    // A succeeded deposit's mint is ordinary persisted state.
    let mut b = captured_base_failed_deposit();
    tx(&mut b).status = eth::TransactionTraceStatus::Succeeded.into();
    tx(&mut b).calls[0].state_reverted = false;
    assert_eq!(changes(&b).unwrap(), changes(&captured_base_failed_deposit()).unwrap());
}

/// Optimism block 107,494,341 (`Block.ver` 4) as captured from Firehose:
/// transaction 1 (`0xfcefb0a0…`) whole, a deposit from `0x333601a8…` whose
/// call to `0x04ba6cf3…` reverts; its root call holds the mint and the
/// reverted value transfer.
const OPTIMISM_FAILED_DEPOSIT_REVERTED_CALL: &str = include_str!("../tests/fixtures/optimism-107494341-failed-deposit-mint-reverted-call.json");
/// Optimism block 106,973,772 (`Block.ver` 4) as captured from Firehose:
/// transaction 1 (`0x99ba9cf4…`) whole, a deposit from and to `0xbc339e62…`
/// that fails outside the EVM; its root call holds only the mint.
const OPTIMISM_FAILED_DEPOSIT: &str = include_str!("../tests/fixtures/optimism-106973772-failed-deposit-mint.json");
/// Optimism block 156,295,824 (`Block.ver` 5) as captured from Firehose:
/// transaction 1 (`0xcb154738…`) whole, a deposit from and to `0x95a7da8a…`
/// that fails outside the EVM ("insufficient balance for transfer"); its
/// root call holds only the mint.
const OPTIMISM_V5_FAILED_DEPOSIT: &str = include_str!("../tests/fixtures/optimism-156295824-failed-deposit-mint.json");
/// Unichain block 744,856 (`Block.ver` 4) as captured from Firehose:
/// transaction 1 (`0xeff18402…`) whole, a deposit from `0x9c5a151f…` whose
/// call to `0x0b929b80…` reverts; its root call holds the mint and the
/// reverted value transfer.
const UNICHAIN_FAILED_DEPOSIT_REVERTED_CALL: &str = include_str!("../tests/fixtures/unichain-744856-failed-deposit-mint-reverted-call.json");

#[test]
fn captured_optimism_and_unichain_failed_deposits_keep_their_mint() {
    use eth::balance_change::Reason;
    // Per fixture: `Block.ver`, the sender, the reverted call's recipient, the
    // sender's balance as the chain reads it before and after the block (the
    // deposit's `mint` apart), and the mint's ordinal. The chain reads each
    // recipient's balance unchanged: 76,098,960,000,000,000,000 wei for
    // `0x04ba6cf3…` and zero for `0x0b929b80…`.
    let cases = [
        (
            OPTIMISM_FAILED_DEPOSIT_REVERTED_CALL,
            4,
            "333601a803cac32b7d17a38d32c9728a93b422f4",
            Some("04ba6cf3c5aa6d4946f5b7f7adf111012a9fac65"),
            ("36783112587824055", "37473112587824055"),
            25,
        ),
        (OPTIMISM_FAILED_DEPOSIT, 4, "bc339e628e6fe32c39e84392d087567b2743ea35", None, ("0", "1"), 25),
        (
            OPTIMISM_V5_FAILED_DEPOSIT,
            5,
            "95a7da8a1d3026d1afb8c94f001532b34648e595",
            None,
            ("40000000012", "1000040000000012"),
            21,
        ),
        (
            UNICHAIN_FAILED_DEPOSIT_REVERTED_CALL,
            4,
            "9c5a151f8c390a554dc6822c5dc3724208c7c69d",
            Some("0b929b809c5485baa13aebb491e31af1f0aa4228"),
            ("0", "100000000000000"),
            31,
        ),
    ];
    for (fixture, ver, sender, recipient, (old, new), ordinal) in cases {
        let b = captured_block(&serde_json::from_str(fixture).unwrap());
        let tx = &b.transaction_traces[0];
        assert_eq!(
            (b.ver, tx.r#type.as_known(), tx.status.as_known()),
            (
                ver,
                Some(eth::transaction_trace::Type::TrxTypeOptimismDeposit),
                Some(eth::TransactionTraceStatus::Failed)
            ),
            "block {}",
            b.number
        );
        // The mint persists like a failed transaction's gas, and nothing else.
        let sender = hex::decode(sender).unwrap();
        let persisted: Vec<_> = records(&b).unwrap().into_iter().map(|r| (r.address, r.ordinal, r.reason, r.scope)).collect();
        assert_eq!(
            persisted,
            [(sender.clone(), ordinal, Reason::IncreaseMint as i32, persist::Scope::TxFailedPersistent)],
            "block {}",
            b.number
        );
        let rows = changes(&b).unwrap();
        assert_eq!(rows.len(), 1, "block {}", b.number);
        let row = row_of(&rows, &sender);
        assert_eq!(
            (row.old_amount.as_str(), row.amount.as_str(), row.first_ordinal, row.ordinal, row.records),
            (old, new, ordinal, ordinal, 1),
            "block {}",
            b.number
        );
        // The root call records the reverted transfer's credit of the
        // recipient, which has no row: the one row is the sender's.
        if let Some(recipient) = recipient {
            let recipient = hex::decode(recipient).unwrap();
            assert!(tx.calls[0]
                .balance_changes
                .iter()
                .any(|c| c.address == recipient && c.reason == Reason::Transfer));
        }
    }
}

/// Base block 50,024,530 (`Block.ver` 5) as captured from Firehose:
/// transaction 30 (`0xbae6554e…`) whole. It withdraws the SequencerFeeVault
/// `0x4200…0011` (the coinbase) and the BaseFeeVault `0x4200…0019` to zero
/// during its execution.
fn captured_fee_vault_credit() -> eth::Block {
    captured_block(&serde_json::from_str(BASE_FEE_VAULT_CREDIT).unwrap())
}
/// Base block 50,326,929 (`Block.ver` 5) as captured from Firehose:
/// transactions 237 (`0x39a5e05e…`) and 238 (`0x3c98767c…`) whole.
/// Transaction 237 withdraws the SequencerFeeVault, the BaseFeeVault and the
/// L1FeeVault `0x4200…001a` to zero during its execution; transaction 238
/// pays each its fee.
fn captured_fee_vault_credits() -> eth::Block {
    captured_block(&serde_json::from_str(BASE_FEE_VAULT_CREDITS).unwrap())
}
const SEQUENCER_FEE_VAULT: &str = "4200000000000000000000000000000000000011";
const BASE_FEE_VAULT: &str = "4200000000000000000000000000000000000019";
const L1_FEE_VAULT: &str = "420000000000000000000000000000000000001a";
/// The captured record at `ordinal`, in any call.
fn record_at(b: &mut eth::Block, ordinal: u64) -> &mut eth::BalanceChange {
    b.transaction_traces
        .iter_mut()
        .flat_map(|t| t.calls.iter_mut())
        .flat_map(|c| c.balance_changes.iter_mut())
        .find(|c| c.ordinal == ordinal)
        .unwrap()
}
fn row_values(rows: &[Change], address: &str) -> (String, String, u64, u64, u32) {
    let row = row_of(rows, &hex::decode(address).unwrap());
    (row.old_amount.clone(), row.amount.clone(), row.first_ordinal, row.ordinal, row.records)
}
fn values(rows: &[(&str, &str, u64, u64, u32)]) -> Vec<(String, String, u64, u64, u32)> {
    rows.iter()
        .map(|&(old, new, first, last, n)| (old.to_string(), new.to_string(), first, last, n))
        .collect()
}

#[test]
fn captured_base_fee_vault_credit_after_its_withdrawal_starts_from_zero() {
    let b = captured_fee_vault_credit();
    let vault = hex::decode(BASE_FEE_VAULT).unwrap();
    // The producer records the credit after the execution from the vault's
    // balance before the transaction, which the withdrawal sent away.
    let mut recorded: Vec<_> = records(&b)
        .unwrap()
        .iter()
        .filter(|r| r.address == vault)
        .map(|r| {
            (
                r.ordinal,
                amount(&value(r.old_value.as_deref()).unwrap()),
                amount(&value(r.new_value.as_deref()).unwrap()),
            )
        })
        .collect();
    recorded.sort();
    assert_eq!(
        recorded,
        [
            (850, "43588704197761546823".to_string(), "0".to_string()),
            (902, "43588704197761546823".to_string(), "43588705214861546823".to_string()),
        ]
    );
    // The vault keeps only the base fee, 203,420 gas at 5,000,000 wei; the
    // coinbase's credit started from zero and is unchanged, as is the L1
    // FeeVault's, which was not withdrawn.
    let rows = changes(&b).unwrap();
    assert_eq!(
        [BASE_FEE_VAULT, SEQUENCER_FEE_VAULT, L1_FEE_VAULT].map(|a| row_values(&rows, a)).to_vec(),
        values(&[
            ("43588704197761546823", "1017100000000", 850, 902, 2),
            ("112792299560060744846", "244104000000", 824, 901, 2),
            ("1740655652242675644", "1740655652612839391", 903, 903, 1),
        ])
    );
}

#[test]
fn captured_fee_vault_credits_chain_into_the_next_transaction() {
    let b = captured_fee_vault_credits();
    // Transaction 238's credits start from the fees that transaction 237
    // credited after withdrawing the vaults: 221,737 gas at the 5,000,000 wei
    // base fee, and the L1 fee the gas purchase holds beyond 230,958 gas at
    // 6,200,000 wei.
    let rows = changes(&b).unwrap();
    assert_eq!(
        [BASE_FEE_VAULT, L1_FEE_VAULT, SEQUENCER_FEE_VAULT].map(|a| row_values(&rows, a)).to_vec(),
        values(&[
            ("79578610751248996038", "1246980000000", 14265, 14344, 3),
            ("2292096308957288419", "1494163156", 14291, 14345, 3),
            ("428970258075516994778", "298929490159", 14239, 14343, 3),
        ])
    );
}

#[test]
fn the_fee_vault_credit_shape_is_exact() {
    fn fails(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("discontinuous")
    }
    // Transaction 237: the GAS_BUY at 14,217, the BaseFeeVault's withdrawal
    // at 14,265 and credit at 14,331, the L1FeeVault's credit at 14,332.
    const BEFORE: u128 = 79_578_610_751_248_996_038;
    const FEE: u128 = 1_108_685_000_000;
    // Only the base fee: one wei more is not rewritten.
    let mut b = captured_fee_vault_credits();
    record_at(&mut b, 14331).new_value = wei(BEFORE + FEE + 1).into();
    assert!(fails(&b));
    let mut b = captured_fee_vault_credits();
    if let Some(header) = b.header.as_option_mut() {
        header.base_fee_per_gas = wei(5_000_001).into();
    }
    assert!(fails(&b));
    // Only the L1 fee that the gas purchase holds.
    let mut b = captured_fee_vault_credits();
    record_at(&mut b, 14332).new_value = wei(2_292_096_308_957_288_419 + 747_081_578 + 1).into();
    assert!(fails(&b));
    let mut b = captured_fee_vault_credits();
    record_at(&mut b, 14217).old_value = wei(196_481_595_811_357_960).into();
    assert!(fails(&b));
    // Only one gas purchase.
    let mut b = captured_fee_vault_credits();
    let sender = record_at(&mut b, 14217).address.clone();
    let second = eth::BalanceChange {
        address: sender,
        reason: eth::balance_change::Reason::GasBuy.into(),
        ..change(0, 14218, Some(1), Some(1))
    };
    b.transaction_traces[0].calls[0].balance_changes.insert(1, second);
    assert!(fails(&b));
    // Only from the vault's balance before the transaction.
    let mut b = captured_fee_vault_credits();
    record_at(&mut b, 14331).old_value = wei(BEFORE - 1).into();
    record_at(&mut b, 14331).new_value = wei(BEFORE - 1 + FEE).into();
    assert!(fails(&b));
    // Only a REWARD_TRANSACTION_FEE credit.
    let mut b = captured_fee_vault_credits();
    record_at(&mut b, 14331).reason = eth::balance_change::Reason::Transfer.into();
    assert!(fails(&b));
    // Only the one record after the execution: with a second credit at
    // 14,333 (room made by moving transaction 237's end ordinal to 14,334)
    // that would chain after the rewrite, and the next record adjusted to
    // chain from it, the credit is not rewritten.
    let mut b = captured_fee_vault_credits();
    b.transaction_traces[0].end_ordinal = 14334;
    let second = eth::BalanceChange {
        address: hex::decode(BASE_FEE_VAULT).unwrap(),
        old_value: wei(FEE).into(),
        new_value: wei(FEE + 1).into(),
        ordinal: 14333,
        reason: eth::balance_change::Reason::RewardTransactionFee.into(),
    };
    b.transaction_traces[0].calls[0].balance_changes.push(second);
    record_at(&mut b, 14344).old_value = wei(FEE + 1).into();
    record_at(&mut b, 14344).new_value = wei(1_246_980_000_001).into();
    assert!(fails(&b));
    // Only after a TRANSFER to zero during the execution: a withdrawal that
    // leaves 1 wei is not rewritten, even when the next record would chain.
    let mut b = captured_fee_vault_credits();
    record_at(&mut b, 14265).new_value = wei(1).into();
    record_at(&mut b, 14344).old_value = wei(FEE + 1).into();
    record_at(&mut b, 14344).new_value = wei(1_246_980_000_001).into();
    assert!(fails(&b));
    let mut b = captured_fee_vault_credits();
    record_at(&mut b, 14265).reason = eth::balance_change::Reason::CallBalanceOverride.into();
    assert!(fails(&b));
    let mut b = captured_fee_vault_credits();
    b.transaction_traces[0].calls[0].begin_ordinal = 14266;
    assert!(fails(&b));
    // Only the BaseFeeVault and the L1FeeVault.
    let mut b = captured_fee_vault_credits();
    for ordinal in [14265, 14331, 14344] {
        record_at(&mut b, ordinal).address = hex::decode("420000000000000000000000000000000000001b").unwrap();
    }
    assert!(fails(&b));
    // A credit that already starts from zero, as the Optimism and Unichain
    // producers record it, chains and is not rewritten.
    let mut b = captured_fee_vault_credits();
    record_at(&mut b, 14331).old_value = None.into();
    record_at(&mut b, 14331).new_value = wei(FEE).into();
    let rows = changes(&b).unwrap();
    assert_eq!(
        row_values(&rows, BASE_FEE_VAULT),
        values(&[("79578610751248996038", "1246980000000", 14265, 14344, 3)])[0]
    );
    // The positive `b` and the succeeded transaction are defensive checks
    // that no mutation reaches: with `b` zero the credit already chains and
    // the rewrite would be the identity, and a failed transaction persists no
    // vault `TRANSFER` during the execution.
}

/// Polygon block 70,044,000 (`Block.ver` 3) as captured from Firehose:
/// transaction 46 (`0x60891cc1…`), the block's last, whole. It is the Bor
/// state-sync transaction: its root call holds four `commitState` calls (2,
/// 33, 64 and 69) from `0xffff…fffe` to `0x…1001`. In three of them
/// `0x884d1aa1…` unwraps WPOL `0x0d500b1d…` (calls 31, 62 and 98) and pays the
/// POL on (calls 32, 63 and 99).
fn captured_polygon_state_sync() -> eth::Block {
    captured_block(&serde_json::from_str(POLYGON_STATE_SYNC).unwrap())
}
const WPOL: &str = "0d500b1d8e8ef31e21c99d1db9a6444d3adf1270";
const STATE_SYNC_RELAY: &str = "884d1aa15f9957e1aeaa86a82a72e49bc2bfcbe3";
fn state_sync_call(b: &mut eth::Block, index: u32) -> &mut eth::Call {
    b.transaction_traces.last_mut().unwrap().calls.iter_mut().find(|c| c.index == index).unwrap()
}
/// `(account, old amount, amount, first ordinal, ordinal, records)` of every
/// row.
fn all_rows(b: &eth::Block) -> Vec<(String, String, String, u64, u64, u32)> {
    changes(b)
        .unwrap()
        .into_iter()
        .map(|r| (hex::encode(&r.address), r.old_amount, r.amount, r.first_ordinal, r.ordinal, r.records))
        .collect()
}
fn expected_rows(rows: &[(&str, &str, &str, u64, u64, u32)]) -> Vec<(String, String, String, u64, u64, u32)> {
    rows.iter()
        .map(|&(a, o, n, f, l, r)| (a.to_string(), o.to_string(), n.to_string(), f, l, r))
        .collect()
}

#[test]
fn captured_polygon_state_sync_records_chain_in_call_order() {
    let b = captured_polygon_state_sync();
    let wpol = hex::decode(WPOL).unwrap();
    // Each commitState call restarts the ordinals: WPOL's three records, in
    // call order, are at 3,409, 3,408 and 3,408, in the first, second and
    // fourth commitState call.
    let placed: Vec<_> = records(&b)
        .unwrap()
        .into_iter()
        .filter(|r| r.address == wpol)
        .map(|r| (r.ordinal, r.state_sync_call))
        .collect();
    assert_eq!(placed, [(3409, Some((3250, 0))), (3408, Some((3250, 1))), (3408, Some((3250, 3)))]);
    // In call order every account chains. eth_getBalance at block 70,044,000
    // reads each amount: the transaction is the block's last.
    assert_eq!(
        all_rows(&b),
        expected_rows(&[
            (WPOL, "188932933086870388198045211", "188932721355625264913504122", 3409, 3408, 3),
            (
                "18640bf13d3148597e1df6b8111486411d7efe67",
                "44717731008732551352",
                "198635573344347696641",
                3421,
                3421,
                1
            ),
            ("8645ad9bdf232d9235f8914603c8f120aa3f5f80", "0", "16189983039435773921", 3423, 3423, 1),
            (STATE_SYNC_RELAY, "7901000000000000000", "7901000000000000000", 3410, 3420, 6),
            ("886b317b6ab2a5a6df46c27f445de858a22f678b", "0", "41623419748233621879", 3422, 3422, 1),
        ])
    );
}

/// Polygon block 20,007,040 (`Block.ver` 3) as captured from Firehose: its
/// only transaction, the Bor state-sync transaction `0xa44f1b27…`, whole. Of
/// its ten `commitState` calls, two are POL deposits: the MRC20 contract
/// `0x…1010` pays `0x22df4f7e…` (call 37) and `0x7becdea5…` (call 41), each at
/// ordinal 13.
#[test]
fn captured_polygon_state_sync_deposits_debit_the_mrc20_contract_in_call_order() {
    let b = captured_block(&serde_json::from_str(POLYGON_STATE_SYNC_DEPOSITS).unwrap());
    // eth_getBalance at block 20,007,040 reads each amount.
    assert_eq!(
        all_rows(&b),
        expected_rows(&[
            (
                "0000000000000000000000000000000000001010",
                "9360194501496243214683032494",
                "9360193673399443124683032494",
                13,
                13,
                2
            ),
            (
                "22df4f7e056b43cd5f5d1bbccb03257196d4dd86",
                "60329268490000000000",
                "150609268490000000000",
                14,
                14,
                1
            ),
            (
                "7becdea52e793ac45c2040c2caaab38ace99c13a",
                "50000000000000000",
                "737866800090000000000",
                14,
                14,
                1
            ),
        ])
    );
}

#[test]
fn the_polygon_state_sync_shape_is_exact() {
    // By ordinal, WPOL's records in calls 62 and 98 tie at 3,408.
    fn by_ordinal(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("ambiguous")
    }
    // With the state-sync signature but outside the shape, the overlapping
    // subtrees fail the block.
    fn unrecognized(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("unrecognized Bor state-sync ordinal layout")
    }
    fn trx(b: &mut eth::Block) -> &mut eth::TransactionTrace {
        b.transaction_traces.last_mut().unwrap()
    }
    let fresh = captured_polygon_state_sync;
    // From and to the zero address (the signature), succeeded, with no gas
    // used and a positive begin ordinal.
    let mut b = fresh();
    trx(&mut b).from = vec![1; 20];
    assert!(by_ordinal(&b));
    let mut b = fresh();
    trx(&mut b).to = vec![1; 20];
    assert!(by_ordinal(&b));
    let mut b = fresh();
    trx(&mut b).status = eth::TransactionTraceStatus::Failed.into();
    assert!(unrecognized(&b));
    let mut b = fresh();
    trx(&mut b).gas_used = 21_000;
    assert!(unrecognized(&b));
    let mut b = fresh();
    trx(&mut b).begin_ordinal = 0;
    assert!(unrecognized(&b));
    // The block's last transaction, whose index no other transaction has,
    // after every other transaction's end ordinal.
    let other = |index: u32, begin: u64| eth::TransactionTrace {
        index,
        begin_ordinal: begin,
        end_ordinal: begin + 2,
        calls: vec![eth::Call {
            index: 1,
            ..Default::default()
        }],
        ..tx(eth::Call::default())
    };
    let mut b = fresh();
    b.transaction_traces.push(other(47, 3444));
    assert!(unrecognized(&b));
    let mut b = fresh();
    b.transaction_traces.insert(0, other(46, 3100));
    assert!(unrecognized(&b));
    let mut b = fresh();
    b.transaction_traces.insert(
        0,
        eth::TransactionTrace {
            end_ordinal: 3250,
            ..other(45, 3100)
        },
    );
    assert!(unrecognized(&b));
    // Only the state-sync transaction's records are placed by call: an earlier
    // transaction's WPOL record in a call with index 98 keeps its ordinal, and
    // WPOL chains from it into the state sync.
    let mut b = fresh();
    let mut earlier = other(45, 3100);
    earlier.calls[0].index = 98;
    earlier.calls[0].balance_changes.push(eth::BalanceChange {
        address: hex::decode(WPOL).unwrap(),
        old_value: big(1).into(),
        new_value: wei(188932933086870388198045211).into(),
        ordinal: 3101,
        reason: eth::balance_change::Reason::Transfer.into(),
    });
    b.transaction_traces.insert(0, earlier);
    let rows = all_rows(&b);
    assert_eq!(
        rows.iter().find(|r| r.0 == WPOL),
        expected_rows(&[(WPOL, "1", "188932721355625264913504122", 3101, 3408, 4)]).first()
    );
    // The root call: a CALL at depth 0 from and to the zero address with begin
    // ordinal 0 (the producer of `Block.ver` 4 records one), without value or
    // records, not reverted and not self-destructing.
    let mut b = fresh();
    state_sync_call(&mut b, 1).caller = vec![1; 20];
    assert!(by_ordinal(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 1).address = vec![1; 20];
    assert!(by_ordinal(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 1).begin_ordinal = 3251;
    assert!(by_ordinal(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 1).call_type = eth::CallType::DELEGATE.into();
    assert!(by_ordinal(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 1).depth = 1;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 1).value = big(1).into();
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 1).balance_changes.push(change(4, 3251, Some(0), Some(1)));
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 1).state_reverted = true;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 1).suicide = true;
    assert!(unrecognized(&b));
    // Every depth-1 call: a CALL of the StateReceiver by the system address,
    // with begin ordinal 0, of the root call. Without any depth-1 call by the
    // system address with begin ordinal 0 there is no signature.
    let mut b = fresh();
    state_sync_call(&mut b, 64).begin_ordinal = 3253;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 64).caller = vec![1; 20];
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 64).address = hex::decode("0000000000000000000000000000000000001010").unwrap();
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 64).call_type = eth::CallType::DELEGATE.into();
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 64).parent_index = 2;
    assert!(unrecognized(&b));
    let mut b = fresh();
    for index in [2, 33, 64, 69] {
        state_sync_call(&mut b, index).caller = vec![1; 20];
    }
    assert!(by_ordinal(&b));
    // Nor does a deeper call by the system address with begin ordinal 0 give
    // the signature.
    let call = state_sync_call(&mut b, 3);
    call.caller = BOR_SYSTEM_ADDRESS.to_vec();
    call.begin_ordinal = 0;
    assert!(by_ordinal(&b));
    let mut b = fresh();
    for index in [2, 33, 64, 69] {
        state_sync_call(&mut b, index).begin_ordinal = 3251;
    }
    assert!(by_ordinal(&b));
    // Every deeper call one level below its parent, in increasing call index,
    // in the subtree of the latest depth-1 call (call 70 is the first call of
    // call 69's), after the call before it in its subtree (call 31 begins at
    // 3,408) and holding its records between its begin and end ordinals (call
    // 31's are at 3,409 and 3,410).
    let mut b = fresh();
    state_sync_call(&mut b, 98).depth = 8;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 98).parent_index = 200;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 99).index = 98;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 70).parent_index = 64;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 32).begin_ordinal = 3408;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 31).begin_ordinal = 3410;
    assert!(unrecognized(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 31).end_ordinal = 3409;
    assert!(unrecognized(&b));
    // No call self-destructs or moves value to itself.
    let mut b = fresh();
    state_sync_call(&mut b, 68).suicide = true;
    assert!(unrecognized(&b));
    let mut b = fresh();
    let call = state_sync_call(&mut b, 67);
    call.address = call.caller.clone();
    call.value = big(1).into();
    assert!(unrecognized(&b));
    // Every record of the transaction lies strictly within its ordinals (3,250
    // to 3,443), and every other record of the block outside them.
    let mut b = fresh();
    state_sync_call(&mut b, 32).balance_changes[1].ordinal = 3443;
    assert!(unrecognized(&b));
    for ordinal in [3250, 3443] {
        let mut b = fresh();
        state_sync_call(&mut b, 64).balance_changes.push(change(4, ordinal, Some(0), Some(1)));
        assert!(unrecognized(&b));
    }
    let mut b = fresh();
    b.balance_changes.push(change(4, 3300, Some(0), Some(1)));
    assert!(unrecognized(&b));
    let mut b = fresh();
    b.system_calls.push(eth::Call {
        index: 1,
        balance_changes: vec![change(4, 3300, Some(0), Some(1))],
        ..Default::default()
    });
    assert!(unrecognized(&b));
    let mut b = fresh();
    let mut earlier = other(45, 3100);
    earlier.calls[0].balance_changes.push(change(4, 3250, Some(0), Some(1)));
    b.transaction_traces.insert(0, earlier);
    assert!(unrecognized(&b));
    // Within one subtree a tie still fails the block, and the records must
    // still chain: `0x884d1aa1…` at 3,410 in calls 31 and 32.
    let mut b = fresh();
    state_sync_call(&mut b, 32).begin_ordinal = 3409;
    state_sync_call(&mut b, 32).balance_changes[0].ordinal = 3410;
    assert!(by_ordinal(&b));
    let mut b = fresh();
    state_sync_call(&mut b, 62).balance_changes[0].old_value = big(1).into();
    assert!(changes(&b).unwrap_err().to_string().contains("discontinuous"));
}

#[test]
fn a_state_sync_outside_the_shape_fails_unless_its_subtrees_follow_call_order() {
    fn unrecognized(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("unrecognized Bor state-sync ordinal layout")
    }
    // Outside the shape (gas used), with records only in some calls.
    let outside = |cleared: &[u32]| {
        let mut b = captured_polygon_state_sync();
        b.transaction_traces.last_mut().unwrap().gas_used = 21_000;
        for &index in cleared {
            state_sync_call(&mut b, index).balance_changes.clear();
        }
        b
    };
    // With records only in the subtree of call 2, whose ordinals follow its
    // execution, the ordinal order stands.
    let rows = all_rows(&outside(&[62, 63, 98, 99]));
    assert_eq!(
        rows.iter().map(|r| (r.0.as_str(), r.5)).collect::<Vec<_>>(),
        [(WPOL, 1), ("8645ad9bdf232d9235f8914603c8f120aa3f5f80", 1), (STATE_SYNC_RELAY, 2)]
    );
    // Subtrees whose records share an ordinal: call 32's at 3,422 and 3,423,
    // and one at 3,423 in call 63.
    let mut b = outside(&[31, 62, 63, 98, 99]);
    state_sync_call(&mut b, 63).balance_changes.push(change(4, 3423, Some(0), Some(1)));
    assert!(unrecognized(&b));
    // Subtrees whose records run backwards, across subtrees without records:
    // call 98's at 3,408 and 3,409 after call 32's.
    assert!(unrecognized(&outside(&[31, 62, 63, 99])));
}

/// Polygon block 20,025,792 (`Block.ver` 3) as captured from Firehose: the
/// state-sync transaction `0xa0991514…` (transaction 119) and the span commit
/// `0xb39c139e…` (transaction 120), the block's last, whole, with the span
/// commit's call input: `commitSpan` of span 3,130, blocks 20,025,856 to
/// 20,032,255. Two of the eight `commitState` calls (2 and 6) are POL
/// deposits: the MRC20 contract `0x…1010` pays `0x1ad528be…` (call 5) and
/// `0xb102e324…` (call 9), each at ordinal 11,969.
const POLYGON_STATE_SYNC_SPAN_COMMIT: &str = include_str!("../tests/fixtures/polygon-20025792-state-sync-span-commit.json");
fn captured_polygon_state_sync_span_commit() -> eth::Block {
    let f: Value = serde_json::from_str(POLYGON_STATE_SYNC_SPAN_COMMIT).unwrap();
    let mut b = captured_block(&f);
    // `captured_block` reads no call input.
    b.transaction_traces[1].calls[0].input = hex::decode(f["transactions"][1]["calls"][0]["input"].as_str().unwrap()).unwrap();
    b
}
fn span_commit_rows() -> Vec<(String, String, String, u64, u64, u32)> {
    // eth_getBalance at block 20,025,792 reads each amount, and at block
    // 20,025,791 each old amount.
    expected_rows(&[
        (
            "0000000000000000000000000000000000001010",
            "9360100171229661770329494204",
            "9360100141477570300329494204",
            11969,
            11969,
            2,
        ),
        ("1ad528be51f6bcee12d621981d4b3c4aff753482", "0", "10000000000000000000", 11970, 11970, 1),
        ("b102e324e429ed7bbeb746b4e9e1589e66c5975b", "0", "19752091470000000000", 11970, 11970, 1),
    ])
}

#[test]
fn captured_polygon_state_sync_before_a_span_commit_chains_in_call_order() {
    let b = captured_polygon_state_sync_span_commit();
    assert_eq!(all_rows(&b), span_commit_rows());
    // Without the span commit the state sync is the block's last transaction.
    let mut b = captured_polygon_state_sync_span_commit();
    b.transaction_traces.pop();
    assert_eq!(all_rows(&b), span_commit_rows());
}

#[test]
fn the_polygon_span_commit_shape_is_exact() {
    // The MRC20 contract's records in calls 5 and 9 tie at 11,969, so with the
    // span commit outside its shape the state sync fails the block.
    fn unrecognized(b: &eth::Block) -> bool {
        changes(b).unwrap_err().to_string().contains("unrecognized Bor state-sync ordinal layout")
    }
    fn span(b: &mut eth::Block) -> &mut eth::TransactionTrace {
        b.transaction_traces.last_mut().unwrap()
    }
    fn call(b: &mut eth::Block) -> &mut eth::Call {
        &mut span(b).calls[0]
    }
    let fresh = captured_polygon_state_sync_span_commit;
    // Succeeded, from the system address to the ValidatorSet, the block's last
    // transaction, after the state sync's end ordinal and with another index.
    let mut b = fresh();
    span(&mut b).status = eth::TransactionTraceStatus::Failed.into();
    assert!(unrecognized(&b));
    let mut b = fresh();
    span(&mut b).from = vec![1; 20];
    assert!(unrecognized(&b));
    let mut b = fresh();
    span(&mut b).to = vec![1; 20];
    assert!(unrecognized(&b));
    let mut b = fresh();
    b.transaction_traces.push(eth::TransactionTrace {
        index: 121,
        begin_ordinal: 12634,
        end_ordinal: 12636,
        ..tx(eth::Call {
            index: 1,
            ..Default::default()
        })
    });
    assert!(unrecognized(&b));
    let mut b = fresh();
    span(&mut b).begin_ordinal = 12153;
    assert!(unrecognized(&b));
    let mut b = fresh();
    span(&mut b).index = 119;
    assert!(unrecognized(&b));
    // One call: a CALL at depth 0 from the system address to the ValidatorSet
    // with begin ordinal 0 and the selector of `commitSpan`, not reverted and
    // without balance records.
    let mut b = fresh();
    let second = eth::Call {
        index: 2,
        parent_index: 1,
        depth: 1,
        ..call(&mut b).clone()
    };
    span(&mut b).calls.push(second);
    assert!(unrecognized(&b));
    let mut b = fresh();
    call(&mut b).depth = 1;
    assert!(unrecognized(&b));
    let mut b = fresh();
    call(&mut b).call_type = eth::CallType::DELEGATE.into();
    assert!(unrecognized(&b));
    let mut b = fresh();
    call(&mut b).caller = vec![1; 20];
    assert!(unrecognized(&b));
    let mut b = fresh();
    call(&mut b).address = vec![1; 20];
    assert!(unrecognized(&b));
    let mut b = fresh();
    call(&mut b).begin_ordinal = 12155;
    assert!(unrecognized(&b));
    let mut b = fresh();
    call(&mut b).input[3] ^= 1;
    assert!(unrecognized(&b));
    let mut b = fresh();
    call(&mut b).state_reverted = true;
    assert!(unrecognized(&b));
    let mut b = fresh();
    call(&mut b).balance_changes.push(change(4, 12200, Some(0), Some(1)));
    assert!(unrecognized(&b));
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
