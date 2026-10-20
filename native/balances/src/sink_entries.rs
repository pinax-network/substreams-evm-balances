//! `map_sink_entries` output: the native balances of a block as
//! `sf.substreams.foundational_store.model.v2.SinkEntries`, the only shape a
//! StreamingFast hosted store ingests (owner, 2026-10-20).
//!
//! One `Entry` per account with a persisted change in the block: the key is the
//! 20-byte account address and the value an `Any` holding
//! `evm.balances.v1.NativeBalance`, the address and its balance after the
//! block. `if_not_exist` is false, so every block overwrites the stored
//! balance. A store keeps an account's last written balance; an account the
//! store does not hold has not changed since the store's start block, which is
//! no observation of a zero balance.
//!
//! `db_out` reads these entries back with [`balances`], which fails the block
//! on any entry this module would not have written.
use buffa_types::google::protobuf::Any;
use proto::pb::evm::balances::v1 as pb;
use substreams::{
    errors::Error,
    pb::sf::substreams::foundational_store::model::v2::{Entry, Key, SinkEntries},
};

/// The `Any` type URL of every value.
pub const VALUE_TYPE_URL: &str = pb::NativeBalance::TYPE_URL;

/// The entries of a block's balances, in the order of `events`.
pub fn entries(events: &pb::NativeEvents) -> SinkEntries {
    SinkEntries {
        entries: events
            .balances
            .iter()
            .map(|balance| Entry {
                key: buffa::MessageField::some(Key {
                    bytes: balance.address.clone(),
                }),
                value: buffa::MessageField::some(Any::pack(balance, VALUE_TYPE_URL)),
            })
            .collect(),
        if_not_exist: false,
    }
}

/// The balances of `entries`, in order. Fails on an entry [`entries`] would
/// not have written: `if_not_exist` set, a missing key or value, another value
/// type, or a key that is not the value's address.
pub fn balances(entries: &SinkEntries) -> Result<Vec<pb::NativeBalance>, Error> {
    if entries.if_not_exist {
        return Err(Error::msg("sink entries set if_not_exist"));
    }
    entries
        .entries
        .iter()
        .map(|entry| {
            let key = entry.key.as_option().ok_or_else(|| Error::msg("sink entry without a key"))?;
            let value = entry.value.as_option().ok_or_else(|| Error::msg("sink entry without a value"))?;
            let balance = value
                .unpack_if::<pb::NativeBalance>(VALUE_TYPE_URL)
                .map_err(|error| Error::msg(format!("sink entry value does not decode: {error}")))?
                .ok_or_else(|| Error::msg(format!("sink entry value is a {:?}, not a {VALUE_TYPE_URL:?}", value.type_url())))?;
            if key.bytes != balance.address {
                return Err(Error::msg("sink entry key is not its balance's address"));
            }
            Ok(balance)
        })
        .collect()
}
