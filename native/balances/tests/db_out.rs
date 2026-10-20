//! `map_sink_entries` and `db_out` through the crate's public API: the
//! entries, the upstream row format, the fail-closed entry and row guards,
//! captured block 122260950 through both modules, the manifest wiring and the
//! embedded schema.
use buffa::Message;
use buffa_types::google::protobuf::{Any, Timestamp};
use native_balances::{db_out::database_changes, sink_entries};
use proto::pb::evm::balances::v1 as pb;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use substreams::{
    pb::{sf::substreams::foundational_store::model::v2::SinkEntries, substreams::Clock},
    Hex,
};
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

fn balance(address: u8, amount: &str) -> pb::NativeBalance {
    pb::NativeBalance {
        address: vec![address; 20],
        amount: amount.to_string(),
    }
}

fn entries(balances: Vec<pb::NativeBalance>) -> SinkEntries {
    sink_entries::entries(&pb::NativeEvents { balances })
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

/// One entry per balance, in order: the key is the address and the value the
/// `NativeBalance` itself, overwriting what the store holds.
#[test]
fn entries_key_each_balance_by_its_address() {
    let balances = vec![balance(1, "0"), balance(2, MAX_UINT256)];
    let out = entries(balances.clone());
    assert!(!out.if_not_exist);
    assert_eq!(out.entries.len(), 2);
    for (entry, balance) in out.entries.iter().zip(&balances) {
        assert_eq!(entry.key.as_option().unwrap().bytes, balance.address);
        let value = entry.value.as_option().unwrap();
        assert_eq!(value.type_url(), "type.googleapis.com/evm.balances.v1.NativeBalance");
        assert_eq!(&value.unpack_unchecked::<pb::NativeBalance>().unwrap(), balance);
    }
    assert_eq!(sink_entries::balances(&out).unwrap(), balances);
    // A block without balances encodes to no bytes, which the handler skips.
    assert!(entries(vec![]).encode_to_vec().is_empty());
}

/// `db_out` fails the block on an entry `map_sink_entries` would not write.
#[test]
fn entries_the_module_would_not_write_fail_the_block() {
    let good = || entries(vec![balance(1, "1")]);
    let mut overwrite_off = good();
    overwrite_off.if_not_exist = true;
    let mut no_key = good();
    no_key.entries[0].key = buffa::MessageField::none();
    let mut no_value = good();
    no_value.entries[0].value = buffa::MessageField::none();
    let mut other_type = good();
    other_type.entries[0].value = buffa::MessageField::some(Any::pack(&balance(1, "1"), "type.googleapis.com/evm.balances.v1.Balance"));
    let mut other_key = good();
    other_key.entries[0].key.as_option_mut().unwrap().bytes = vec![2; 20];
    let mut garbage = good();
    garbage.entries[0].value.as_option_mut().unwrap().value = vec![0xff].into();
    for bad in [overwrite_off, no_key, no_value, other_type, other_key, garbage] {
        assert!(sink_entries::balances(&bad).is_err());
        assert!(database_changes(&clock(), &bad).is_err());
    }
}

#[test]
fn rows_follow_the_upstream_format() {
    let changes = database_changes(&clock(), &entries(vec![balance(1, "0"), balance(2, MAX_UINT256)]))
        .unwrap()
        .table_changes;

    let tables: Vec<&str> = changes.iter().map(|c| c.table.as_str()).collect();
    assert_eq!(tables, ["native_balances", "native_balances", "blocks"]);
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

    let native_row = &changes[0];
    assert!(matches!(&native_row.primary_key, Some(PrimaryKey::Pk(a)) if *a == hex20(1)));
    let cols = columns(native_row);
    assert_eq!(
        cols.keys().copied().collect::<Vec<_>>(),
        ["address", "balance", "block_hash", "block_num", "timestamp"]
    );
    assert_eq!((cols["address"], cols["balance"]), (hex20(1).as_str(), "0"));
    assert_eq!(columns(&changes[1])["balance"], MAX_UINT256);

    let blocks_row = &changes[2];
    assert!(matches!(&blocks_row.primary_key, Some(PrimaryKey::CompositePk(c)) if c.keys.len() == 1 && c.keys["block_num"] == "100"));
    assert_eq!(columns(blocks_row).len(), 3);
}

#[test]
fn a_block_without_balances_writes_nothing() {
    let changes = database_changes(&clock(), &entries(vec![])).unwrap();
    assert!(changes.table_changes.is_empty());
}

#[test]
fn rows_the_sink_would_store_wrongly_fail_the_block() {
    let too_large = "115792089237316195423570985008687907853269984665640564039457584007913129639936";
    let mut short = balance(1, "1");
    short.address.pop();
    for bad in [
        balance(1, ""),
        balance(1, "0x10"),
        balance(1, "-1"),
        balance(1, "01"),
        balance(1, too_large),
        short,
    ] {
        assert!(database_changes(&clock(), &entries(vec![bad])).is_err());
    }
    let mut no_time = clock();
    no_time.timestamp = buffa::MessageField::none();
    assert!(database_changes(&no_time, &entries(vec![])).is_err());
}

/// Captured BSC block 122260950 through `map_sink_entries`, as the manifest
/// wires it, and `db_out`: one row per balance, carrying the map's values
/// verbatim.
#[test]
fn captured_block_writes_one_row_per_map_balance() {
    use substreams_ethereum::pb::eth::v2 as eth;

    let block = eth::Block::decode_from_slice(include_bytes!("../../../erc20/balances/tests/fixtures/bsc-122260950.pb")).unwrap();
    let clock = Clock {
        id: Hex::encode(&block.hash),
        number: block.number,
        timestamp: block.header.as_option().unwrap().timestamp.clone(),
    };
    let native = native_balances::project(&block).unwrap();
    // The bytes `map_sink_entries` outputs, decoded as `db_out` receives them.
    let output = SinkEntries::decode_from_slice(&sink_entries::entries(&native).encode_to_vec()).unwrap();
    assert_eq!(sink_entries::balances(&output).unwrap(), native.balances);
    let changes = database_changes(&clock, &output).unwrap().table_changes;

    let block_columns = [
        ("block_num", "122260950".to_string()),
        ("block_hash", format!("0x{}", clock.id)),
        ("timestamp", clock.timestamp.as_option().unwrap().seconds.to_string()),
    ];
    let row = |extra: Vec<(&'static str, String)>| -> BTreeMap<&'static str, String> { block_columns.iter().cloned().chain(extra).collect() };
    let hex = |bytes: &[u8]| format!("0x{}", Hex::encode(bytes));
    let mut expected: Vec<(&str, BTreeMap<&str, String>)> = native
        .balances
        .iter()
        .map(|b| ("native_balances", row(vec![("address", hex(&b.address)), ("balance", b.amount.clone())])))
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
    assert_eq!((native.balances.len(), changes.len()), (82, 82 + 1));
    assert_eq!(actual, expected);
}

/// The handler takes `(Clock, SinkEntries)`: the manifest must feed the inputs
/// in that order, with no params, and embed the generated schema.
#[test]
fn manifest_wires_db_out_and_the_schema() {
    assert!(MANIFEST.contains(
        "  - name: map_sink_entries\n    kind: map\n    inputs:\n      - source: sf.ethereum.type.v2.Block\n    output:\n      type: proto:sf.substreams.foundational_store.model.v2.SinkEntries\n"
    ));
    assert!(MANIFEST.contains(
        "  - name: db_out\n    kind: map\n    inputs:\n      - source: sf.substreams.v1.Clock\n      - map: map_sink_entries\n    output:\n      type: proto:sf.substreams.sink.database.v1.DatabaseChanges\n"
    ));
    assert!(!MANIFEST.contains("map_events"));
    assert!(MANIFEST.contains("sink:\n  module: db_out\n  type: sf.substreams.sink.sql.v1.Service\n  config:\n    schema: \"./clickhouse/schema.sql\""));
    assert!(!MANIFEST.contains("- params:") && !MANIFEST.contains("\nparams:") && !MANIFEST.contains("\nnetworks:"));
    assert!(MANIFEST.contains("\nnetwork: bsc\n"));
}

/// The SQL is upstream's: two files byte for byte, the native half of
/// `schema.1.table.balances.sql` (its lines 29-51), which follows the ERC-20
/// half in erc20/balances after one blank line, and the historical view with
/// only the 1h, 4h, 1d and 1w intervals the Token API serves (owner,
/// 2026-10-14; upstream also writes 1m, 5m, 10m and 30m). `make schema`
/// concatenates them into the embedded schema.
#[test]
fn schema_is_the_native_half_of_upstreams_v0_3_4_schema() {
    const FILES: [(&str, &str); 4] = [
        ("schema.0.blocks.sql", "b2ff361c74cd0f8a501c16c70e47375ce7ffa26e9d3927f429c89645ebbec2c5"),
        (
            "schema.1.table.native-balances.sql",
            "b87bf25beb18fe85ec95e75017bdb3e4b4221ea6cdbc5d11427fc28dbc379ed6",
        ),
        (
            "schema.2.mv.historical-native-balances.sql",
            "4208ff4de25d6b4860aa43acb9977e714348b5ab53327d6ab86e6e95ca7bb8f7",
        ),
        (
            "schema.3.view.historical-native-balances.sql",
            "f6aa09d25133dbc8e7b46bc743fc95cf950a4bfd868f10f4a616a56d4647a017",
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
    assert_eq!(hex(&schema), "c12a8613befe757a4fb7678d0e466d10384825ebd58be783b401f63187562e57");
}
