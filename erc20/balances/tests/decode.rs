//! `map_events` decodes its block with a larger element budget than buffa's
//! default.
use buffa::Message;
use erc20_balances::decode_block;
use substreams_ethereum::pb::eth::v2 as eth;

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
