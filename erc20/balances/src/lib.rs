//! A single RPC-free map with the shared ERC-20 Events output: per-block
//! transfer-guided balances inferred from persisted storage (`src/infer.rs`).
//! `db_out` writes them into the substreams-evm ClickHouse tables (`db_out`
//! module).
pub mod db_out;
mod infer;
pub mod persist;

use proto::pb::evm::balances::v1 as balances_pb;
use substreams::{errors::Error, scalar::BigInt};
use substreams_ethereum::pb::eth::v2 as eth;
use tiny_keccak::{Hasher, Keccak};

fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error::msg(message.to_string()))
    }
}
fn hash(bytes: &[u8]) -> [u8; 32] {
    let mut result = [0; 32];
    let mut hasher = Keccak::v256();
    hasher.update(bytes);
    hasher.finalize(&mut result);
    result
}
fn mapping(owner: &[u8], position: &[u8; 32]) -> [u8; 32] {
    let mut preimage = [0; 64];
    preimage[12..32].copy_from_slice(owner);
    preimage[32..].copy_from_slice(position);
    hash(&preimage)
}
fn validate_block(block: &eth::Block) -> Result<(), Error> {
    require(block.detail_level == eth::block::DetailLevel::DetaillevelExtended, "Extended blocks required")?;
    require((3..=5).contains(&block.ver), "unsupported Extended producer version")?;
    let header = block.header.as_option().ok_or_else(|| Error::msg("missing header"))?;
    require(
        block.hash.len() == 32 && header.parent_hash.len() == 32 && header.state_root.len() == 32,
        "invalid block identity",
    )?;
    require(header.number == block.number, "header number mismatch")?;
    for tx in &block.transaction_traces {
        require(
            (1..=3).contains(&tx.status.to_i32()) && !tx.calls.is_empty(),
            "incomplete transaction persistence data",
        )?;
    }
    Ok(())
}

/// Discards every record: `run` needs only the persistence rules' verdict.
struct Discard;
impl persist::Sink for Discard {
    fn storage(&mut self, _: &eth::StorageChange, _: persist::Ctx) {}
    fn balance(&mut self, _: &eth::BalanceChange, _: persist::Ctx) {}
    fn nonce(&mut self, _: &eth::NonceChange, _: persist::Ctx) {}
    fn code(&mut self, _: &eth::CodeChange, _: persist::Ctx) {}
}

/// The `map_events` body. A block fails only when it is not a complete
/// Extended block or the persistence rules cannot resolve it. Inference never
/// fails a block: on any doubt it drops that contract's rows for the block.
/// `block_len` is the length of the block's delivered encoding, one bound on
/// where a producer could have cut call data.
pub fn run(block: &eth::Block, block_len: usize) -> Result<balances_pb::Events, Error> {
    validate_block(block)?;
    persist::collect_block(block, &mut Discard)?;
    let balances = infer::rows(block, block_len)
        .into_iter()
        .map(|((contract, address), value)| balances_pb::Balance {
            contract: Some(contract.to_vec()),
            address: address.to_vec(),
            amount: BigInt::from_unsigned_bytes_be(&value).to_string(),
        })
        .collect();
    Ok(balances_pb::Events { balances })
}
/// The element memory one block decode may materialize: 1 GiB.
///
/// buffa bounds what the elements of repeated fields materialize, 32 MiB by
/// default, so that a small untrusted payload cannot force a large
/// allocation. A Firehose block is trusted input, and BSC block 43,772,003
/// (29.9 MB encoded) failed to decode under the defaults that the SDK's
/// handler macro applies. The wasm32 heap bounds the decode anyway; this
/// budget only keeps its failure an explicit decode error.
pub const BLOCK_ELEMENT_MEMORY_LIMIT: usize = 1 << 30;

/// Decodes the `map_events` input block with [`BLOCK_ELEMENT_MEMORY_LIMIT`];
/// buffa's other decode limits keep their defaults.
pub fn decode_block(bytes: &[u8]) -> Result<eth::Block, buffa::DecodeError> {
    buffa::DecodeOptions::new()
        .with_element_memory_limit(BLOCK_ELEMENT_MEMORY_LIMIT)
        .decode_from_slice(bytes)
}

// The SDK macro generates raw-pointer parameter decoding and discards function
// attributes. Keep its ABI-specific lint exception scoped to this wrapper. The
// export exists only in the WASM build, as in the other packages.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod handler {
    use super::*;

    // Written out because the macro decodes with buffa's default limits (see
    // `decode_block`). Otherwise it is the macro's map export: panic hook,
    // empty-output skip, and a panic with the error's debug text.
    #[no_mangle]
    pub extern "C" fn map_events(block_ptr: *mut u8, block_len: usize) {
        substreams::register_panic_hook();
        // SAFETY: the runtime passes the block's bytes, valid for this call.
        let bytes = unsafe { std::slice::from_raw_parts(block_ptr, block_len) };
        let block =
            decode_block(bytes).unwrap_or_else(|error| panic!("Unable to decode Protobuf data ({block_len} bytes) to 'eth::Block' message's struct: {error}"));
        substreams::skip_empty_output();
        match run(&block, block_len) {
            Ok(events) => substreams::output(events),
            Err(error) => panic!("{error:?}"),
        }
    }

    #[substreams::handlers::map]
    fn db_out(
        clock: substreams::pb::substreams::Clock,
        events: balances_pb::Events,
    ) -> Result<substreams_database_change::pb::sf::substreams::sink::database::v1::DatabaseChanges, Error> {
        db_out::database_changes(&clock, &events)
    }
}
