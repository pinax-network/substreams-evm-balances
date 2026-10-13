//! `db_out`: the ERC-20 balances of `map_events` as rows of the substreams-evm
//! ClickHouse tables `erc20_balances` and `blocks`, in the row format of the
//! upstream `db_out` at
//! [substreams-evm@cb8607f](https://github.com/pinax-network/substreams-evm/tree/cb8607f59a37aa9daf85547162b8871d12568282/db-evm-balances/src),
//! so a database started from this package can take the RPC-era history by
//! partition copy.
//!
//! Every row is a CREATE and carries `block_num`, `block_hash` (`0x` + clock
//! id) and `timestamp` (clock seconds). Addresses are `0x` + lowercase hex and
//! `balance` is `Balance.amount` verbatim. One `blocks` row is written only
//! when the block wrote a balance row. Unlike upstream, a row the SQL sink
//! would store wrongly fails the block: a balance without a contract, an
//! address that is not 20 bytes or an amount that is not a decimal `uint256`.
//! A missing clock timestamp is an error, not a panic.
//!
//! The changes are encoded by `substreams-database-change` 5.0.0, where every
//! `Field` also carries `update_op = UPDATE_OP_SET` (field 4). Upstream's 3.0.0
//! has no such field; the SQL sink uses only a CREATE field's name and value,
//! so the stored rows are the same. `native/balances` keeps its own copy of the
//! row helpers below, so neither package's hashes depend on the other.
use proto::pb::evm::balances::v1 as pb;
use substreams::{errors::Error, pb::substreams::Clock, Hex};
use substreams_database_change::{
    pb::sf::substreams::sink::database::v1::DatabaseChanges,
    tables::{Row, Tables},
};

/// `2^256 - 1`, the largest value of the `UInt256` balance columns.
const MAX_UINT256: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";

/// The block columns every row carries.
struct BlockColumns {
    block_num: String,
    block_hash: String,
    timestamp: String,
}

impl BlockColumns {
    fn new(clock: &Clock) -> Result<Self, Error> {
        let timestamp = clock.timestamp.as_option().ok_or_else(|| Error::msg("missing clock timestamp"))?;
        Ok(Self {
            block_num: clock.number.to_string(),
            block_hash: format!("0x{}", clock.id),
            timestamp: timestamp.seconds.to_string(),
        })
    }

    fn set(&self, row: &mut Row) {
        row.set("block_num", &self.block_num)
            .set("block_hash", &self.block_hash)
            .set("timestamp", &self.timestamp);
    }
}

/// `0x` + lowercase hex of a 20-byte address.
fn address(bytes: &[u8], what: &str) -> Result<String, Error> {
    if bytes.len() != 20 {
        return Err(Error::msg(format!("{what} is not a 20-byte address")));
    }
    Ok(format!("0x{}", Hex::encode(bytes)))
}

/// `amount` verbatim when it is a canonical decimal `uint256`. The SQL sink
/// parses `UInt256` without checking: anything else would be stored as a wrong
/// number or abort the sink.
fn amount(amount: &str) -> Result<&str, Error> {
    let decimal = !amount.is_empty() && amount.bytes().all(|b| b.is_ascii_digit()) && (amount == "0" || !amount.starts_with('0'));
    let in_range = amount.len() < MAX_UINT256.len() || (amount.len() == MAX_UINT256.len() && amount <= MAX_UINT256);
    if decimal && in_range {
        Ok(amount)
    } else {
        Err(Error::msg(format!("balance {amount:?} is not a decimal uint256")))
    }
}

/// The `db_out` body: ERC-20 rows, then the `blocks` row.
pub fn database_changes(clock: &Clock, events: &pb::Events) -> Result<DatabaseChanges, Error> {
    let block = BlockColumns::new(clock)?;
    let mut tables = Tables::new();

    for balance in &events.balances {
        let contract_bytes = balance.contract.as_deref().ok_or_else(|| Error::msg("ERC-20 balance without a contract"))?;
        let contract = address(contract_bytes, "ERC-20 contract")?;
        let address = address(&balance.address, "ERC-20 holder")?;
        let key = [("contract", contract.to_string()), ("address", address.to_string())];
        let row = tables
            .create_row("erc20_balances", key)
            .set("contract", &contract)
            .set("address", &address)
            .set("balance", amount(&balance.amount)?);
        block.set(row);
    }

    // ONLY include blocks if events are present. Every table `Tables` holds has
    // at least one row, so this is upstream's `!tables.tables.is_empty()`.
    if tables.all_row_count() != 0 {
        block.set(tables.create_row("blocks", [("block_num", block.block_num.clone())]));
    }

    substreams::log::info!("Total rows {}", tables.all_row_count());
    let mut changes = tables.to_database_changes();
    // The crate emits a row's fields in `HashMap` order, which changes with
    // every WASM build; the sink reads them by name. Sorting keeps the output
    // bytes stable across builds. Composite keys stay in map order.
    for change in &mut changes.table_changes {
        change.fields.sort_by(|a, b| a.name.cmp(&b.name));
    }
    Ok(changes)
}
