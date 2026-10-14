//! `db_out` through the crate's public API: the upstream row format, the
//! fail-closed row guards, captured block 122260950 through `map_events`, the
//! manifest wiring and the embedded schema.
use buffa_types::google::protobuf::Timestamp;
use erc20_balances::db_out::database_changes;
use proto::pb::evm::balances::v1 as pb;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use substreams::{pb::substreams::Clock, Hex};
use substreams_database_change::pb::sf::substreams::sink::database::v1::{
    field::UpdateOp,
    table_change::{Operation, PrimaryKey},
    TableChange,
};

const MANIFEST: &str = include_str!("../substreams.yaml");
/// `2^256 - 1`, the largest value of the `UInt256` balance columns.
const MAX_UINT256: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";

fn clock() -> Clock {
    Clock {
        id: "ab".repeat(32),
        number: 100,
        timestamp: buffa::MessageField::some(Timestamp {
            seconds: 1_700_000_000,
            nanos: 0,
            ..Default::default()
        }),
    }
}

fn balance(contract: Option<u8>, address: u8, amount: &str) -> pb::Balance {
    pb::Balance {
        contract: contract.map(|c| vec![c; 20]),
        address: vec![address; 20],
        amount: amount.to_string(),
    }
}

fn events(balances: Vec<pb::Balance>) -> pb::Events {
    pb::Events { balances }
}

fn hex20(byte: u8) -> String {
    format!("0x{}", format!("{byte:02x}").repeat(20))
}

/// A change as the sink reads it: fields by column name, never by position.
/// Every field is a plain SET (`update_op` 1, the only addition of the 5.0.0
/// encoding over upstream's 3.0.0).
fn columns(change: &TableChange) -> BTreeMap<&str, &str> {
    change
        .fields
        .iter()
        .map(|f| {
            assert_eq!(f.update_op, UpdateOp::UPDATE_OP_SET);
            (f.name.as_str(), f.value.as_str())
        })
        .collect()
}

#[test]
fn rows_follow_the_upstream_format() {
    let changes = database_changes(&clock(), &events(vec![balance(Some(9), 1, "1000"), balance(Some(8), 1, MAX_UINT256)]))
        .unwrap()
        .table_changes;

    let tables: Vec<&str> = changes.iter().map(|c| c.table.as_str()).collect();
    assert_eq!(tables, ["erc20_balances", "erc20_balances", "blocks"]);
    let block = [
        ("block_hash", "0xabababababababababababababababababababababababababababababababab"),
        ("block_num", "100"),
        ("timestamp", "1700000000"),
    ];
    for (i, change) in changes.iter().enumerate() {
        assert_eq!(change.ordinal, i as u64);
        assert_eq!(change.operation, Operation::OPERATION_CREATE);
        // Sorted by name, so the bytes do not depend on `HashMap` order.
        assert!(change.fields.iter().map(|f| &f.name).is_sorted());
        let cols = columns(change);
        for (name, value) in block {
            assert_eq!(cols[name], value);
        }
    }

    let erc20_row = &changes[0];
    let mut pk = BTreeMap::new();
    pk.insert("address".to_string(), hex20(1));
    pk.insert("contract".to_string(), hex20(9));
    assert!(matches!(&erc20_row.primary_key, Some(PrimaryKey::CompositePk(c)) if c.keys.clone().into_iter().collect::<BTreeMap<_, _>>() == pk));
    let cols = columns(erc20_row);
    assert_eq!(
        cols.keys().copied().collect::<Vec<_>>(),
        ["address", "balance", "block_hash", "block_num", "contract", "timestamp"]
    );
    assert_eq!(
        (cols["contract"], cols["address"], cols["balance"]),
        (hex20(9).as_str(), hex20(1).as_str(), "1000")
    );
    assert_eq!(columns(&changes[1])["balance"], MAX_UINT256);

    let blocks_row = &changes[2];
    assert!(matches!(&blocks_row.primary_key, Some(PrimaryKey::CompositePk(c)) if c.keys.len() == 1 && c.keys["block_num"] == "100"));
    assert_eq!(columns(blocks_row).len(), 3);
}

#[test]
fn a_block_without_balances_writes_nothing() {
    let changes = database_changes(&clock(), &events(vec![])).unwrap();
    assert!(changes.table_changes.is_empty());
}

#[test]
fn rows_the_sink_would_store_wrongly_fail_the_block() {
    let too_large = "115792089237316195423570985008687907853269984665640564039457584007913129639936";
    let mut short = balance(Some(9), 1, "1");
    short.address.pop();
    let mut short_contract = balance(Some(9), 1, "1");
    short_contract.contract.as_mut().unwrap().pop();
    for bad in [
        balance(None, 1, "1"),
        balance(Some(9), 1, ""),
        balance(Some(9), 1, "0x10"),
        balance(Some(9), 1, "-1"),
        balance(Some(9), 1, "01"),
        balance(Some(9), 1, too_large),
        short,
        short_contract,
    ] {
        assert!(database_changes(&clock(), &events(vec![bad])).is_err());
    }
    let mut no_time = clock();
    no_time.timestamp = buffa::MessageField::none();
    assert!(database_changes(&no_time, &events(vec![])).is_err());
}

/// Captured BSC block 122260950 through `map_events`, as the manifest wires
/// it, and `db_out`: one row per balance, carrying the map's values verbatim.
#[test]
fn captured_block_writes_one_row_per_map_balance() {
    use buffa::Message;
    use substreams_ethereum::pb::eth::v2 as eth;

    let block = eth::Block::decode_from_slice(include_bytes!("../../../erc20/balances/tests/fixtures/bsc-122260950.pb")).unwrap();
    let clock = Clock {
        id: Hex::encode(&block.hash),
        number: block.number,
        timestamp: block.header.as_option().unwrap().timestamp.clone(),
    };
    let erc20 = erc20_balances::run(&block).unwrap();
    let changes = database_changes(&clock, &erc20).unwrap().table_changes;

    let block_columns = [
        ("block_num", "122260950".to_string()),
        ("block_hash", format!("0x{}", clock.id)),
        ("timestamp", clock.timestamp.as_option().unwrap().seconds.to_string()),
    ];
    let row = |extra: Vec<(&'static str, String)>| -> BTreeMap<&'static str, String> { block_columns.iter().cloned().chain(extra).collect() };
    let hex = |bytes: &[u8]| format!("0x{}", Hex::encode(bytes));
    let mut expected: Vec<(&str, BTreeMap<&str, String>)> = erc20
        .balances
        .iter()
        .map(|b| {
            let contract = hex(b.contract.as_deref().unwrap());
            (
                "erc20_balances",
                row(vec![("contract", contract), ("address", hex(&b.address)), ("balance", b.amount.clone())]),
            )
        })
        .collect();
    expected.push(("blocks", row(vec![])));

    let actual: Vec<(&str, BTreeMap<&str, String>)> = changes
        .iter()
        .map(|c| {
            assert_eq!(c.operation, Operation::OPERATION_CREATE);
            let fields = columns(c).into_iter().map(|(k, v)| (k, v.to_string())).collect();
            (c.table.as_str(), fields)
        })
        .collect();
    assert_eq!((erc20.balances.len(), changes.len()), (164, 164 + 1));
    assert_eq!(actual, expected);
}

/// The handler takes `(Clock, Events)`: the manifest must feed the inputs in
/// that order, with no params, and embed the generated schema.
#[test]
fn manifest_wires_db_out_and_the_schema() {
    assert!(MANIFEST.contains(
        "  - name: map_events\n    kind: map\n    inputs:\n      - source: sf.ethereum.type.v2.Block\n    output:\n      type: proto:evm.balances.v1.Events\n"
    ));
    assert!(MANIFEST.contains(
        "  - name: db_out\n    kind: map\n    inputs:\n      - source: sf.substreams.v1.Clock\n      - map: map_events\n    output:\n      type: proto:sf.substreams.sink.database.v1.DatabaseChanges\n"
    ));
    assert!(MANIFEST.contains("sink:\n  module: db_out\n  type: sf.substreams.sink.sql.v1.Service\n  config:\n    schema: \"./clickhouse/schema.sql\""));
    assert!(!MANIFEST.contains("- params:") && !MANIFEST.contains("\nparams:") && !MANIFEST.contains("\nnetworks:"));
    assert!(MANIFEST.contains("\nnetwork: bsc\n"));
}

/// The SQL is upstream's: two files byte for byte, the ERC-20 half of
/// `schema.1.table.balances.sql` (its lines 1-27), which the native half in
/// native/balances follows after one blank line, and the historical view with
/// only the 1h, 4h, 1d and 1w intervals the Token API serves (owner,
/// 2026-10-14; upstream also writes 1m, 5m, 10m and 30m). `make schema`
/// concatenates them into the embedded schema.
#[test]
fn schema_is_the_erc20_half_of_upstreams_v0_3_4_schema() {
    const FILES: [(&str, &str); 4] = [
        ("schema.0.blocks.sql", "b2ff361c74cd0f8a501c16c70e47375ce7ffa26e9d3927f429c89645ebbec2c5"),
        (
            "schema.1.table.erc20-balances.sql",
            "40ca4171d2b992e19d7d159c2c7b85a295e5cc3288f91f860da31365e5d5659b",
        ),
        (
            "schema.2.mv.historical-erc20-balances.sql",
            "c428e9035c47ee6da8bc7114c9ad1a5532a20e89ce90dbbcc873355f6310bb84",
        ),
        (
            "schema.3.view.historical-erc20-balances.sql",
            "dcdcaa21b3095d63835610ed2a04f46eea7014e2f9bc74cfab96f12528f00396",
        ),
    ];
    let hex = |bytes: &[u8]| Hex::encode(Sha256::digest(bytes));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("clickhouse");

    let mut parts: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("schema.") && n.ends_with(".sql") && n != "schema.sql")
        .collect();
    parts.sort();
    assert_eq!(parts, FILES.map(|(name, _)| name));

    let mut schema = b"-- This file is generated. Do not edit.\n\n".to_vec();
    for (name, sha256) in FILES {
        let sql = std::fs::read(dir.join(name)).unwrap();
        assert_eq!(hex(&sql), sha256, "{name}");
        schema.extend_from_slice(&sql);
        schema.push(b'\n');
    }
    assert_eq!(hex(&schema), "5ede93d93f52201dd65dc538765b61b69ea278b14b346f311eb26cffdfc80ed6");
}
