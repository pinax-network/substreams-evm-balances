//! Inference rules on synthetic blocks, block failures, the manifest shape and
//! captured block 122260950 against its saved same-block RPC values, also as a
//! firehose-tracer 5.5.0 producer would record it.
use buffa::Message;
use substreams_ethereum::pb::eth::v2 as eth;
use tiny_keccak::{Hasher, Keccak};

/// `map_events` on a block delivered as its own encoding.
fn run(b: &eth::Block) -> Result<proto::pb::evm::balances::v1::Events, substreams::errors::Error> {
    erc20_balances::run(b, b.encoded_len() as usize)
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0; 32];
    let mut hasher = Keccak::v256();
    hasher.update(bytes);
    hasher.finalize(&mut out);
    out
}

const TOKEN: [u8; 20] = [0x70; 20];
const IMPL: [u8; 20] = [0x71; 20];
const BEACON: [u8; 20] = [0x72; 20];
const FROM: [u8; 20] = [0xf0; 20];
const A: [u8; 20] = [0xa1; 20];
const B: [u8; 20] = [0xb2; 20];
const C: [u8; 20] = [0xc3; 20];
const D: [u8; 20] = [0xd4; 20];
const E: [u8; 20] = [0xe5; 20];
const FEE: [u8; 20] = [0xfe; 20];
const BASE: u8 = 3;
const TRANSFER: &str = "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const MINT: &str = "0f6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885";
const DEPOSIT: &str = "e1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c";
const WITHDRAWAL: &str = "7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65";
const APPROVAL: &str = "8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925";
/// FiatToken v2.2 `Blacklisted(address indexed)`, a reference candidate shape.
const BLACKLISTED: &str = "ffa4e6181777692565cf28528fc88fd1516ea86b56da075235fa575af6a4b855";
/// TetherToken `Issue(uint256)`, `Redeem(uint256)` and
/// `DestroyedBlackFunds(address,uint256)` (also TrueCurrency's, with the
/// holder indexed), and USDT0's `DestroyedBlockedFunds(address indexed,uint256)`.
const ISSUE: &str = "cb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a";
const REDEEM: &str = "702d5967f45f6513a38ffc42d6ba9bf230bd40e8f53b16363c7eb4fd2deb9a44";
const DESTROYED_BLACK_FUNDS: &str = "61e6e66b0d6339b2980aecc6ccc0039736791f0ccde9ed512e789a7fbdd698c6";
const DESTROYED_BLOCKED_FUNDS: &str = "6a2859ae7902313752498feb80a014e6e7275fe964c79aa965db815db1c7f1e9";
/// TetherToken `destroyBlackFunds(address)`.
const DESTROY_BLACK_FUNDS: [u8; 4] = [0xf3, 0xbd, 0xc2, 0x28];
/// The Tether owner, the sender of its admin calls.
const OWNER: [u8; 20] = [0x0c; 20];
/// WETH9 `deposit()` and `withdraw(uint256)`.
const DEPOSIT_CALL: [u8; 4] = [0xd0, 0xe3, 0x0d, 0xb0];
const WITHDRAW_CALL: [u8; 4] = [0x2e, 0x1a, 0x7d, 0x4d];
/// A token amount unit large enough that exact votes are not dust.
const U: u64 = 1_000_000;

fn w(v: u64) -> Vec<u8> {
    let mut out = vec![0; 32];
    out[24..].copy_from_slice(&v.to_be_bytes());
    out
}
fn pad(a: &[u8; 20]) -> Vec<u8> {
    [vec![0; 12], a.to_vec()].concat()
}
fn preimage(holder: &[u8; 20], slot: u8) -> (String, Vec<u8>) {
    let pre = [pad(holder), w(u64::from(slot))].concat();
    (hex::encode(hash(&pre)), pre)
}
fn call(index: u32, parent: u32, depth: u32, kind: eth::CallType, address: [u8; 20]) -> eth::Call {
    eth::Call {
        index,
        parent_index: parent,
        depth,
        call_type: kind.into(),
        address: address.to_vec(),
        begin_ordinal: 1,
        end_ordinal: 1000,
        ..Default::default()
    }
}
fn root() -> eth::Call {
    call(1, 0, 0, eth::CallType::Call, TOKEN)
}
/// Persists the word at `key` with its same-frame `preimage`.
fn store_key(c: &mut eth::Call, key: [u8; 32], preimage: &[u8], old: Vec<u8>, new: Vec<u8>, ordinal: u64) {
    c.keccak_preimages.insert(hex::encode(key), hex::encode(preimage));
    c.storage_changes.push(eth::StorageChange {
        address: TOKEN.to_vec(),
        key: key.to_vec(),
        old_value: old,
        new_value: new,
        ordinal,
    });
}
/// Persists `holder`'s raw word under mapping `slot` with its preimage.
fn store_word(c: &mut eth::Call, holder: [u8; 20], slot: u8, old: Vec<u8>, new: Vec<u8>, ordinal: u64) {
    let (key, pre) = preimage(&holder, slot);
    store_key(c, hex::decode(key).unwrap().try_into().unwrap(), &pre, old, new, ordinal);
}
/// Persists `holder`'s word under mapping `slot` with its same-frame preimage.
fn store(c: &mut eth::Call, holder: [u8; 20], slot: u8, old: u64, new: u64, ordinal: u64) {
    store_word(c, holder, slot, w(old), w(new), ordinal);
}
/// Persists the plain (unhashed) word at slot `slot`, such as a total supply.
fn store_plain(c: &mut eth::Call, slot: u64, old: u64, new: u64, ordinal: u64) {
    c.storage_changes.push(eth::StorageChange {
        address: TOKEN.to_vec(),
        key: w(slot),
        old_value: w(old),
        new_value: w(new),
        ordinal,
    });
}
/// A log of the token with only topic0 and the given data.
fn unindexed(topic0: &str, data: Vec<u8>) -> eth::Log {
    eth::Log {
        address: TOKEN.to_vec(),
        topics: vec![hex::decode(topic0).unwrap()],
        data,
        ..Default::default()
    }
}
fn log(topic0: &str, holders: &[[u8; 20]], value: u64) -> eth::Log {
    eth::Log {
        address: TOKEN.to_vec(),
        topics: [vec![hex::decode(topic0).unwrap()], holders.iter().map(pad).collect()].concat(),
        data: w(value),
        ..Default::default()
    }
}
fn transfer(from: [u8; 20], to: [u8; 20], value: u64) -> eth::Log {
    log(TRANSFER, &[from, to], value)
}
/// A STATICCALL `balanceOf(holder)` of the token that reads mapping `slot`.
fn balance_of(index: u32, holder: [u8; 20], slot: u8, value: u64, ordinal: u64) -> eth::Call {
    let mut c = call(index, 1, 1, eth::CallType::Static, TOKEN);
    c.input = [vec![0x70, 0xa0, 0x82, 0x31], pad(&holder)].concat();
    c.return_data = w(value);
    let (key, pre) = preimage(&holder, slot);
    c.keccak_preimages.insert(key, hex::encode(pre));
    (c.begin_ordinal, c.end_ordinal) = (ordinal, ordinal + 1);
    c
}
/// A router transaction whose calls follow the root call at depth one.
fn router(calls: Vec<eth::Call>) -> Vec<eth::Call> {
    [vec![call(1, 0, 0, eth::CallType::Call, [0x99; 20])], calls].concat()
}
fn block(txs: Vec<Vec<eth::Call>>) -> eth::Block {
    eth::Block {
        ver: 5,
        number: 7,
        hash: vec![1; 32],
        detail_level: eth::block::DetailLevel::DetaillevelExtended.into(),
        header: buffa::MessageField::some(eth::BlockHeader {
            number: 7,
            parent_hash: vec![2; 32],
            state_root: vec![3; 32],
            ..Default::default()
        }),
        transaction_traces: txs
            .into_iter()
            .enumerate()
            .map(|(i, calls)| eth::TransactionTrace {
                index: i as u32,
                from: FROM.to_vec(),
                status: eth::TransactionTraceStatus::Succeeded.into(),
                calls,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}
/// The token's inferred rows as (holder byte, amount).
fn rows(b: &eth::Block) -> Vec<(u8, String)> {
    run(b)
        .unwrap()
        .balances
        .into_iter()
        .filter(|r| r.contract.as_deref() == Some(&TOKEN[..]))
        .map(|r| (r.address[0], r.amount))
        .collect()
}
fn r(holder: [u8; 20], amount: u64) -> (u8, String) {
    (holder[0], amount.to_string())
}
/// Sets the native value a call moves, recorded as minimal big-endian bytes.
fn pay(c: &mut eth::Call, value: u64) {
    let bytes = value.to_be_bytes();
    let start = bytes.iter().take_while(|b| **b == 0).count();
    c.value = buffa::MessageField::some(eth::BigInt {
        bytes: bytes[start..].to_vec(),
    });
}
/// A WETH9 `deposit()` by `holder` with `wad` of native value, whose word
/// under mapping `slot` goes from `before` to `after`.
fn deposit(holder: [u8; 20], wad: u64, slot: u8, before: u64, after: u64) -> eth::Call {
    let mut c = root();
    c.caller = holder.to_vec();
    c.input = DEPOSIT_CALL.to_vec();
    pay(&mut c, wad);
    c.logs = vec![log(DEPOSIT, &[holder], wad)];
    store(&mut c, holder, slot, before, after, 10);
    c
}
/// A WETH9 `withdraw(wad)` by `holder`, whose word goes from `before` to
/// `after`, and its only call: the payout of `wad` to the holder with the
/// 2,300 gas of a Solidity `transfer`.
fn withdrawal(holder: [u8; 20], wad: u64, before: u64, after: u64) -> Vec<eth::Call> {
    let mut c = root();
    c.caller = holder.to_vec();
    c.input = [WITHDRAW_CALL.to_vec(), w(wad)].concat();
    c.logs = vec![log(WITHDRAWAL, &[holder], wad)];
    store(&mut c, holder, BASE, before, after, 10);
    let mut payout = call(2, 1, 1, eth::CallType::Call, holder);
    payout.caller = TOKEN.to_vec();
    pay(&mut payout, wad);
    payout.gas_limit = 2300;
    vec![c, payout]
}
/// A transfer of 30 from A (100) to B (5) under the voted mapping.
fn simple() -> eth::Call {
    let mut c = root();
    c.logs = vec![transfer(A, B, 30)];
    store(&mut c, A, BASE, 100, 70, 10);
    store(&mut c, B, BASE, 5, 35, 11);
    c
}

#[test]
fn exact_votes_emit_the_last_persisted_word_of_candidate_holders() {
    let mut first = simple();
    // C has a mapping write but no event: not a reference candidate.
    store(&mut first, C, BASE, 1, 2, 12);
    let mut second = root();
    second.logs = vec![transfer(B, A, 5)];
    store(&mut second, B, BASE, 35, 30, 20);
    store(&mut second, A, BASE, 70, 75, 21);
    let b = block(vec![vec![first], vec![second]]);
    // FROM and the token itself are candidates without a write or a read.
    assert_eq!(rows(&b), vec![r(A, 75), r(B, 30)]);
}

#[test]
fn tax_transfers_vote_by_net_flow() {
    let mut c = root();
    c.logs = vec![transfer(A, B, 90), transfer(A, FEE, 10)];
    store(&mut c, A, BASE, 100, 0, 10);
    store(&mut c, B, BASE, 0, 90, 11);
    store(&mut c, FEE, BASE, 0, 10, 12);
    assert_eq!(rows(&block(vec![vec![c]])), vec![r(A, 0), r(B, 90), r(FEE, 10)]);
    // WBTC/FiatToken: Mint repeats the zero-address Transfer, which alone votes.
    let mut wbtc = root();
    wbtc.logs = vec![log(MINT, &[B], 7), transfer([0; 20], B, 7)];
    store(&mut wbtc, B, BASE, 0, 7, 10);
    assert_eq!(rows(&block(vec![vec![wbtc]])), vec![r(B, 7)]);
    // DSToken: a Mint-only holder is written under the base that transfers chose.
    let mut ds = root();
    ds.logs = vec![log(MINT, &[C], 4)];
    store(&mut ds, C, BASE, 0, 4, 1);
    assert_eq!(rows(&block(vec![vec![ds.clone()], vec![simple()]])), vec![r(A, 70), r(B, 35), r(C, 4)]);
    // Without a Transfer from the contract in the block, nothing is inferred.
    assert!(rows(&block(vec![vec![ds]])).is_empty());
}

/// A staking pool's `Deposit(user, amount)` that no native value backs, with
/// `userInfo[user].amount` at struct offset 0, matches the WETH shape but is
/// not a token.
#[test]
fn deposits_alone_do_not_make_a_token() {
    let mut pool = root();
    pool.logs = vec![log(DEPOSIT, &[A], 500)];
    store(&mut pool, A, 4, 1000, 1500, 10);
    assert!(rows(&block(vec![vec![pool]])).is_empty());
}

#[test]
fn tied_or_mismatched_bases_emit_nothing() {
    let mut mirror = simple();
    store(&mut mirror, A, 4, 100, 70, 12);
    store(&mut mirror, B, 4, 5, 35, 13);
    assert!(rows(&block(vec![vec![mirror]])).is_empty());
    let mut fee = root();
    fee.logs = vec![transfer(A, B, 30)];
    store(&mut fee, A, BASE, 100, 71, 10);
    store(&mut fee, B, BASE, 5, 34, 11);
    assert!(rows(&block(vec![vec![fee]])).is_empty());
    // One exact side outvotes one mismatched side, unless it is dust.
    let silent_fee = |u: u64| {
        let mut c = root();
        c.logs = vec![transfer(A, B, 30 * u)];
        store(&mut c, A, BASE, 100 * u, 69 * u, 10);
        store(&mut c, B, BASE, 5 * u, 35 * u, 11);
        store(&mut c, TOKEN, BASE, 0, u, 12);
        block(vec![vec![c]])
    };
    assert_eq!(rows(&silent_fee(U)), vec![r(TOKEN, U), r(A, 69 * U), r(B, 35 * U)]);
    assert!(rows(&silent_fee(1)).is_empty());
}

/// An allowance `allowance[A][A]` that moves with A's `transferFrom` is an
/// inner mapping, never a balance.
#[test]
fn inner_mappings_are_never_chosen() {
    let mut c = root();
    c.logs = vec![transfer(A, B, 30)];
    let (inner, inner_pre) = preimage(&A, 1);
    let inner: [u8; 32] = hex::decode(inner).unwrap().try_into().unwrap();
    store_key(&mut c, inner, &inner_pre, w(0), w(0), 9);
    let outer_pre = [pad(&A), inner.to_vec()].concat();
    store_key(&mut c, hash(&outer_pre), &outer_pre, w(1000), w(970), 10);
    assert!(rows(&block(vec![vec![c]])).is_empty());
}

#[test]
fn reverted_frames_neither_vote_nor_persist() {
    let mut parent = simple();
    parent.end_ordinal = 100;
    let mut reverted = call(2, 1, 1, eth::CallType::Call, TOKEN);
    reverted.state_reverted = true;
    reverted.logs = vec![transfer(B, A, 1)];
    store(&mut reverted, B, BASE, 35, 34, 30);
    store(&mut reverted, A, BASE, 70, 71, 31);
    assert_eq!(rows(&block(vec![vec![parent, reverted.clone()]])), vec![r(A, 70), r(B, 35)]);
    let mut only = simple();
    only.state_reverted = true;
    assert!(rows(&block(vec![vec![only]])).is_empty());
}

#[test]
fn delegate_frames_match_proxy_logs_and_writes() {
    let proxy = root();
    let mut implementation = simple();
    (implementation.index, implementation.parent_index, implementation.depth) = (2, 1, 1);
    implementation.call_type = eth::CallType::Delegate.into();
    implementation.address = IMPL.to_vec();
    assert_eq!(rows(&block(vec![vec![proxy, implementation]])), vec![r(A, 70), r(B, 35)]);
}

#[test]
fn contradicting_or_impure_balance_of_reads_exclude_the_token() {
    let with = |reads: Vec<eth::Call>| rows(&block(vec![vec![simple()], router(reads)]));
    assert_eq!(with(vec![balance_of(2, B, BASE, 35, 50)]), vec![r(A, 70), r(B, 35)]);
    assert!(with(vec![balance_of(2, B, BASE, 36, 50)]).is_empty());
    // A read before the write is checked against the write's old word.
    assert!(rows(&block(vec![router(vec![balance_of(2, B, BASE, 6, 5)]), vec![simple()]])).is_empty());
    // balanceOf also reads another holder mapping, or reads a different one.
    let mut other = balance_of(2, B, BASE, 35, 50);
    let (key, pre) = preimage(&B, 9);
    other.keccak_preimages.insert(key, hex::encode(pre));
    assert!(with(vec![other]).is_empty());
    assert!(with(vec![balance_of(2, B, 9, 35, 50)]).is_empty());
    // A call out of the token's storage context, unless it is a beacon lookup.
    let mut outside = call(3, 2, 2, eth::CallType::Static, [0x55; 20]);
    outside.return_data = w(1);
    assert!(with(vec![balance_of(2, B, BASE, 35, 50), outside]).is_empty());
    let mut proxy = balance_of(2, B, BASE, 35, 50);
    proxy.keccak_preimages.clear();
    let mut lookup = call(3, 2, 2, eth::CallType::Static, BEACON);
    lookup.return_data = pad(&IMPL);
    let mut delegate = balance_of(4, B, BASE, 35, 50);
    (delegate.parent_index, delegate.depth, delegate.call_type, delegate.address) = (2, 2, eth::CallType::Delegate.into(), IMPL.to_vec());
    assert_eq!(with(vec![proxy, lookup, delegate]), vec![r(A, 70), r(B, 35)]);
}

#[test]
fn pure_reads_supply_unwritten_candidate_holders() {
    let with = |reads: Vec<eth::Call>| rows(&block(vec![vec![simple()], router(reads)]));
    assert_eq!(with(vec![balance_of(2, FROM, BASE, 42, 50)]), vec![r(A, 70), r(B, 35), r(FROM, 42)]);
    // C is not a candidate; two different unchanged reads are a contradiction.
    assert_eq!(with(vec![balance_of(2, C, BASE, 42, 50)]), vec![r(A, 70), r(B, 35)]);
    assert!(with(vec![balance_of(2, FROM, BASE, 42, 50), balance_of(3, FROM, BASE, 43, 60)]).is_empty());
}

/// Producers on firehose-tracer 5.5.0 and later drop the preimages that explain
/// no storage write of their transaction, so a `balanceOf` of an unchanged
/// holder usually records none. Such an unkeyed read still excludes the token
/// when it contradicts the stored word or calls out, but not for lacking a
/// preimage, and it never chooses the base, supplies an unwritten holder or
/// vouches for an unexplained write.
#[test]
fn unkeyed_reads_only_veto() {
    let unkeyed = |index: u32, holder: [u8; 20], value: u64| {
        let mut c = balance_of(index, holder, BASE, value, 50);
        c.keccak_preimages.clear();
        c
    };
    let with = |txs: Vec<Vec<eth::Call>>, reads: Vec<eth::Call>| rows(&block([txs, vec![router(reads)]].concat()));
    assert_eq!(with(vec![vec![simple()]], vec![unkeyed(2, B, 35)]), vec![r(A, 70), r(B, 35)]);
    assert!(with(vec![vec![simple()]], vec![unkeyed(2, B, 36)]).is_empty());
    let mut outside = call(3, 2, 2, eth::CallType::Static, [0x55; 20]);
    outside.return_data = w(1);
    assert!(with(vec![vec![simple()]], vec![unkeyed(2, B, 35), outside]).is_empty());
    assert_eq!(with(vec![vec![simple()]], vec![unkeyed(2, FROM, 42)]), vec![r(A, 70), r(B, 35)]);
    let mut mirror = simple();
    store(&mut mirror, A, 4, 100, 70, 12);
    store(&mut mirror, B, 4, 5, 35, 13);
    assert!(with(vec![vec![mirror]], vec![unkeyed(2, B, 35)]).is_empty());
    let mut sync = root();
    sync.logs = vec![log(APPROVAL, &[C, D], 1)];
    store(&mut sync, C, BASE, 50, 60, 20);
    assert_eq!(with(vec![vec![simple()], vec![sync]], vec![unkeyed(2, C, 60)]), vec![r(A, 70), r(B, 35)]);
}

/// Producers on firehose-tracer 5.5.0 (`Block.ver` 5) cut a transaction's
/// later internal calls to their selector past 50 MiB of call input and drop
/// their return data past 25 MiB, and cut a block over 1 GiB again with
/// halved limits. A successful `balanceOf` of the token that keeps only its
/// selector or has no return data may then be a read whose veto was lost, so
/// it drops the token's rows where a cut was possible: in a transaction whose
/// internal calls record more than 25 MiB of input or 12.5 MiB of return
/// data, or in a block of 64 MiB or 150M gas. Elsewhere it is neither a read
/// nor a doubt.
#[test]
fn cut_balance_of_calls_drop_the_token_where_a_cut_was_possible() {
    let mut input = balance_of(2, B, BASE, 35, 50);
    input.input.truncate(4);
    let mut output = balance_of(2, B, BASE, 35, 50);
    output.return_data.clear();
    let mut big_input = call(3, 1, 1, eth::CallType::Call, [0x55; 20]);
    big_input.input = vec![0; (25 << 20) + 1];
    let mut big_return = call(3, 1, 1, eth::CallType::Call, [0x55; 20]);
    big_return.return_data = vec![0; (25 << 19) + 1];
    let with = |calls: Vec<eth::Call>| block(vec![vec![simple()], router(calls)]);
    let kept = vec![r(A, 70), r(B, 35)];
    // A 4-byte call that a token compiled before Solidity 0.5 answers with
    // balanceOf(0), or a call that returned nothing.
    assert_eq!(rows(&with(vec![input.clone()])), kept);
    assert_eq!(rows(&with(vec![output.clone()])), kept);
    assert!(rows(&with(vec![input.clone(), big_input.clone()])).is_empty());
    assert!(rows(&with(vec![output.clone(), big_input.clone()])).is_empty());
    assert!(rows(&with(vec![output.clone(), big_return.clone()])).is_empty());
    // Exactly 25 MiB of internal input, the 4-byte call's own included.
    let mut exact = big_input.clone();
    exact.input.truncate((25 << 20) - 4);
    assert_eq!(rows(&with(vec![input.clone(), exact])), kept);
    // The root call's input is never cut and does not count.
    let mut root_input = with(vec![input.clone()]);
    root_input.transaction_traces[1].calls[0].input = big_input.input.clone();
    assert_eq!(rows(&root_input), kept);
    // A block that a later halving could have cut.
    let mut gas = with(vec![input.clone()]);
    gas.header.get_or_insert_default().gas_used = 150_000_000;
    assert!(rows(&gas).is_empty());
    let small = with(vec![input.clone()]);
    let at = |len: usize| erc20_balances::run(&small, len).unwrap().balances.len();
    assert_eq!((at(64 << 20), at((64 << 20) - 1)), (0, 2));
    // A caller that passes 0, or the length of a re-encoding, keeps the
    // bound: the block's own encoding counts too.
    let mut padded = with(vec![input.clone()]);
    padded.transaction_traces[1].calls[0].input = vec![0; 64 << 20];
    assert_eq!(erc20_balances::run(&padded, 0).unwrap().balances.len(), 0);
    padded.transaction_traces[1].calls.truncate(1);
    assert_eq!(erc20_balances::run(&padded, 0).unwrap().balances.len(), 2);
    // Producers before 5.5.0 (Block.ver 3 and 4) cut nothing.
    let mut older = with(vec![input.clone(), big_input.clone()]);
    older.ver = 4;
    assert_eq!(rows(&older), kept);
    // A failed call, or a call to another contract, is not a cut read.
    input.status_failed = true;
    assert_eq!(rows(&with(vec![input.clone(), big_input.clone()])), kept);
    (input.status_failed, input.address) = (false, [0x55; 20].to_vec());
    assert_eq!(rows(&with(vec![input, big_input])), kept);
}

/// A `balanceOf` input of more than 36 bytes is read as
/// `balanceOf(input[16..36])`, as the ABI decoder reads it (arbitrage bots
/// reuse a 68-byte `transfer(address,uint256)` buffer): it vetoes, vouches and
/// supplies a value like a 36-byte read. A shorter input, or a longer one with
/// nonzero upper address bytes, succeeds only on a token compiled before
/// Solidity 0.5, which zero-extends and masks the argument: it only vetoes. A
/// 36-byte input with nonzero upper bytes stays ignored.
#[test]
fn long_balance_of_inputs_are_reads() {
    // Holders whose zero-extended 28-byte argument names them.
    const Y: [u8; 20] = [0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0, 0, 0, 0, 0, 0, 0, 0];
    const Z: [u8; 20] = [0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0, 0, 0, 0, 0, 0, 0, 0];
    let with = |reads: Vec<eth::Call>| rows(&block(vec![vec![simple()], router(reads)]));
    let kept = vec![r(A, 70), r(B, 35)];
    let long = |holder: [u8; 20], value: u64, tail: &[u8]| {
        let mut c = balance_of(2, holder, BASE, value, 50);
        c.input.extend_from_slice(tail);
        c
    };
    assert_eq!(with(vec![long(B, 35, &[0; 32])]), kept);
    let tail = hex::decode("0694f01c00000000000000000000000000000000000000000000000000000000").unwrap();
    assert_eq!(with(vec![long(B, 35, &tail)]), kept);
    assert_eq!(with(vec![long(B, 35, &[0; 28])]), kept);
    assert!(with(vec![long(B, 36, &[0; 32])]).is_empty());
    assert_eq!(with(vec![long(FROM, 42, &[0; 32])]), vec![r(A, 70), r(B, 35), r(FROM, 42)]);
    // An unkeyed long read supplies no value.
    let mut bare = long(FROM, 42, &[0; 32]);
    bare.keccak_preimages.clear();
    assert_eq!(with(vec![bare]), kept);
    let mut unkeyed = long(B, 35, &[0; 32]);
    unkeyed.keccak_preimages.clear();
    let mut outside = call(3, 2, 2, eth::CallType::Static, [0x55; 20]);
    outside.return_data = w(1);
    assert!(with(vec![unkeyed, outside]).is_empty());
    // Nonzero upper address bytes: the masked holder, veto only.
    let dirty = |holder: [u8; 20], value: u64, tail: &[u8]| {
        let mut c = long(holder, value, tail);
        c.input[4] = 1;
        c
    };
    assert_eq!(with(vec![dirty(B, 35, &[0; 32])]), kept);
    assert!(with(vec![dirty(B, 36, &[0; 32])]).is_empty());
    assert_eq!(with(vec![dirty(FROM, 42, &[0; 32])]), kept);
    assert_eq!(with(vec![dirty(B, 36, &[])]), kept);
    // A 28-byte input: the zero-extended argument, veto only.
    let mut moved = root();
    moved.logs = vec![transfer(Z, E, 10 * U), log(APPROVAL, &[Y, D], 1)];
    store(&mut moved, Z, BASE, 50 * U, 40 * U, 30);
    store(&mut moved, E, BASE, 0, 10 * U, 31);
    let short = |holder: [u8; 20], value: u64| {
        let mut c = balance_of(2, holder, BASE, value, 50);
        c.input.truncate(28);
        c
    };
    let both = |read: eth::Call| rows(&block(vec![vec![simple()], vec![moved.clone()], router(vec![read])]));
    let all = vec![r(Z, 40 * U), r(A, 70), r(B, 35), r(E, 10 * U)];
    assert_eq!(both(short(Z, 40 * U)), all);
    assert!(both(short(Z, 41 * U)).is_empty());
    assert_eq!(both(short(Y, 42)), all);
    assert_eq!(
        both(balance_of(2, Y, BASE, 42, 50)),
        vec![r(Y, 42), r(Z, 40 * U), r(A, 70), r(B, 35), r(E, 10 * U)]
    );
}

/// A reflection-style second mapping (`rOwned`) that moves with the flows
/// suggests a computed `balanceOf`. A traced read vouches only for its holder,
/// whose code path may differ (e.g. `holder == pair ? tOwned : rOwned / rate`).
#[test]
fn other_holder_state_moving_with_transfers_needs_the_holders_own_read() {
    let mut c = root();
    c.logs = vec![transfer(C, B, 30), transfer(A, C, 20)];
    store(&mut c, C, BASE, 1000, 990, 10);
    store(&mut c, B, BASE, 5, 35, 11);
    store(&mut c, A, BASE, 100, 80, 12);
    store(&mut c, C, 4, 1_000_000, 990_000, 13);
    store(&mut c, B, 4, 6_000, 36_000, 14);
    store(&mut c, A, 4, 101_000, 81_000, 15);
    assert!(rows(&block(vec![vec![c.clone()]])).is_empty());
    let read = router(vec![balance_of(2, C, BASE, 990, 50)]);
    assert_eq!(rows(&block(vec![vec![c], read])), vec![r(C, 990)]);
}

/// Counters (deltas below a thousandth of the flow) do not shadow a base with
/// two or more exact votes, unless another mapping also matches exactly or the
/// base has a single exact vote.
#[test]
fn counters_do_not_shadow_a_well_supported_base() {
    let counted = |extra: &dyn Fn(&mut eth::Call)| {
        let mut c = root();
        c.logs = vec![transfer(A, B, 30 * U)];
        store(&mut c, A, BASE, 100 * U, 70 * U, 10);
        store(&mut c, B, BASE, 5 * U, 35 * U, 11);
        store(&mut c, A, 7, 1, 2, 12);
        store(&mut c, B, 7, 1, 2, 13);
        extra(&mut c);
        rows(&block(vec![vec![c]]))
    };
    assert_eq!(counted(&|_| {}), vec![r(A, 70 * U), r(B, 35 * U)]);
    assert!(counted(&|c| store(c, A, 5, 100 * U, 70 * U, 14)).is_empty());
    let mut mint = root();
    mint.logs = vec![transfer([0; 20], B, 30 * U)];
    store(&mut mint, B, BASE, 5 * U, 35 * U, 10);
    store(&mut mint, B, 7, 1, 2, 11);
    assert!(rows(&block(vec![vec![mint]])).is_empty());
}

/// A tie between two exactly moving mappings is broken by the traced reads.
#[test]
fn pure_reads_choose_the_base_on_a_tie() {
    let mut mirror = simple();
    store(&mut mirror, A, 4, 100, 70, 12);
    store(&mut mirror, B, 4, 5, 35, 13);
    let reads = router(vec![balance_of(2, B, BASE, 35, 50)]);
    assert_eq!(rows(&block(vec![vec![mirror], reads])), vec![r(A, 70), r(B, 35)]);
}

/// Scaled balances (stored = amount / index, Aave-style): a dust transfer that
/// rounds to an exact match does not certify the mapping.
#[test]
fn dust_matches_do_not_certify_a_mismatching_base() {
    let mut big = root();
    big.logs = vec![transfer(A, B, 1000 * U)];
    store(&mut big, A, BASE, 2000 * U, 1048 * U, 10);
    store(&mut big, B, BASE, 0, 952 * U, 11);
    let mut dust = root();
    dust.logs = vec![transfer(C, FEE, 1)];
    store(&mut dust, C, BASE, 500, 499, 10);
    store(&mut dust, FEE, BASE, 0, 1, 11);
    assert!(rows(&block(vec![vec![big], vec![dust]])).is_empty());
}

#[test]
fn self_destructed_contracts_are_not_inferred() {
    let mut destroyed = simple();
    destroyed.suicide = true;
    assert!(rows(&block(vec![vec![destroyed]])).is_empty());
    let mut removed = simple();
    removed.code_changes = vec![eth::CodeChange {
        address: TOKEN.to_vec(),
        old_hash: vec![1; 32],
        new_hash: hash(&[]).to_vec(),
        ordinal: 20,
        ..Default::default()
    }];
    assert!(rows(&block(vec![vec![removed]])).is_empty());
}

/// Many interleaved writes and reads of one word are checked in ordinal
/// order (binary search, not a scan per read).
#[test]
fn interleaved_reads_check_the_word_at_their_ordinal() {
    let n = 2000;
    let mut c = root();
    c.logs = vec![transfer(A, B, 30 * U)];
    store(&mut c, B, BASE, 5 * U, 35 * U, 1);
    let mut value = 100 * U;
    let mut reads = Vec::new();
    for i in 0..n {
        let next = if i + 1 == n { 70 * U } else { value - 1 };
        store(&mut c, A, BASE, value, next, 10 + 10 * i);
        reads.push(balance_of(2 + i as u32, A, BASE, next, 15 + 10 * i));
        value = next;
    }
    let b = |reads: Vec<eth::Call>| block(vec![vec![c.clone()], router(reads)]);
    assert_eq!(rows(&b(reads.clone())), vec![r(A, 70 * U), r(B, 35 * U)]);
    reads[n as usize / 2].return_data = w(1);
    assert!(rows(&b(reads)).is_empty());
}

/// `Deposit`/`Withdrawal`/`Mint`/`Burn` name holders but do not vote, except a
/// native-backed WETH9 frame in a block where the contract has no `Transfer`
/// flow: a staking pool's `userInfo[user].amount` or a lock ledger moving with
/// them is not a balance.
#[test]
fn unbacked_credits_and_debits_never_vote() {
    let staking = |topic0: &str, holder: [u8; 20], before: u64, after: u64| {
        let mut c = root();
        c.logs = vec![log(topic0, &[holder], before.abs_diff(after))];
        store(&mut c, holder, 4, before, after, 10);
        c
    };
    // A first stake that pays a zero reward with Transfer(TOKEN, A, 0).
    let mut first = staking(DEPOSIT, A, 0, 500 * U);
    first.logs.push(transfer(TOKEN, A, 0));
    assert!(rows(&block(vec![vec![first]])).is_empty());
    // Stakes and an unstake do not outvote one ordinary transfer.
    let mut t = root();
    t.logs = vec![transfer(D, E, 10 * U)];
    store(&mut t, D, BASE, 100 * U, 90 * U, 10);
    store(&mut t, E, BASE, 5 * U, 15 * U, 11);
    let txs = vec![
        vec![staking(DEPOSIT, A, 0, 500 * U)],
        vec![staking(DEPOSIT, B, 40 * U, 100 * U)],
        vec![staking(WITHDRAWAL, C, 9 * U, 3 * U)],
        vec![t],
    ];
    assert_eq!(rows(&block(txs)), vec![r(D, 90 * U), r(E, 15 * U)]);
    // A stake that also pays a reward: only the Transfer is A's balance flow.
    let mut reward = staking(DEPOSIT, A, 0, 500 * U);
    reward.logs.push(transfer(TOKEN, A, 2 * U));
    store(&mut reward, TOKEN, BASE, 100 * U, 98 * U, 11);
    store(&mut reward, A, BASE, 0, 2 * U, 12);
    assert_eq!(rows(&block(vec![vec![reward]])), vec![r(TOKEN, 98 * U), r(A, 2 * U)]);
    // A batch lock-mint: Mint per beneficiary, one Transfer to the contract.
    let mut lock = root();
    lock.logs = vec![log(MINT, &[A], 300 * U), log(MINT, &[B], 200 * U), transfer([0; 20], TOKEN, 500 * U)];
    store(&mut lock, A, 6, 0, 300 * U, 10);
    store(&mut lock, B, 6, 0, 200 * U, 11);
    store(&mut lock, TOKEN, BASE, 1000 * U, 1500 * U, 12);
    assert_eq!(rows(&block(vec![vec![lock]])), vec![r(TOKEN, 1500 * U)]);
    // A native staking pool's stake by its holder with the exact value: it
    // also writes the pool total, or credits less or someone else.
    let mut total = deposit(A, 500 * U, 4, 0, 500 * U);
    store_plain(&mut total, 5, 1000 * U, 1500 * U, 11);
    assert!(rows(&block(vec![vec![total]])).is_empty());
    let mut less = deposit(A, 500 * U, 4, 0, 500 * U);
    pay(&mut less, 500 * U - 1);
    assert!(rows(&block(vec![vec![less]])).is_empty());
    let mut other = deposit(A, 500 * U, 4, 0, 500 * U);
    other.caller = B.to_vec();
    assert!(rows(&block(vec![vec![other]])).is_empty());
}

/// In a block where the contract has no `Transfer` flow, a WETH9 frame votes
/// the amount of its only log of the contract when the frame's execution
/// backs it: a `deposit()` (or the fallback) by the holder that receives
/// exactly the wad and makes no call, or a `withdraw(wad)` by the holder whose
/// only call pays exactly the wad to the holder with the 2,300 gas of a
/// `transfer`. Each writes one word of the contract, the holder's.
#[test]
fn native_backed_wrapped_deposits_and_withdrawals_vote_without_transfers() {
    let one = |calls: Vec<eth::Call>| rows(&block(vec![calls]));
    let d = || deposit(A, 500 * U, BASE, 100 * U, 600 * U);
    let wd = || withdrawal(B, 30 * U, 50 * U, 20 * U);
    assert_eq!(one(vec![d()]), vec![r(A, 600 * U)]);
    assert_eq!(one(wd()), vec![r(B, 20 * U)]);
    // Wallets append bytes to the call data, which WETH9 ignores; a value with
    // a leading zero byte is the same number.
    let mut tagged = d();
    tagged.input.extend(w(500 * U));
    let mut fallback = d();
    fallback.input.clear();
    fallback.value.get_or_insert_default().bytes.insert(0, 0);
    let mut zerion = wd();
    zerion[0].input.extend(b"zerion");
    assert_eq!(rows(&block(vec![vec![tagged], zerion])), vec![r(A, 600 * U), r(B, 20 * U)]);
    assert_eq!(one(vec![fallback]), vec![r(A, 600 * U)]);
    type Edit = Box<dyn Fn(&mut Vec<eth::Call>)>;
    let deposits: Vec<(&str, Edit)> = vec![
        ("another selector", Box::new(|c| c[0].input = [0xa9, 0x05, 0x9c, 0xbb].to_vec())),
        ("no value", Box::new(|c| c[0].value = Default::default())),
        ("an Approval too", Box::new(|c| c[0].logs.push(log(APPROVAL, &[A, D], 1)))),
        ("a fee: 499 credited for 500", Box::new(|c| c[0].storage_changes[0].new_value = w(599 * U))),
        ("a child call", Box::new(|c| c.push(call(2, 1, 1, eth::CallType::Static, [0x55; 20])))),
        (
            "a reverted child call",
            Box::new(|c| {
                let mut child = call(2, 1, 1, eth::CallType::Call, [0x55; 20]);
                child.state_reverted = true;
                c.push(child);
            }),
        ),
        (
            "a 0-wei deposit without a write",
            Box::new(|c| {
                c[0].logs = vec![log(DEPOSIT, &[A], 0)];
                c[0].value = Default::default();
                c[0].storage_changes.clear();
            }),
        ),
        (
            "a DELEGATECALL frame under a proxy",
            Box::new(|c| {
                let mut implementation = c.remove(0);
                (implementation.index, implementation.parent_index, implementation.depth) = (2, 1, 1);
                (implementation.call_type, implementation.address) = (eth::CallType::Delegate.into(), IMPL.to_vec());
                let mut proxy = root();
                (proxy.caller, proxy.input) = (A.to_vec(), implementation.input.clone());
                pay(&mut proxy, 500 * U);
                *c = vec![proxy, implementation];
            }),
        ),
    ];
    for (case, edit) in deposits {
        let mut calls = vec![d()];
        edit(&mut calls);
        assert!(one(calls).is_empty(), "deposit: {case}");
    }
    let withdrawals: Vec<(&str, Edit)> = vec![
        ("a payout of 30 + 1", Box::new(|c| pay(&mut c[1], 30 * U + 1))),
        ("a payout to C", Box::new(|c| c[1].address = C.to_vec())),
        ("a payout with input", Box::new(|c| c[1].input = vec![0])),
        ("a payout with 100,000 gas", Box::new(|c| c[1].gas_limit = 100_000)),
        ("a reverted payout", Box::new(|c| c[1].state_reverted = true)),
        ("a failed payout", Box::new(|c| c[1].status_failed = true)),
        ("no payout", Box::new(|c| drop(c.pop()))),
        (
            "a second value call",
            Box::new(|c| {
                let mut second = c[1].clone();
                second.index = 3;
                c.push(second);
            }),
        ),
        ("input for another wad", Box::new(|c| c[0].input = [WITHDRAW_CALL.to_vec(), w(31 * U)].concat())),
        ("input cut to the selector", Box::new(|c| c[0].input.truncate(4))),
        ("a frame that receives value", Box::new(|c| pay(&mut c[0], 1))),
        ("the holder is not the caller", Box::new(|c| c[0].caller = A.to_vec())),
        ("a second write", Box::new(|c| store_plain(&mut c[0], 5, 0, 1, 11))),
    ];
    for (case, edit) in withdrawals {
        let mut calls = wd();
        edit(&mut calls);
        assert!(one(calls).is_empty(), "withdrawal: {case}");
    }
    // A block with a Transfer flow keeps only its Transfer votes.
    let keyed = deposit(A, 500 * U, 4, 100 * U, 600 * U);
    assert_eq!(rows(&block(vec![vec![simple()], vec![keyed.clone()]])), vec![r(A, 70), r(B, 35)]);
    // Accepted (owner, 2026-10-17): no state crosses blocks, so a contract
    // whose Transfers vote base 3 in other blocks still gets the word of a
    // WETH9-shaped ledger (here base 4) in a block without a Transfer.
    assert_eq!(one(vec![keyed]), vec![r(A, 600 * U)]);
}

/// A packed word `lastBlock << 128 | balance`: a holder's first touch in the
/// block also moves the block field (a large mismatch), later touches only the
/// balance (exact). More large mismatches than exact votes reject the mapping.
#[test]
fn packed_words_whose_other_field_moves_are_not_chosen() {
    let packed = |high: u64, low: u64| {
        let mut out = w(low);
        out[8..16].copy_from_slice(&high.to_be_bytes());
        out
    };
    let (mut pair, mut last) = (1000 * U, 480);
    let mut txs = Vec::new();
    for (i, buyer) in [A, B, C].into_iter().enumerate() {
        let (amount, at) = ((10 + i as u64) * U, 10 * i as u64);
        let mut c = root();
        c.logs = vec![transfer(FEE, buyer, amount)];
        store_word(&mut c, FEE, BASE, packed(last, pair), packed(500, pair - amount), 10 + at);
        store_word(&mut c, buyer, BASE, packed(477, U), packed(500, U + amount), 11 + at);
        (pair, last) = (pair - amount, 500);
        txs.push(vec![c]);
    }
    assert!(rows(&block(txs)).is_empty());
}

/// A word at or above 2^128 is not a plain balance: another field shares the
/// slot and did not move in the block (a packed timestamp, FiatToken v2.2's
/// blacklist flag in bit 255). The token's rows for the block are dropped.
#[test]
fn words_at_or_above_2_pow_128_drop_the_token() {
    let with = |byte: usize, set: u8, value: u64| {
        let mut out = w(value);
        out[byte] = set;
        out
    };
    let transfer_of = |a: (Vec<u8>, Vec<u8>), b: (Vec<u8>, Vec<u8>)| {
        let mut c = root();
        c.logs = vec![transfer(A, B, 30)];
        store_word(&mut c, A, BASE, a.0, a.1, 10);
        store_word(&mut c, B, BASE, b.0, b.1, 11);
        block(vec![vec![c]])
    };
    // Every holder's word carries the other field.
    let b = transfer_of((with(0, 1, 100), with(0, 1, 70)), (with(0, 1, 5), with(0, 1, 35)));
    assert!(rows(&b).is_empty());
    // One holder's flag drops the token's other rows in the block too.
    let b = transfer_of((with(0, 0x80, 100), with(0, 0x80, 70)), (w(5), w(35)));
    assert!(rows(&b).is_empty());
    // A word just below 2^128 is kept.
    let b = transfer_of((with(16, 1, 100), with(16, 1, 70)), (with(16, 1, 5), with(16, 1, 35)));
    let high = 1u128 << 120;
    assert_eq!(rows(&b), vec![(A[0], (high + 70).to_string()), (B[0], (high + 35).to_string())]);
}

/// A third party's word that changes with no balance event naming it in that
/// frame (an approval sync, FiatToken v2.2's blacklist flag in bit 255) needs
/// the holder's own `balanceOf`; the contract's own silent fee writes do not.
/// FiatToken's `balanceOf` masks the flag, so its read contradicts the stored
/// word and drops the token for the block (a documented limit).
#[test]
fn unexplained_writes_need_the_holders_own_read() {
    let mut flag = root();
    flag.logs = vec![eth::Log {
        address: TOKEN.to_vec(),
        topics: vec![hex::decode(BLACKLISTED).unwrap(), pad(&C)],
        ..Default::default()
    }];
    let mut flagged = w(50 * U);
    flagged[0] |= 0x80;
    store_word(&mut flag, C, BASE, w(50 * U), flagged, 20);
    assert_eq!(rows(&block(vec![vec![simple()], vec![flag.clone()]])), vec![r(A, 70), r(B, 35)]);
    // Limit: a read of C after the flag returns the masked word, which
    // contradicts the stored one, and drops the token's rows for the block.
    let masked = router(vec![balance_of(2, C, BASE, 50 * U, 50)]);
    assert!(rows(&block(vec![vec![simple()], vec![flag], masked])).is_empty());
    // An approval makes C a candidate; its silent sync is kept once C's own
    // balanceOf confirms the word.
    let mut sync = root();
    sync.logs = vec![log(APPROVAL, &[C, D], 1)];
    store(&mut sync, C, BASE, 50, 60, 20);
    assert_eq!(rows(&block(vec![vec![simple()], vec![sync.clone()]])), vec![r(A, 70), r(B, 35)]);
    let read = router(vec![balance_of(2, C, BASE, 60, 50)]);
    assert_eq!(rows(&block(vec![vec![simple()], vec![sync], read])), vec![r(A, 70), r(B, 35), r(C, 60)]);
}

/// A keyed `balanceOf` vouches for an unexplained write only when it was
/// traced after that write: a read before it validated only the word it saw,
/// not a later flag, admin or sync write. Under shadowing, a read at any
/// point still keeps a holder whose write a `Transfer` names.
#[test]
fn reads_vouch_only_for_writes_they_saw() {
    let with = |txs: Vec<Vec<eth::Call>>| rows(&block(txs));
    let mut sync = root();
    sync.logs = vec![log(APPROVAL, &[C, D], 1)];
    store(&mut sync, C, BASE, 50, 60, 20);
    // v0.7.3 kept C at 60 on the earlier read of its old word.
    let before = router(vec![balance_of(2, C, BASE, 50, 5)]);
    assert_eq!(with(vec![before, vec![simple()], vec![sync.clone()]]), vec![r(A, 70), r(B, 35)]);
    let after = router(vec![balance_of(2, C, BASE, 60, 50)]);
    assert_eq!(with(vec![vec![simple()], vec![sync], after]), vec![r(A, 70), r(B, 35), r(C, 60)]);
    // FiatToken v2.2 blacklists C after a read of its balance: the flag is no
    // longer vouched for, so it no longer drops the token (2^128 rule).
    let mut flag = root();
    flag.logs = vec![eth::Log {
        address: TOKEN.to_vec(),
        topics: vec![hex::decode(BLACKLISTED).unwrap(), pad(&C)],
        ..Default::default()
    }];
    let mut flagged = w(50 * U);
    flagged[0] |= 0x80;
    store_word(&mut flag, C, BASE, w(50 * U), flagged, 20);
    let before = router(vec![balance_of(2, C, BASE, 50 * U, 5)]);
    assert_eq!(with(vec![before, vec![simple()], vec![flag]]), vec![r(A, 70), r(B, 35)]);
    // A shadowed block: C's write is named by its Transfers, so a read of C
    // before it still keeps C. D's sync write is unexplained: only a read
    // after it keeps D.
    let mut shadowed = root();
    shadowed.logs = vec![transfer(C, B, 30), transfer(A, C, 20)];
    store(&mut shadowed, C, BASE, 1000, 990, 10);
    store(&mut shadowed, B, BASE, 5, 35, 11);
    store(&mut shadowed, A, BASE, 100, 80, 12);
    store(&mut shadowed, C, 4, 1_000_000, 990_000, 13);
    store(&mut shadowed, B, 4, 6_000, 36_000, 14);
    store(&mut shadowed, A, 4, 101_000, 81_000, 15);
    let mut sync = root();
    sync.logs = vec![log(APPROVAL, &[D, E], 1)];
    store(&mut sync, D, BASE, 50, 60, 20);
    let reads = |d: eth::Call| router(vec![balance_of(2, C, BASE, 1000, 5), d]);
    let txs = |d: eth::Call| vec![vec![shadowed.clone()], vec![sync.clone()], reads(d)];
    assert_eq!(with(txs(balance_of(3, D, BASE, 50, 6))), vec![r(C, 990)]);
    assert_eq!(with(txs(balance_of(3, D, BASE, 60, 50))), vec![r(C, 990), r(D, 60)]);
}

/// Tether's owner-only `issue`/`redeem` move the sender's balance, and its
/// `destroyBlackFunds` zeroes a blacklisted holder's, each with the total
/// supply and without a `Transfer`. The event explains the holder's write when
/// the frame's net change of the word equals its amount, a destruction also
/// leaving it at zero; otherwise the write stays unexplained (no row), and the
/// token keeps its other rows.
#[test]
fn tether_admin_events_explain_exact_writes() {
    // A CALL from OWNER with one admin event, a holder's word and the supply.
    let admin = |event: eth::Log, holder: [u8; 20], before: u64, after: u64, supply: (u64, u64)| {
        let mut c = root();
        c.caller = OWNER.to_vec();
        c.logs = vec![event];
        store(&mut c, holder, BASE, before, after, 20);
        store_plain(&mut c, 1, supply.0, supply.1, 21);
        c
    };
    let with = |tx: Vec<eth::Call>| rows(&block(vec![vec![simple()], tx]));
    let issue = |amount: u64| unindexed(ISSUE, w(amount));
    let redeem = |amount: u64| unindexed(REDEEM, w(amount));
    let destroyed = |holder: [u8; 20], amount: u64| unindexed(DESTROYED_BLACK_FUNDS, [pad(&holder), w(amount)].concat());
    let simple_rows = vec![r(A, 70), r(B, 35)];
    let plus = |row: (u8, String)| [simple_rows.clone(), vec![row]].concat();
    let issued = admin(issue(5 * U), OWNER, 10 * U, 15 * U, (100 * U, 105 * U));
    assert_eq!(with(vec![issued]), [vec![r(OWNER, 15 * U)], simple_rows.clone()].concat());
    assert_eq!(with(vec![admin(issue(5 * U), OWNER, 10 * U, 16 * U, (100 * U, 105 * U))]), simple_rows);
    let redeemed = admin(redeem(5 * U), OWNER, 15 * U, 10 * U, (105 * U, 100 * U));
    assert_eq!(with(vec![redeemed.clone()]), [vec![r(OWNER, 10 * U)], simple_rows.clone()].concat());
    // Under a proxy the event is in the DELEGATECALL frame, whose recorded
    // caller is the proxy itself: the sender is the proxy's caller.
    let mut implementation = redeemed.clone();
    (implementation.index, implementation.parent_index, implementation.depth) = (2, 1, 1);
    (implementation.call_type, implementation.address, implementation.caller) = (eth::CallType::Delegate.into(), IMPL.to_vec(), TOKEN.to_vec());
    let mut proxy = root();
    proxy.caller = OWNER.to_vec();
    assert_eq!(with(vec![proxy, implementation]), [vec![r(OWNER, 10 * U)], simple_rows.clone()].concat());
    // TetherToken's destruction names its holder in data word 0.
    assert_eq!(with(vec![admin(destroyed(C, 50 * U), C, 50 * U, 0, (1000 * U, 950 * U))]), plus(r(C, 0)));
    for (before, after) in [(51 * U, 0), (50 * U, 1), (51 * U, U)] {
        let supply = (1000 * U, 1000 * U - (before - after));
        assert_eq!(with(vec![admin(destroyed(C, 50 * U), C, before, after, supply)]), simple_rows);
    }
    let mut dirty = destroyed(C, 50 * U);
    dirty.data[0] = 1;
    assert_eq!(with(vec![admin(dirty, C, 50 * U, 0, (1000 * U, 950 * U))]), simple_rows);
    // TrueCurrency's destruction indexes its holder (owner, 2026-10-17).
    let true_currency = |after: u64| admin(log(DESTROYED_BLACK_FUNDS, &[C], 50 * U), C, 50 * U, after, (1000 * U, 950 * U + after));
    assert_eq!(with(vec![true_currency(0)]), plus(r(C, 0)));
    assert_eq!(with(vec![true_currency(1)]), simple_rows);
    // USDT0's redemption also emits Transfer(owner, 0): one row.
    let mut both = redeemed;
    both.logs.insert(0, transfer(OWNER, [0; 20], 5 * U));
    assert_eq!(rows(&block(vec![vec![both]])), vec![r(OWNER, 10 * U)]);
    // Accepted: an explained word at or above 2^128 drops the token's rows,
    // as any emitted word does.
    let high = |v: u64| {
        let mut out = w(v);
        out[0] = 1;
        out
    };
    let mut packed = admin(issue(5 * U), OWNER, 0, 0, (100 * U, 105 * U));
    packed.storage_changes.remove(0);
    store_word(&mut packed, OWNER, BASE, high(10 * U), high(15 * U), 20);
    assert!(with(vec![packed]).is_empty());
}

/// Lido stETH's `ExternalSharesBurnt(uint256)` adds the emitting frame's
/// recorded caller as a candidate, as the RPC reference does. Under stETH's
/// proxy the DELEGATECALL frame emits it, whose recorded caller is the proxy
/// itself, not the proxy's msg.sender (unlike Tether `Issue`/`Redeem`).
#[test]
fn steth_external_shares_burnt_names_the_recorded_caller() {
    const BURNT: &str = "ad21467656c56eb8c99f7916faa12f7a657a04d8802100acec62e92451ac5606";
    // D's keyed read supplies its value only if D is a candidate.
    let with = |tx: Vec<eth::Call>| rows(&block(vec![vec![simple()], tx, router(vec![balance_of(2, D, BASE, 42, 50)])]));
    let mut direct = root();
    direct.caller = D.to_vec();
    direct.logs = vec![unindexed(BURNT, w(5))];
    assert_eq!(with(vec![direct]), vec![r(A, 70), r(B, 35), r(D, 42)]);
    let mut proxy = root();
    proxy.caller = D.to_vec();
    let mut implementation = call(2, 1, 1, eth::CallType::Delegate, IMPL);
    implementation.caller = TOKEN.to_vec();
    implementation.logs = vec![unindexed(BURNT, w(5))];
    assert_eq!(with(vec![proxy, implementation]), vec![r(A, 70), r(B, 35)]);
}

/// USDT0's `destroyBlockedFunds` burns through `Transfer(holder, 0)`, which
/// already names the holder.
#[test]
fn destroyed_blocked_funds_are_explained_by_their_transfer() {
    let mut c = root();
    c.logs = vec![transfer(C, [0; 20], 50 * U), log(DESTROYED_BLOCKED_FUNDS, &[C], 50 * U)];
    store(&mut c, C, BASE, 50 * U, 0, 20);
    assert_eq!(rows(&block(vec![vec![simple()], vec![c]])), vec![r(A, 70), r(B, 35), r(C, 0)]);
}

/// In a block where TetherToken has no `Transfer` flow (a blacklisted holder
/// never transfers again), a `destroyBlackFunds(holder)` frame votes: a CALL
/// that makes no call, whose only log of the token is the 1-topic
/// `DestroyedBlackFunds(holder, amount)`, and whose writes are the holder's
/// word, ending at zero, and the unhashed supply, falling by the amount.
/// `Issue`/`Redeem` frames never vote: a deprecated TetherToken still issues
/// to its owner after its `balanceOf` forwards to the upgraded contract.
#[test]
fn tether_destructions_vote_without_transfers() {
    let destruction = |holder: [u8; 20], amount: u64, ordinal: u64, supply: (u64, u64)| {
        let mut c = root();
        c.caller = OWNER.to_vec();
        c.input = [DESTROY_BLACK_FUNDS.to_vec(), pad(&holder)].concat();
        c.logs = vec![unindexed(DESTROYED_BLACK_FUNDS, [pad(&holder), w(amount)].concat())];
        store(&mut c, holder, BASE, amount, 0, ordinal);
        store_plain(&mut c, 1, supply.0, supply.1, ordinal + 1);
        c
    };
    let c = || destruction(C, 50 * U, 20, (1000 * U, 950 * U));
    assert_eq!(rows(&block(vec![vec![c()]])), vec![r(C, 0)]);
    // The owner Safe's call data ends with a zero word (ETH 22,730,180).
    let mut safe = c();
    safe.input.extend(w(0));
    let d = destruction(D, 20 * U, 30, (950 * U, 930 * U));
    assert_eq!(rows(&block(vec![vec![safe], vec![d]])), vec![r(C, 0), r(D, 0)]);
    type Edit = Box<dyn Fn(&mut eth::Call)>;
    let edits: Vec<(&str, Edit)> = vec![
        ("a third write", Box::new(|c| store_plain(c, 7, 0, 1, 22))),
        ("no supply write", Box::new(|c| drop(c.storage_changes.pop()))),
        ("the supply falls by 49 for 50", Box::new(|c| c.storage_changes[1].new_value = w(951 * U))),
        (
            "the holder's word ends at U",
            Box::new(|c| (c.storage_changes[0].old_value, c.storage_changes[0].new_value) = (w(51 * U), w(U))),
        ),
        ("no preimage of the holder's word", Box::new(|c| c.keccak_preimages.clear())),
        ("another selector", Box::new(|c| c.input[..4].copy_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]))),
        ("no input", Box::new(|c| c.input.clear())),
        (
            "the input names another holder",
            Box::new(|c| c.input = [DESTROY_BLACK_FUNDS.to_vec(), pad(&D)].concat()),
        ),
        ("a second log", Box::new(|c| c.logs.push(log(APPROVAL, &[C, D], 1)))),
        ("TrueCurrency's event", Box::new(|c| c.logs = vec![log(DESTROYED_BLACK_FUNDS, &[C], 50 * U)])),
    ];
    for (case, edit) in edits {
        let mut frame = c();
        edit(&mut frame);
        assert!(rows(&block(vec![vec![frame]])).is_empty(), "{case}");
    }
    // A child call, even a reverted one: in deprecated mode the amount comes
    // from the upgraded contract's balanceOf.
    let mut child = call(2, 1, 1, eth::CallType::Static, [0x55; 20]);
    child.return_data = w(50 * U);
    assert!(rows(&block(vec![vec![c(), child.clone()]])).is_empty());
    (child.status_failed, child.state_reverted) = (true, true);
    assert!(rows(&block(vec![vec![c(), child]])).is_empty());
    // A DELEGATECALL frame under a proxy.
    let mut implementation = c();
    (implementation.index, implementation.parent_index, implementation.depth) = (2, 1, 1);
    (implementation.call_type, implementation.address, implementation.caller) = (eth::CallType::Delegate.into(), IMPL.to_vec(), TOKEN.to_vec());
    let mut proxy = root();
    (proxy.caller, proxy.input) = (OWNER.to_vec(), implementation.input.clone());
    assert!(rows(&block(vec![vec![proxy, implementation]])).is_empty());
    // An issue or a redemption alone.
    for (event, before, after) in [(ISSUE, 10 * U, 15 * U), (REDEEM, 15 * U, 10 * U)] {
        let mut owner = root();
        owner.caller = OWNER.to_vec();
        owner.logs = vec![unindexed(event, w(5 * U))];
        store(&mut owner, OWNER, BASE, before, after, 20);
        store_plain(&mut owner, 1, 100 * U + before, 100 * U + after, 21);
        assert!(rows(&block(vec![vec![owner]])).is_empty(), "{event}");
    }
}

/// Bor's system address, StateReceiver and ValidatorSet.
const BOR_SYSTEM: [u8; 20] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe,
];
const STATE_RECEIVER: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10, 0x01];
const VALIDATOR_SET: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10, 0x00];

/// A Polygon `Block.ver` 3 block after `earlier`, whose last transaction is a
/// Bor state sync (ordinals 100 to 200) committing one bridge mint of the
/// token per `(holder, amount)`. Each depth-1 `commitState` call restarts the
/// ordinals: its mint frame (101 to 110) writes the holder's word at 102 and
/// the total supply (slot 2) at 103, so the supply's writes tie by ordinal
/// and chain in call order. There are storage records only, no balance
/// records.
fn bor(earlier: Vec<Vec<eth::Call>>, mints: &[([u8; 20], u64)]) -> eth::Block {
    let zero = [0; 20];
    let mut root = call(1, 0, 0, eth::CallType::Call, zero);
    (root.caller, root.begin_ordinal, root.end_ordinal) = (zero.to_vec(), 0, 200);
    let mut calls = vec![root];
    let mut supply = 1000 * U;
    for (i, (holder, amount)) in mints.iter().enumerate() {
        let index = 2 + 2 * i as u32;
        let mut commit = call(index, 1, 1, eth::CallType::Call, STATE_RECEIVER);
        (commit.caller, commit.begin_ordinal, commit.end_ordinal) = (BOR_SYSTEM.to_vec(), 0, 120);
        let mut mint = call(index + 1, index, 2, eth::CallType::Call, TOKEN);
        (mint.caller, mint.begin_ordinal, mint.end_ordinal) = (STATE_RECEIVER.to_vec(), 101, 110);
        mint.logs = vec![transfer(zero, *holder, *amount)];
        store(&mut mint, *holder, BASE, 0, *amount, 102);
        store_plain(&mut mint, 2, supply, supply + amount, 103);
        supply += amount;
        calls.extend([commit, mint]);
    }
    let mut b = block([earlier, vec![calls]].concat());
    b.ver = 3;
    let tx = b.transaction_traces.last_mut().unwrap();
    (tx.from, tx.to, tx.begin_ordinal, tx.end_ordinal) = (zero.to_vec(), zero.to_vec(), 100, 200);
    b
}
/// A transaction appended after the block's others, with the given calls and
/// ordinals.
fn append(b: &mut eth::Block, from: [u8; 20], to: [u8; 20], calls: Vec<eth::Call>, ordinals: (u64, u64)) {
    b.transaction_traces.push(eth::TransactionTrace {
        index: b.transaction_traces.len() as u32,
        from: from.to_vec(),
        to: to.to_vec(),
        status: eth::TransactionTraceStatus::Succeeded.into(),
        begin_ordinal: ordinals.0,
        end_ordinal: ordinals.1,
        calls,
        ..Default::default()
    });
}

/// Polygon's `Block.ver` 3 state-sync transaction restarts the ordinals in
/// each depth-1 `commitState` call. In the exact shape (native v0.7.8's,
/// checked over storage and balance records), its records are ordered by
/// depth-1 call, then ordinal: writes chain in call order, and a read compares
/// with the word that order gives. A transaction with the state-sync
/// signature in any other layout drops every token it writes or reads.
#[test]
fn bor_state_sync_writes_chain_in_call_order() {
    let mints = [(A, 5 * U), (B, 7 * U)];
    let synced = vec![r(A, 5 * U), r(B, 7 * U)];
    // By ordinal the supply's two writes tie at 103, and v0.7.3 dropped the token.
    assert_eq!(rows(&bor(vec![], &mints)), synced);
    // An earlier transaction of the token, and a span commit after the sync.
    let mut earlier = root();
    earlier.logs = vec![transfer(D, E, 10 * U)];
    store(&mut earlier, D, BASE, 100 * U, 90 * U, 10);
    store(&mut earlier, E, BASE, 0, 10 * U, 11);
    let mut spanned = bor(vec![vec![earlier.clone()]], &mints);
    let mut commit = call(1, 0, 0, eth::CallType::Call, VALIDATOR_SET);
    (commit.caller, commit.begin_ordinal, commit.input) = (BOR_SYSTEM.to_vec(), 0, vec![0x23, 0xc2, 0xa2, 0xb4]);
    append(&mut spanned, BOR_SYSTEM, VALIDATOR_SET, vec![commit], (201, 210));
    assert_eq!(rows(&spanned), vec![r(A, 5 * U), r(B, 7 * U), r(D, 90 * U), r(E, 10 * U)]);
    // Writes that chain in reverse call order.
    let mut reversed = bor(vec![], &mints);
    let calls = &mut reversed.transaction_traces[0].calls;
    (calls[2].storage_changes[1].old_value, calls[2].storage_changes[1].new_value) = (w(1007 * U), w(1012 * U));
    (calls[4].storage_changes[1].old_value, calls[4].storage_changes[1].new_value) = (w(1000 * U), w(1007 * U));
    assert!(rows(&reversed).is_empty());
    // The producer's later numbering, subtree after subtree: call order is
    // ordinal order.
    let mut sequential = bor(vec![], &mints);
    let second = &mut sequential.transaction_traces[0].calls[4];
    (second.begin_ordinal, second.end_ordinal) = (111, 120);
    second.storage_changes.iter_mut().for_each(|c| c.ordinal += 10);
    assert_eq!(rows(&sequential), synced);
    // Not the exact shape, although these writes chain by ordinal (v0.7.3
    // emitted them): another transaction after the sync, gas used, or another
    // transaction's storage record inside the sync's ordinals.
    let mut later = sequential.clone();
    append(&mut later, FROM, [0x55; 20], vec![call(1, 0, 0, eth::CallType::Call, [0x55; 20])], (300, 310));
    assert!(rows(&later).is_empty());
    let mut gas = sequential.clone();
    gas.transaction_traces[0].gas_used = 21_000;
    assert!(rows(&gas).is_empty());
    let mut other = call(1, 0, 0, eth::CallType::Call, [0x55; 20]);
    other.storage_changes.push(eth::StorageChange {
        address: [0x55; 20].to_vec(),
        key: w(0),
        old_value: w(0),
        new_value: w(1),
        ordinal: 150,
    });
    assert!(rows(&bor(vec![vec![other]], &mints)).is_empty());
    // A transaction with the signature in another layout drops the tokens it
    // only reads, too.
    let mut reader = bor(vec![vec![simple()]], &[]);
    let mut commit = call(2, 1, 1, eth::CallType::Call, STATE_RECEIVER);
    (commit.caller, commit.begin_ordinal) = (BOR_SYSTEM.to_vec(), 0);
    let mut read = balance_of(3, B, BASE, 35, 150);
    (read.parent_index, read.depth) = (2, 2);
    reader.transaction_traces[1].calls.extend([commit, read]);
    assert_eq!(rows(&reader), vec![r(A, 70), r(B, 35)]);
    reader.transaction_traces[1].gas_used = 21_000;
    assert!(rows(&reader).is_empty());
    // A read in the second subtree of a word that the first wrote at a higher
    // ordinal compares with the first subtree's new word.
    let mut ahead = bor(vec![], &mints);
    ahead.transaction_traces[0].calls[2].storage_changes[0].ordinal = 108;
    let with = |value: u64| {
        let mut b = ahead.clone();
        let mut read = balance_of(6, A, BASE, value, 105);
        (read.parent_index, read.depth) = (5, 3);
        b.transaction_traces[0].calls.push(read);
        rows(&b)
    };
    assert_eq!(with(5 * U), synced);
    assert!(with(0).is_empty());
}

/// Vyper's `HashMap` key preimage of `holder` under the slot word `slot`:
/// `slot || pad(holder)`, the reverse of Solidity's.
fn vyper_preimage(holder: &[u8; 20], slot: &[u8]) -> (String, Vec<u8>) {
    let pre = [slot.to_vec(), pad(holder)].concat();
    (hex::encode(hash(&pre)), pre)
}
/// Persists `holder`'s word under Vyper `HashMap` slot `slot` with its
/// same-frame preimage.
fn store_vyper(c: &mut eth::Call, holder: [u8; 20], slot: &[u8], old: u64, new: u64, ordinal: u64) {
    let (key, pre) = vyper_preimage(&holder, slot);
    store_key(c, hex::decode(key).unwrap().try_into().unwrap(), &pre, w(old), w(new), ordinal);
}
/// A STATICCALL `balanceOf(holder)` of a Vyper token that reads `HashMap`
/// slot `slot`.
fn balance_of_vyper(index: u32, holder: [u8; 20], slot: &[u8], value: u64, ordinal: u64) -> eth::Call {
    let mut c = balance_of(index, holder, BASE, value, ordinal);
    let (key, pre) = vyper_preimage(&holder, slot);
    c.keccak_preimages = [(key, hex::encode(pre))].into_iter().collect();
    c
}

/// Vyper's `HashMap` keys a holder's word at `keccak(slot || pad(holder))`,
/// the reverse of Solidity's order, in every release (crvUSD, CRV, Curve pools
/// and gauges, Yearn vaults). Each order is its own base, so they never share
/// votes or reads, and a Vyper slot is below 2^64. Under a Vyper base the
/// contract's own row is never emitted, from a write or a read: a Yearn v3
/// vault's `balanceOf(self)` subtracts the shares still unlocking, which fall
/// with time and no write.
#[test]
fn vyper_hash_maps_key_the_slot_first() {
    let one = w(1);
    let transfer_under = |slot: &[u8]| {
        let mut c = root();
        c.logs = vec![transfer(A, B, 30 * U)];
        store_vyper(&mut c, A, slot, 100 * U, 70 * U, 10);
        store_vyper(&mut c, B, slot, 5 * U, 35 * U, 11);
        c
    };
    let kept = vec![r(A, 70 * U), r(B, 35 * U)];
    // v0.7.3 read these preimages only as `pad(holder) || base`, whose holder,
    // address(1), has no flow, and inferred nothing.
    assert_eq!(rows(&block(vec![vec![transfer_under(&one)]])), kept);
    assert_eq!(rows(&block(vec![vec![transfer_under(&w(u64::MAX))]])), kept);
    let mut high = w(0);
    high[23] = 1;
    assert!(rows(&block(vec![vec![transfer_under(&high)]])).is_empty());
    // One exact vote in each order is a tie, not two votes for slot 1.
    let mut mixed = root();
    mixed.logs = vec![transfer(A, B, 30 * U)];
    store_vyper(&mut mixed, A, &one, 100 * U, 70 * U, 10);
    store(&mut mixed, B, 1, 5 * U, 35 * U, 11);
    assert!(rows(&block(vec![vec![mixed]])).is_empty());
    // Keyed reads in Vyper's order check, vouch and supply holders; a read
    // keyed in Solidity's order reads another mapping.
    let with = |reads: Vec<eth::Call>| rows(&block(vec![vec![transfer_under(&one)], router(reads)]));
    assert_eq!(with(vec![balance_of_vyper(2, B, &one, 35 * U, 50)]), kept);
    assert!(with(vec![balance_of_vyper(2, B, &one, 36 * U, 50)]).is_empty());
    assert_eq!(
        with(vec![balance_of_vyper(2, FROM, &one, 42, 50)]),
        vec![r(A, 70 * U), r(B, 35 * U), r(FROM, 42)]
    );
    // The contract's own read supplies no row (Solidity's would).
    assert_eq!(with(vec![balance_of_vyper(2, TOKEN, &one, 42, 50)]), kept);
    assert!(with(vec![balance_of(2, B, 1, 35 * U, 50)]).is_empty());
    // The contract's own word, which `Transfer(0, TOKEN)` names (a Yearn v3
    // report mints its fee shares to the vault).
    let report = |vyper: bool| {
        let mut c = root();
        c.logs = vec![transfer(A, B, 30 * U), transfer([0; 20], TOKEN, 5 * U)];
        for (holder, old, new, ordinal) in [(A, 100 * U, 70 * U, 10), (B, 5 * U, 35 * U, 11), (TOKEN, 20 * U, 25 * U, 12)] {
            if vyper {
                store_vyper(&mut c, holder, &one, old, new, ordinal);
            } else {
                store(&mut c, holder, 1, old, new, ordinal);
            }
        }
        c
    };
    let own = [vec![r(TOKEN, 25 * U)], kept.clone()].concat();
    assert_eq!(rows(&block(vec![vec![report(false)]])), own);
    assert_eq!(rows(&block(vec![vec![report(true)]])), kept);
    // A read after the write that shows the word still gives no own row.
    let read = |value: u64, ordinal: u64| router(vec![balance_of_vyper(2, TOKEN, &one, value, ordinal)]);
    assert_eq!(rows(&block(vec![vec![report(true)], read(25 * U, 50)])), kept);
    assert_eq!(rows(&block(vec![read(20 * U, 5), vec![report(true)]])), kept);
    // `balanceOf(self)` less the unlocking shares contradicts the stored word.
    assert!(rows(&block(vec![vec![report(true)], read(24 * U, 50)])).is_empty());
}

/// Curve's BoostV2 (Vyper) keeps `received[user]` as a `Point` (bias, slope,
/// ts) at the user's key and the two words after it. Its `checkpoint_user`
/// emits `Transfer(user, 0, dbias)` and lowers the bias by exactly dbias, but
/// its `balanceOf` is the voting escrow's balance less the delegated point
/// plus the received one. A holder's word with a written word right after it
/// is a struct field, which may not be the balance: unless the holder's own
/// keyed `balanceOf` traced after its last write shows the word (an ERC-4626
/// vault that keeps a holder index next to the shares, ETH 23,892,123), the
/// contract is dropped for the block. BoostV2's `balanceOf` calls the voting
/// escrow, which vetoes.
#[test]
fn struct_values_are_not_balances() {
    let six = w(6);
    let key: [u8; 32] = hex::decode(vyper_preimage(&C, &six).0).unwrap().try_into().unwrap();
    let after = |i: u8| {
        let mut out = key;
        let (sum, carry) = out[31].overflowing_add(i);
        out[31] = sum;
        assert!(!carry);
        out
    };
    // `checkpoint_user(C)`: the bias, then the given words after it (slope 1,
    // ts 2), in one frame.
    let frame = |fields: &[u8]| {
        let mut c = root();
        c.logs = vec![transfer(C, [0; 20], 5 * U)];
        store_vyper(&mut c, C, &six, 50 * U, 45 * U, 20);
        for (ordinal, i) in (21..).zip(fields) {
            c.storage_changes.push(eth::StorageChange {
                address: TOKEN.to_vec(),
                key: after(*i).to_vec(),
                old_value: w(1),
                new_value: w(2),
                ordinal,
            });
        }
        c
    };
    let checkpoint = |fields: &[u8]| rows(&block(vec![vec![frame(fields)]]));
    assert_eq!(checkpoint(&[]), vec![r(C, 45 * U)]);
    assert!(checkpoint(&[1, 2]).is_empty());
    // Without a slope change only the ts moves.
    assert!(checkpoint(&[2]).is_empty());
    assert!(checkpoint(&[1]).is_empty());
    // A keyed read of C after the write shows the word; one before it does not.
    let read = |ordinal: u64| router(vec![balance_of_vyper(2, C, &six, 45 * U, ordinal)]);
    assert_eq!(rows(&block(vec![vec![frame(&[1, 2])], read(50)])), vec![r(C, 45 * U)]);
    let mut early = read(5);
    early[1].return_data = w(50 * U);
    assert!(rows(&block(vec![early, vec![frame(&[1, 2])]])).is_empty());
    // A struct under Solidity's order, and a field written in another frame.
    let mut solidity = simple();
    let (b_key, _) = preimage(&B, BASE);
    let mut field: [u8; 32] = hex::decode(b_key).unwrap().try_into().unwrap();
    field[31] = field[31].wrapping_add(1);
    let mut other = root();
    other.storage_changes.push(eth::StorageChange {
        address: TOKEN.to_vec(),
        key: field.to_vec(),
        old_value: w(1),
        new_value: w(2),
        ordinal: 30,
    });
    assert!(rows(&block(vec![vec![solidity.clone()], vec![other]])).is_empty());
    solidity.storage_changes.push(eth::StorageChange {
        address: TOKEN.to_vec(),
        key: field.to_vec(),
        old_value: w(1),
        new_value: w(2),
        ordinal: 12,
    });
    assert!(rows(&block(vec![vec![solidity]])).is_empty());
}

/// A holder whose address is the number of the mapping's slot, such as
/// address(3) under slot 3 or address(0) under slot 0, has the preimage
/// `pad(x) || pad(x)`, which reads as its key in both orders. Both readings
/// name one storage key: a read of it is pure, and a write of it is credited
/// to one order only, the one the frame's other keys of that slot use
/// (Solidity's otherwise). A symmetric read cannot break a tie between the two
/// orders.
#[test]
fn symmetric_preimages_name_one_key() {
    let mut x = [0; 20];
    x[19] = BASE;
    let kept = vec![r(A, 70), r(B, 35)];
    // v0.7.3 read one base here and kept the token's rows.
    assert_eq!(rows(&block(vec![vec![simple()], router(vec![balance_of(2, x, BASE, 0, 50)])])), kept);
    let mut slot0 = root();
    slot0.logs = vec![transfer(A, B, 30)];
    store(&mut slot0, A, 0, 100, 70, 10);
    store(&mut slot0, B, 0, 5, 35, 11);
    assert_eq!(rows(&block(vec![vec![slot0], router(vec![balance_of(2, [0; 20], 0, 0, 50)])])), kept);
    // A symmetric read still checks the word it reads.
    let mut moved = root();
    moved.logs = vec![transfer(A, x, 30)];
    store(&mut moved, A, BASE, 100, 70, 10);
    store(&mut moved, x, BASE, 5, 35, 11);
    let read = |value: u64| router(vec![balance_of(2, x, BASE, value, 50)]);
    assert_eq!(rows(&block(vec![vec![moved.clone()], read(35)])), vec![r(x, 35), r(A, 70)]);
    assert!(rows(&block(vec![vec![moved], read(36)])).is_empty());
    // A lone mint to address(3) under Solidity slot 3, as v0.7.3 emitted it.
    let mut mint = root();
    mint.logs = vec![transfer([0; 20], x, 5 * U)];
    store(&mut mint, x, BASE, 0, 5 * U, 10);
    assert_eq!(rows(&block(vec![vec![mint]])), vec![r(x, 5 * U)]);
    // Under Vyper slot 3: A's key is in Vyper's order, so is address(3)'s.
    let mut vyper = root();
    vyper.logs = vec![transfer(A, x, 30 * U)];
    store_vyper(&mut vyper, A, &w(3), 100 * U, 70 * U, 10);
    store(&mut vyper, x, BASE, 5 * U, 35 * U, 11);
    assert_eq!(rows(&block(vec![vec![vyper]])), vec![r(x, 35 * U), r(A, 70 * U)]);
    // Two orders that tie on A and B stay tied after a read of address(3).
    let mut tie = root();
    tie.logs = vec![transfer(A, B, 30 * U)];
    store(&mut tie, A, BASE, 100 * U, 70 * U, 10);
    store(&mut tie, B, BASE, 5 * U, 35 * U, 11);
    store_vyper(&mut tie, A, &w(3), 100 * U, 70 * U, 12);
    store_vyper(&mut tie, B, &w(3), 5 * U, 35 * U, 13);
    assert!(rows(&block(vec![vec![tie.clone()], router(vec![balance_of(2, x, BASE, 0, 50)])])).is_empty());
    let solidity = router(vec![balance_of(2, B, BASE, 35 * U, 50)]);
    assert_eq!(rows(&block(vec![vec![tie], solidity])), vec![r(A, 70 * U), r(B, 35 * U)]);
}

/// Recorded preimages longer than 64 bytes are never decoded, however often
/// their key is written.
#[test]
fn long_preimages_are_ignored() {
    let mut c = simple();
    let long = vec![7; 4096];
    for i in 0..2000 {
        store_key(&mut c, hash(&long), &long, w(i), w(i + 1), 100 + i);
    }
    assert_eq!(rows(&block(vec![vec![c]])), vec![r(A, 70), r(B, 35)]);
}

/// Only an incomplete Extended block or unresolvable persistence fails the
/// block. Doubt about a contract, such as a malformed storage word, drops its
/// rows for the block instead.
#[test]
fn blocks_fail_only_on_incomplete_extended_data() {
    let fails = |b: &eth::Block| run(b).unwrap_err().to_string();
    let mut b = block(vec![vec![simple()]]);
    b.detail_level = eth::block::DetailLevel::DetaillevelBase.into();
    assert_eq!(fails(&b), "Extended blocks required");
    b = block(vec![vec![simple()]]);
    b.ver = 2;
    assert_eq!(fails(&b), "unsupported Extended producer version");
    assert_eq!(fails(&block(vec![vec![simple()], vec![]])), "incomplete transaction persistence data");
    // A failed SetCode transaction without an authorization/execution boundary.
    b = block(vec![vec![root()]]);
    let tx = &mut b.transaction_traces[0];
    tx.status = eth::TransactionTraceStatus::Failed.into();
    tx.r#type = eth::transaction_trace::Type::TrxTypeSetCode.into();
    tx.set_code_authorizations.push(eth::SetCodeAuthorization {
        authority: Some(C.to_vec()),
        ..Default::default()
    });
    tx.calls[0].begin_ordinal = 0;
    assert!(fails(&b).contains("no authorization/execution ordinal boundary"));
    let mut c = simple();
    c.storage_changes[0].new_value = vec![1; 33];
    assert!(run(&block(vec![vec![c]])).unwrap().balances.is_empty());
}

/// On captured BSC block 122260950, inferred rows equal the saved same-block
/// RPC values wherever those exist: all 18 recorded ERC-20 `balanceOf` checks,
/// and the reference rows of USDT, WBNB, USDC and BTCB (101 of 136 inferred).
#[test]
fn captured_block_matches_the_saved_rpc_balances() {
    use std::collections::{BTreeMap, BTreeSet};
    let block = eth::Block::decode_from_slice(include_bytes!("fixtures/bsc-122260950.pb").as_slice()).unwrap();
    let ours: BTreeMap<_, _> = run(&block)
        .unwrap()
        .balances
        .into_iter()
        .map(|b| {
            (
                (format!("0x{}", hex::encode(b.contract.unwrap())), format!("0x{}", hex::encode(b.address))),
                b.amount,
            )
        })
        .collect();
    assert_eq!(ours.len(), 164);
    let key = |row: &serde_json::Value| {
        (
            row["contract"].as_str().unwrap().to_lowercase(),
            row["address"].as_str().unwrap().to_lowercase(),
        )
    };
    let checks: serde_json::Value = serde_json::from_str(include_str!("fixtures/bsc-122260950.json")).unwrap();
    let erc20: Vec<_> = checks["rpc_checks"].as_array().unwrap().iter().filter(|c| c["contract"] != "").collect();
    assert_eq!(erc20.len(), 18);
    for check in erc20 {
        assert_eq!(ours.get(&key(check)).map(String::as_str), check["balance"].as_str(), "{check}");
    }
    let core: serde_json::Value = serde_json::from_str(include_str!("fixtures/bsc-122260950-core-reference.json")).unwrap();
    let reference: BTreeMap<_, _> = core["balances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (key(r), r["amount"].as_str().unwrap()))
        .collect();
    let tokens: BTreeSet<_> = reference.keys().map(|k| &k.0).collect();
    let inferred: Vec<_> = ours.iter().filter(|(k, _)| tokens.contains(&k.0)).collect();
    assert_eq!((reference.len(), tokens.len(), inferred.len()), (136, 4, 101));
    assert!(inferred.iter().all(|(k, v)| reference.get(*k) == Some(&v.as_str())));
}

/// Captured block 122260950 as a firehose-tracer 5.5.0 producer would record
/// it, without the 383 preimages that explain no storage write of their
/// transaction (`fixtures/bsc-122260950-5.5.0-dropped-preimages.json`). 13 of
/// its `balanceOf` calls lose their holder's preimage and every row stays the
/// same; while unkeyed reads vetoed, 5 tokens lost all 104 of their rows here.
#[test]
fn captured_block_without_read_preimages_keeps_its_rows() {
    let original = eth::Block::decode_from_slice(include_bytes!("fixtures/bsc-122260950.pb").as_slice()).unwrap();
    let dropped: serde_json::Value = serde_json::from_str(include_str!("fixtures/bsc-122260950-5.5.0-dropped-preimages.json")).unwrap();
    let mut filtered = original.clone();
    for d in dropped["dropped"].as_array().unwrap() {
        let (tx, index, hash) = (d[0].as_u64().unwrap() as usize, d[1].as_u64().unwrap() as u32, d[2].as_str().unwrap());
        let call = filtered.transaction_traces[tx].calls.iter_mut().find(|c| c.index == index).unwrap();
        assert!(call.keccak_preimages.remove(hash).is_some(), "{d}");
    }
    // `balanceOf` calls that recorded `pad(holder) || base` in their own frame.
    let keyed = |b: &eth::Block| {
        let holder = |c: &eth::Call| format!("{:0>64}", hex::encode(&c.input[16..]));
        b.transaction_traces
            .iter()
            .flat_map(|tx| &tx.calls)
            .filter(|c| c.input.len() == 36 && c.input[..4] == [0x70, 0xa0, 0x82, 0x31])
            .filter(|c| c.keccak_preimages.values().any(|p| p.len() == 128 && p[..64] == holder(c)))
            .count()
    };
    assert_eq!((keyed(&original), keyed(&filtered)), (175, 162));
    let all = |b: &eth::Block| {
        run(b)
            .unwrap()
            .balances
            .into_iter()
            .map(|r| (r.contract, r.address, r.amount))
            .collect::<Vec<_>>()
    };
    assert_eq!(all(&filtered).len(), 164);
    assert_eq!(all(&filtered), all(&original));
}

/// The v0.7.4 captures in `fixtures/v074/`: Ethereum, BSC and Polygon blocks
/// cut to the transactions of one token, and two one-transaction WBNB captures of
/// `fixtures/wbnb-mutations/`, with provenance in `cases.json`. Each gives
/// exactly its recorded rows, which equal the RPC-era values at the block
/// wherever the RPC-era database has them, and still holds the calls its case
/// names.
#[test]
fn captured_v074_blocks_match_their_rows_and_the_rpc_era_values() {
    use sha2::Digest;
    use std::collections::BTreeMap;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v074");
    let cases: serde_json::Value = serde_json::from_str(include_str!("fixtures/v074/cases.json")).unwrap();
    for case in cases["cases"].as_array().unwrap() {
        let file = case["file"].as_str().unwrap();
        let bytes = std::fs::read(format!("{dir}/{file}")).unwrap();
        assert_eq!(hex::encode(sha2::Sha256::digest(&bytes)), case["fixture_sha256"], "{file}");
        let block = eth::Block::decode_from_slice(&bytes).unwrap();
        assert_eq!(
            (block.number, format!("0x{}", hex::encode(&block.hash))),
            (case["block"].as_u64().unwrap(), case["block_hash"].as_str().unwrap().to_string())
        );
        let kept: Vec<u32> = block.transaction_traces.iter().map(|tx| tx.index).collect();
        assert_eq!(serde_json::json!(kept), case["kept_transactions"], "{file}");
        let mut ours = BTreeMap::<String, BTreeMap<String, String>>::new();
        for b in run(&block).unwrap().balances {
            let contract = format!("0x{}", hex::encode(b.contract.unwrap()));
            ours.entry(contract).or_default().insert(format!("0x{}", hex::encode(b.address)), b.amount);
        }
        assert_eq!(serde_json::json!(ours), case["rows"], "{file}");
        for (contract, values) in case["rpc_era"]["values"].as_object().unwrap() {
            for (holder, value) in values.as_object().unwrap() {
                let row = ours.get(contract).and_then(|t| t.get(holder)).map(String::as_str);
                assert_eq!(row, value.as_str(), "{file} {contract} {holder}");
            }
        }
        let token = case["token"].as_str().unwrap();
        for c in case["calls"].as_array().unwrap() {
            let tx = block.transaction_traces.iter().find(|tx| c["tx"] == tx.index).unwrap();
            let call = tx.calls.iter().find(|x| c["call"] == x.index).unwrap();
            assert_eq!(
                format!("0x{}", hex::encode(&call.address)),
                c["address"].as_str().unwrap_or(token),
                "{file} {c}"
            );
            if let Some(t0) = c["log_topic0"].as_str() {
                assert!(call.logs.iter().any(|l| hex::encode(&l.topics[0]) == t0), "{file} {c}");
            }
            if let Some(n) = c["input_len"].as_u64() {
                assert_eq!(call.input.len() as u64, n, "{file} {c}");
            }
            if let Some(n) = c["return_len"].as_u64() {
                assert_eq!(call.return_data.len() as u64, n, "{file} {c}");
            }
            if let Some(n) = c["gas_limit"].as_u64() {
                assert_eq!(call.gas_limit, n, "{file} {c}");
            }
            if let Some(keyed) = c["keyed"].as_bool() {
                // A verified 64-byte preimage of pad(input[16..36]), in either
                // order, in the call or its DELEGATECALL members.
                let holder = hex::encode(&call.input[16..36]);
                let mut context = vec![call.index];
                for x in &tx.calls {
                    if x.call_type == eth::CallType::Delegate && context.contains(&x.parent_index) {
                        context.push(x.index);
                    }
                }
                let recorded = tx.calls.iter().filter(|x| context.contains(&x.index)).any(|x| {
                    x.keccak_preimages.iter().any(|(key, value)| {
                        let named = value.len() == 128
                            && ((value[..24] == "0".repeat(24) && value[24..64] == holder) || (value[64..88] == "0".repeat(24) && value[88..] == holder));
                        named && hex::encode(hash(&hex::decode(value).unwrap())) == *key
                    })
                });
                assert_eq!(recorded, keyed, "{file} {c}");
            }
        }
    }
}

/// Captured Ethereum block 26,174,479, transaction 178
/// (`fixtures/v074/eth-26174479-scrvusd-report.pb`): a report of scrvUSD, a
/// Yearn v3 vault (Vyper), writes the vault's own word, 5480155112664263237347
/// shares under `HashMap` slot 18. VaultV3 returns that word less the shares
/// still unlocking: the RPC era's `balanceOf(vault)` fell to
/// 325862159115270955558 by block 26,214,028 with no further write. So the
/// vault never gets a row of its own: a keyed read appended after the report
/// that returns the word adds none, and one that returns less contradicts the
/// word and drops the vault's rows for the block; crvUSD's rows stay.
#[test]
fn captured_yearn_v3_vault_never_gets_its_own_row() {
    let block = eth::Block::decode_from_slice(include_bytes!("fixtures/v074/eth-26174479-scrvusd-report.pb").as_slice()).unwrap();
    let vault: [u8; 20] = hex::decode("0655977feb2f289a4ab78af67bab0d17aab84367").unwrap().try_into().unwrap();
    let crvusd = hex::decode("f939e0a03fb07f59a73314e73794be0e57ac1b4e").unwrap();
    let of = |b: &eth::Block, token: &[u8]| {
        let rows = run(b).unwrap().balances.into_iter();
        let rows = rows.filter(|r| r.contract.as_deref() == Some(token));
        rows.map(|r| (hex::encode(r.address), r.amount)).collect::<Vec<_>>()
    };
    let stablecoin = of(&block, &crvusd);
    assert_eq!(stablecoin.len(), 2);
    assert!(of(&block, &vault).is_empty());
    let read = |shares: u128| {
        let mut b = block.clone();
        let end = b.transaction_traces[0].end_ordinal;
        let mut get = balance_of_vyper(2, vault, &w(18), 0, end + 2);
        get.address = vault.to_vec();
        get.return_data[16..].copy_from_slice(&shares.to_be_bytes());
        let mut calls = router(vec![get]);
        (calls[0].begin_ordinal, calls[0].end_ordinal) = (end + 1, end + 4);
        append(&mut b, FROM, [0x99; 20], calls, (end + 1, end + 4));
        b
    };
    let word: u128 = 5480155112664263237347;
    assert!(of(&read(word), &vault).is_empty());
    assert_eq!(of(&read(word), &crvusd), stablecoin);
    assert!(of(&read(word - 1), &vault).is_empty());
    assert_eq!(of(&read(word - 1), &crvusd), stablecoin);
}

#[test]
fn package_has_one_block_only_map_its_db_out_and_only_the_shared_protobuf() {
    let manifest = include_str!("../substreams.yaml");
    let modules = manifest.lines().filter(|line| line.starts_with("  - name:")).collect::<Vec<_>>();
    assert_eq!(modules, vec!["  - name: map_events", "  - name: db_out"]);
    assert!(manifest.contains("    inputs:\n      - source: sf.ethereum.type.v2.Block\n    output:\n"));
    assert!(manifest.contains("    inputs:\n      - source: sf.substreams.v1.Clock\n      - map: map_events\n    output:\n"));
    assert!(!manifest.contains("params"));
    assert!(manifest.contains("files: [balances.proto]"));
    assert!(manifest.contains("type: proto:evm.balances.v1.Events"));
}
