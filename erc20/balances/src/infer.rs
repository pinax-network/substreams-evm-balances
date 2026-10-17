//! RPC-free transfer-guided balances, inferred for every contract from the
//! block's persisted storage. Per block and per contract:
//!
//! 1. Candidates are the RPC reference's `(contract, holder)` pairs: see
//!    [`candidates`]. For Tether `Issue`/`Redeem`, where the reference called
//!    `owner()`, the holder is the sender of the emitting frame: both functions
//!    are owner-only and move the sender's balance. TrueCurrency's
//!    `DestroyedBlackFunds` also names its holder.
//! 2. Every non-reverted call frame of a successful transaction votes. A holder's
//!    signed net `Transfer` flow in the frame is compared with the net delta of
//!    the frame's persisted writes whose key has a verified same-frame Keccak
//!    preimage `pad(holder) || base`. Equal is an exact vote for `base`, unequal
//!    a mismatch. Exact votes for flows below 10^6 do not count for a base that
//!    also mismatches. `Deposit`/`Withdrawal`/`Mint`/`Burn` and Tether's admin
//!    events do not vote, except in a block where the contract has no
//!    `Transfer` flow: there a CALL frame votes the amount of its only log of
//!    the contract when the frame's own execution backs that amount (see
//!    [`backed`]). The cases are a WETH9 `deposit()` by the holder that
//!    receives exactly that native value and makes no call, a WETH9
//!    `withdraw(wad)` by the holder whose only call pays exactly that value to
//!    the holder with the 2,300 gas of a `transfer`, each writing one word of
//!    the contract, and a TetherToken `destroyBlackFunds(holder)` that makes no
//!    call, zeroes the holder's word and moves one unhashed word (the supply)
//!    by the same amount.
//! 3. The base is the mapping every keyed `balanceOf` reads, if it has exact
//!    votes, or else the base with the most exact votes; a tie chooses none.
//!    A traced `balanceOf` is keyed when its storage context recorded a
//!    `pad(holder) || base` preimage. Producers on firehose-tracer 5.5.0 and
//!    later keep only the preimages that explain a storage write of their
//!    transaction, so a read of an unchanged holder is usually not keyed.
//!    Inner (nested) mappings are never chosen, nor a base whose own large
//!    mismatches outnumber its exact votes.
//! 4. The contract is excluded for the block when a persisted record or event
//!    address is malformed, its code is removed or it self-destructs, writes to
//!    one key are discontinuous, a traced `balanceOf` calls out or returns a
//!    value that contradicts storage, a keyed one reads other holder state, or
//!    a successful `balanceOf` call to it keeps only its 4-byte selector or has
//!    no return data, the shape a producer leaves when it cuts a call, where a
//!    cut was possible (see [`cut_possible`]). A `balanceOf` input of more than
//!    36 bytes is read as `balanceOf(input[16..36])`, as the ABI decoder reads
//!    it; a shorter one, or a longer one with nonzero upper address bytes, only
//!    vetoes.
//! 5. When other holder mappings mismatch the event flows at least as often as
//!    the chosen one matches them, only holders with their own keyed
//!    `balanceOf` keep rows. With two or more exact votes and no other mapping
//!    matching, only mismatches of at least a thousandth of the flow count.
//! 6. Candidate holders written under the base get their last persisted word,
//!    unless that write is unexplained. A write is explained when its frame has
//!    a `Transfer`/`Deposit`/`Withdrawal`/`Mint`/`Burn` naming the holder, or a
//!    Tether `Issue`/`Redeem` (for the frame's sender) or `DestroyedBlackFunds`
//!    whose amount equals the frame's net change of the holder's word, a
//!    destruction also leaving it at zero. The contract's own word needs no
//!    explanation. A holder whose last write is unexplained needs its own keyed
//!    `balanceOf`, traced after that write. Candidate holders without a write
//!    get the value of a keyed `balanceOf` that read exactly the base.
//!
//! Inference never fails a block: any detected doubt drops that contract's
//! inferred rows for the block. Holders without a write or such a read stay
//! unknown. Per-block work is bounded by the block's calls, logs, writes and
//! 64-byte preimages; longer recorded preimages are never decoded.
use crate::{eth, hash, mapping, persist};
use std::collections::{BTreeMap, BTreeSet};

type Addr = [u8; 20];
type Word = [u8; 32];
/// (system call, transaction index, call index)
type Frame = (bool, u32, u32);
/// Net voting flow per holder of one contract in one frame.
type Flows<'a> = BTreeMap<(Frame, Addr), (&'a eth::Call, BTreeMap<Addr, Net>)>;
/// Per contract, the holders that its events name or explain.
type Named = BTreeMap<Addr, Names>;
/// Per frame and contract, the net persisted delta and the last new word of
/// each written key.
type Framed = BTreeMap<(Frame, Addr), BTreeMap<Word, (Net, Word)>>;

/// The holders whose writes in a frame a contract's events account for.
#[derive(Default)]
struct Names {
    /// `(frame, holder)` pairs that a balance event names.
    named: BTreeSet<(Frame, Addr)>,
    /// Tether admin events: the signed amount they state for a holder in a
    /// frame, and whether its word must end at zero (a destruction). They
    /// explain the holder's write only when the frame's net change of the
    /// holder's word under the chosen base equals that amount.
    admin: BTreeMap<(Frame, Addr), (Net, bool)>,
}

const SUCCEEDED: eth::TransactionTraceStatus = eth::TransactionTraceStatus::Succeeded;
const CALL: eth::CallType = eth::CallType::Call;
const STATIC: eth::CallType = eth::CallType::Static;
const DELEGATE: eth::CallType = eth::CallType::Delegate;
const BALANCE_OF: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];
/// TetherToken `destroyBlackFunds(address)`.
const DESTROY_BLACK_FUNDS: [u8; 4] = [0xf3, 0xbd, 0xc2, 0x28];
const ZERO: Addr = [0; 20];
const TRANSFER: Word = topic("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
/// TetherToken `Issue(uint256)` and `Redeem(uint256)`: the owner-only issue
/// and redemption of the sender's own balance.
const ISSUE: Word = topic("cb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a");
const REDEEM: Word = topic("702d5967f45f6513a38ffc42d6ba9bf230bd40e8f53b16363c7eb4fd2deb9a44");
/// `DestroyedBlackFunds(address,uint256)`: TetherToken's, with nothing
/// indexed, and TrueCurrency's, with the holder indexed. Both zero a
/// blacklisted holder's balance and state the amount destroyed.
const DESTROYED_BLACK_FUNDS: Word = topic("61e6e66b0d6339b2980aecc6ccc0039736791f0ccde9ed512e789a7fbdd698c6");
/// WETH9 `Deposit(address indexed, uint256)` and
/// `Withdrawal(address indexed, uint256)`.
const DEPOSIT: Word = topic("e1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c");
const WITHDRAWAL: Word = topic("7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65");
/// WETH9 `deposit()` and `withdraw(uint256)`.
const DEPOSIT_CALL: [u8; 4] = [0xd0, 0xe3, 0x0d, 0xb0];
const WITHDRAW_CALL: [u8; 4] = [0x2e, 0x1a, 0x7d, 0x4d];
/// The gas a Solidity `transfer` forwards with a value: WETH9's payout.
const STIPEND: u64 = 2300;
/// WETH `Deposit`/`Withdrawal` and the `Mint`/`Burn(address indexed, uint256)`
/// of DSToken, WBTC, FiatToken and Tether: they name a holder whose balance
/// changes, but may also mean staking or locking, so they vote only when the
/// frame's own execution backs them (see [`backed`]).
const CREDITS_DEBITS: [Word; 4] = [
    DEPOSIT,
    WITHDRAWAL,
    topic("0f6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885"),
    topic("cc16f5dbb4873280815c1ee09dbd06736cffcc184412cf7a71a0fdb75d397ca5"),
];

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("lowercase hex expected"),
    }
}
const fn topic(hex: &str) -> Word {
    let b = hex.as_bytes();
    assert!(b.len() == 64);
    let mut out = [0; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(b[2 * i]) << 4) | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

/// Which addresses a matched reference event contributes as holders.
#[derive(Clone, Copy)]
enum Part {
    /// Only `tx.from` and the emitting contract.
    Context,
    Topic1,
    Topic2,
    Topics12,
    Word0,
    Words01,
    /// The msg.sender of the frame that emitted the log (see [`senders`]).
    Sender,
}
struct Shape {
    topic0: Word,
    topics: usize,
    data: usize,
    part: Part,
}
const fn shape(topic0: &str, topics: usize, data: usize, part: Part) -> Shape {
    Shape {
        topic0: topic(topic0),
        topics,
        data,
        part,
    }
}
/// The event shapes of the RPC reference (substreams-evm erc20-balances v0.3.4
/// with its erc20-transfers and erc20-tokens inputs): topic0, exact topic count
/// and exact data length, with no address filter. Since v0.7.4 (owner,
/// 2026-10-17) Tether `Issue`/`Redeem` name the emitting frame's sender, where
/// the reference called `owner()`: both are owner-only and move the sender's
/// balance. TrueCurrency's `DestroyedBlackFunds(address indexed,uint256)`, the
/// last shape, is not one of the reference's.
const SHAPES: [Shape; 65] = [
    shape("e1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c", 2, 32, Part::Topic1), // weth.Deposit
    shape("7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65", 2, 32, Part::Topic1), // weth.Withdrawal
    shape("ab8530f87dc9b59234c4623bf917212bb2536d647574c8e7e5da92c2ede0c9f8", 3, 32, Part::Topics12), // fiattokenv2_2.Mint
    shape("cc16f5dbb4873280815c1ee09dbd06736cffcc184412cf7a71a0fdb75d397ca5", 2, 32, Part::Topic1), // fiattokenv2_2.Burn
    shape("ffa4e6181777692565cf28528fc88fd1516ea86b56da075235fa575af6a4b855", 2, 0, Part::Topic1),  // fiattokenv2_2.Blacklisted
    shape("c67398012c111ce95ecb7429b933096c977380ee6c421175a71a4a4c6c88c06e", 2, 0, Part::Context), // fiattokenv2_2.BlacklisterChanged
    shape("db66dfa9c6b8f5226fe9aac7e51897ae8ee94ac31dc70bb6c9900b2574b707e6", 2, 0, Part::Context), // fiattokenv2_2.MasterMinterChanged
    shape("46980fca912ef9bcdbd36877427b6b90e860769f604e89c0e67720cece530d20", 2, 32, Part::Context), // fiattokenv2_2.MinterConfigured
    shape("e94479a9f7e1952cc78f2d6baab678adc1b772d936c6583def489e524cb66692", 2, 0, Part::Context), // fiattokenv2_2.MinterRemoved
    shape("8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0", 1, 64, Part::Words01), // fiattokenv2_2.OwnershipTransferred
    shape("6985a02210a168e66602d3235cb6db0e70f92b3ba4d376a33c0f3d9434bff625", 1, 0, Part::Context), // fiattokenv2_2.Pause
    shape("b80482a293ca2e013eda8683c9bd7fc8347cfdaeea5ede58cba46df502c2a604", 2, 0, Part::Context), // fiattokenv2_2.PauserChanged
    shape("e475e580d85111348e40d8ca33cfdd74c30fe1655c2d8537a13abc10065ffa5a", 2, 0, Part::Context), // fiattokenv2_2.RescuerChanged
    shape("117e3210bb9aa7d9baff172026820255c6f6c30ba8999d1c2fd88e2848137c4e", 2, 0, Part::Topic1),  // fiattokenv2_2.UnBlacklisted
    shape("7805862f689e2f13df9f062ff482ad3ad112aca9e0847911ed832e158c525b33", 1, 0, Part::Context), // fiattokenv2_2.Unpause
    shape("1cdd46ff242716cdaa72d159d339a485b3438398348d68f09d7c8c0a59353d81", 3, 0, Part::Context), // fiattokenv2_2.AuthorizationCanceled
    shape("98de503528ee59b575ef0c0a2576a82497bfc029a5685b209e9ec333479b10a5", 3, 0, Part::Context), // fiattokenv2_2.AuthorizationUsed
    shape("cb8241adb0c3fdb35b70c24ce35c5eb0c17af7431c99f827d44a445ca624176a", 1, 32, Part::Sender), // tethertoken.Issue
    shape("702d5967f45f6513a38ffc42d6ba9bf230bd40e8f53b16363c7eb4fd2deb9a44", 1, 32, Part::Sender), // tethertoken.Redeem
    shape("cc358699805e9a8b7f77b522628c7cb9abd07d9efb86b6fb616af1609036a99e", 1, 32, Part::Context), // tethertoken.Deprecate
    shape("b044a1e409eac5c48e5af22d4af52670dd1a99059537a78b31b48c6500a6354e", 1, 64, Part::Context), // tethertoken.Params
    shape("61e6e66b0d6339b2980aecc6ccc0039736791f0ccde9ed512e789a7fbdd698c6", 1, 64, Part::Word0),  // tethertoken.DestroyedBlackFunds
    shape("42e160154868087d6bfdc0ca23d96a1c1cfa32f1b72ba9ba27b69b98a0d819dc", 1, 32, Part::Word0),  // tethertoken.AddedBlackList
    shape("d7e9ec6e6ecd65492dce6bf513cd6867560d49544421d0783ddf06e76c24470c", 1, 32, Part::Word0),  // tethertoken.RemovedBlackList
    shape("406bbf2d8d145125adf1198d2cf8a67c66cc4bb0ab01c37dccd4f7c0aae1e7c7", 2, 0, Part::Topic1),  // tethertoken.BlockPlaced
    shape("665918c9e02eb2fd85acca3969cb054fc84c138e60ec4af22ab6ef2fd4c93c27", 2, 0, Part::Topic1),  // tethertoken.BlockReleased
    shape("0f6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885", 2, 32, Part::Topic1), // tethertoken.Mint
    shape("6a2859ae7902313752498feb80a014e6e7275fe964c79aa965db815db1c7f1e9", 2, 32, Part::Topic1), // tethertoken.DestroyedBlockedFunds
    shape("783cda63e44f9c07bbe4cf839a147c52931d0cd508a4930af9d1656a1201f036", 2, 0, Part::Context), // tethertoken.NewPrivilegedContract
    shape("dda50bf0570e969140ea1a415f2cd2636674ab5a19402b052918462b58c4297e", 2, 0, Part::Context), // tethertoken.RemovedPrivilegedContract
    shape("05d0634fe981be85c22e2942a880821b70095d84e152c3ea3c17a4e4250d9d61", 3, 32, Part::Topic2), // erc20swapasset.LogSwapin
    shape("6b616089d04950dc06c45c6dd787d657980543f89651aec47924752c7d16c888", 3, 32, Part::Topic1), // erc20swapasset.LogSwapout
    shape("e1968d4263a733e2597ef67ea6ad267343bba5f8bf0f99d85190e06b05d824d9", 4, 0, Part::Context), // erc20swapasset.LogChangeDcrmOwner
    shape("0f6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885", 2, 32, Part::Topic1), // wbtc.Mint
    shape("cc16f5dbb4873280815c1ee09dbd06736cffcc184412cf7a71a0fdb75d397ca5", 2, 32, Part::Topic1), // wbtc.Burn
    shape("ae5184fba832cb2b1f702aca6117b8d265eaf03ad33eb133f19dde0f5920fa08", 1, 0, Part::Context), // wbtc.MintFinished
    shape("f8df31144d9c2f0f6b59d69b8b98abd5459d07f2742c4df920b25aae33c64820", 2, 0, Part::Context), // wbtc.OwnershipRenounced
    shape("0f6798a560793a54c3bcfe86a93cde1e73087d944c0ea20544137d4121396885", 2, 32, Part::Topic1), // sai.Mint
    shape("cc16f5dbb4873280815c1ee09dbd06736cffcc184412cf7a71a0fdb75d397ca5", 2, 32, Part::Topic1), // sai.Burn
    shape("1abebea81bfa2637f28358c371278fb15ede7ea8dd28d2e03b112ff6d936ada4", 2, 0, Part::Context), // sai.LogSetAuthority
    shape("ce241d7ca1f669fee44b6fc00b8eba2df3bb514eed0f6f668f8f89096e81ed94", 2, 0, Part::Context), // sai.LogSetOwner
    shape("96a25c8ce0baabc1fdefd93e9ed25d8e092a3332f3aa9a41722b5697231d1d1a", 2, 64, Part::Topic1), // steth.Submitted
    shape("76a397bea5768d4fca97ef47792796e35f98dc81b16c1de84e28a818e1f97108", 1, 32, Part::Context), // steth.Unbuffered
    shape("ff08c3ef606d198e316ef5b822193c489965899eb4e3c248cea1a4626c3eda50", 2, 192, Part::Context), // steth.TokenRebased
    shape("9d9c909296d9c674451c0c24f02cb64981eb3b727f99865939192f880a755dcb", 3, 32, Part::Topics12), // steth.TransferShares
    shape("8b2a1e1ad5e0578c3dd82494156e985dade827a87c573b5c1c7716a32162ad64", 2, 96, Part::Topic1), // steth.SharesBurnt
    shape("ee473f96486a2f4b93ccb6729f121223e96975db6e0d6a5ef01f56477e3eab3b", 2, 32, Part::Topic1), // steth.ExternalSharesMinted
    shape("ad21467656c56eb8c99f7916faa12f7a657a04d8802100acec62e92451ac5606", 1, 32, Part::Sender), // steth.ExternalSharesBurnt
    shape("4ee34277c93491eeca655ad5c42ae1c193a5719e1c8837df9058af7696817cce", 1, 32, Part::Context), // steth.ExternalEtherTransferredToBuffer
    shape("4e80196ef1285462b2c4ee20f88e18e58c59e405eec6fd51f5fd1d614bc98a7f", 1, 32, Part::Context), // steth.ExternalBadDebtInternalized
    shape("13c514ee70ee403f89bdf5ab83908edba92a77e3a61e19b774670c0a2cb2d7e6", 1, 32, Part::Context), // steth.MaxExternalRatioBpSet
    shape("1252331d4f3ee8a9f0a3484c4c2fb059c70a047b5dc5482a3ee6415f742d9f2e", 2, 64, Part::Context), // steth.ClValidatorsUpdated
    shape("e0aacfc334457703148118055ec794ac17654c6f918d29638ba3b18003cee5ff", 1, 32, Part::Context), // steth.DepositedValidatorsChanged
    shape("92dd3cb149a1eebd51fd8c2a3653fd96f30c4ac01d4f850fc16d46abd6c3e92f", 2, 160, Part::Context), // steth.EthDistributed
    shape("af00d86be4cd299db16aa59803992e174fa88b67d81a0c7dd0148f9a75606a8d", 2, 96, Part::Context), // steth.InternalShareRateUpdated
    shape("26d1807b479eaba249c1214b82e4b65bbb0cc73ee8a17901324b1ef1b5904e49", 1, 0, Part::Context), // steth.StakingPaused
    shape("edaeeae9aed70c4545d3ab0065713261c9cee8d6cf5c8b07f52f0a65fd91efda", 1, 0, Part::Context), // steth.StakingResumed
    shape("ce9fddf6179affa1ea7bf36d80a6bf0284e0f3b91f4b2fa6eea2af923e7fac2d", 1, 64, Part::Context), // steth.StakingLimitSet
    shape("9b2a687c198898fcc32a33bbc610d478f177a73ab7352023e6cc1de5bf12a3df", 1, 0, Part::Context), // steth.StakingLimitRemoved
    shape("d27f9b0c98bdee27044afa149eadcd2047d6399cb6613a45c5b87e6aca76e6b5", 1, 32, Part::Context), // steth.ElRewardsReceived
    shape("6e5086f7e1ab04bd826e77faae35b1bcfe31bd144623361a40ea4af51670b1c3", 1, 32, Part::Context), // steth.WithdrawalsReceived
    shape("61f9416d3c29deb4e424342445a2b132738430becd9fa275e11297c90668b22e", 1, 32, Part::Context), // steth.LidoLocatorSet
    shape("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", 3, 32, Part::Topics12), // erc20.Transfer
    shape("8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925", 3, 32, Part::Topics12), // erc20.Approval
    shape("61e6e66b0d6339b2980aecc6ccc0039736791f0ccde9ed512e789a7fbdd698c6", 2, 32, Part::Topic1), // truecurrency.DestroyedBlackFunds
];

fn insert(out: &mut BTreeMap<Addr, BTreeSet<Addr>>, contract: &[u8], holder: &[u8]) {
    if let (Ok(contract), Ok(holder)) = (Addr::try_from(contract), Addr::try_from(holder)) {
        if contract != ZERO && holder != ZERO {
            out.entry(contract).or_default().insert(holder);
        }
    }
}
fn visit(out: &mut BTreeMap<Addr, BTreeSet<Addr>>, from: &[u8], log: &eth::Log, sender: Option<&[u8]>) {
    let Some(topic0) = log.topics.first() else {
        return;
    };
    let mut matched = false;
    for s in SHAPES.iter().filter(|s| s.topic0[..] == topic0[..]) {
        if log.topics.len() != s.topics || log.data.len() != s.data || log.topics[1..].iter().any(|t| t.len() < 32) {
            continue;
        }
        let topic = |i: usize| Some(&log.topics[i][12..32]);
        let word = |i: usize| Some(&log.data[i * 32 + 12..i * 32 + 32]);
        let parts = match s.part {
            Part::Sender => match sender {
                Some(sender) => [Some(sender), None],
                None => continue,
            },
            Part::Context => [None, None],
            Part::Topic1 => [topic(1), None],
            Part::Topic2 => [topic(2), None],
            Part::Topics12 => [topic(1), topic(2)],
            Part::Word0 => [word(0), None],
            Part::Words01 => [word(0), word(1)],
        };
        matched = true;
        for holder in parts.into_iter().flatten() {
            insert(out, &log.address, holder);
        }
    }
    if matched {
        insert(out, &log.address, from);
        insert(out, &log.address, &log.address);
    }
}
/// The RPC reference's `(contract, holder)` candidates for a block, by
/// contract. Successful transactions' logs from non-reverted calls (receipt
/// logs when a trace has no calls) that match an event shape of [`SHAPES`] add
/// its holders, `tx.from` and the contract; zero or non-20-byte addresses are
/// dropped. Tether `Issue`/`Redeem` and stETH `ExternalSharesBurnt` take the
/// emitting frame's msg.sender.
pub(crate) fn candidates(block: &eth::Block) -> BTreeMap<Addr, BTreeSet<Addr>> {
    let mut out = BTreeMap::new();
    for tx in block.transaction_traces.iter().filter(|tx| tx.status == SUCCEEDED) {
        if tx.calls.is_empty() {
            for log in tx.receipt.as_option().into_iter().flat_map(|r| &r.logs) {
                visit(&mut out, &tx.from, log, None);
            }
        }
        let senders = senders(tx);
        for call in tx.calls.iter().filter(|c| !c.state_reverted) {
            for log in &call.logs {
                visit(&mut out, &tx.from, log, senders.get(&call.index).copied());
            }
        }
    }
    out
}
/// The msg.sender of each call of a transaction, by call index: a
/// DELEGATECALL runs with its parent's sender, every other call with its own
/// caller (Firehose records a DELEGATECALL's caller as the proxy itself).
fn senders(tx: &eth::TransactionTrace) -> BTreeMap<u32, &[u8]> {
    let mut out = BTreeMap::new();
    for c in &tx.calls {
        let own = match out.get(&c.parent_index) {
            Some(parent) if c.call_type == DELEGATE => *parent,
            _ => &c.caller[..],
        };
        out.insert(c.index, own);
    }
    out
}

/// Exact signed sum of uint256 words, as wrapping 320-bit two's complement.
/// A block cannot hold 2^63 words, so equal residues are equal integers.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Net([u64; 5]);
fn limb(w: &Word, i: usize) -> u64 {
    if i < 4 {
        u64::from_be_bytes(w[24 - 8 * i..32 - 8 * i].try_into().unwrap())
    } else {
        0
    }
}
impl Net {
    fn add(&mut self, w: &Word) {
        self.merge(&Net([limb(w, 0), limb(w, 1), limb(w, 2), limb(w, 3), 0]));
    }
    fn sub(&mut self, w: &Word) {
        let mut borrow = false;
        for i in 0..5 {
            let (a, b1) = self.0[i].overflowing_sub(limb(w, i));
            let (b, b2) = a.overflowing_sub(u64::from(borrow));
            self.0[i] = b;
            borrow = b1 || b2;
        }
    }
    fn merge(&mut self, other: &Net) {
        let mut carry = false;
        for i in 0..5 {
            let (a, c1) = self.0[i].overflowing_add(other.0[i]);
            let (b, c2) = a.overflowing_add(u64::from(carry));
            self.0[i] = b;
            carry = c1 || c2;
        }
    }
    fn is_zero(&self) -> bool {
        self.0 == [0; 5]
    }
    /// The magnitude, little-endian limbs.
    fn abs(&self) -> [u64; 5] {
        if self.0[4] >> 63 == 0 {
            return self.0;
        }
        let mut out = [0; 5];
        let mut carry = true;
        for (o, l) in out.iter_mut().zip(self.0) {
            let (v, c) = (!l).overflowing_add(u64::from(carry));
            *o = v;
            carry = c;
        }
        out
    }
}
/// `|delta| * 1000 >= |flow|`: a mismatch too large for a counter.
fn large(flow: &Net, delta: &Net) -> bool {
    let (flow, delta) = (flow.abs(), delta.abs());
    let mut scaled = [0u64; 5];
    let mut carry = 0u128;
    for (s, d) in scaled.iter_mut().zip(delta) {
        let p = u128::from(d) * 1000 + carry;
        *s = p as u64;
        carry = p >> 64;
    }
    carry > 0 || scaled.iter().rev().cmp(flow.iter().rev()).is_ge()
}

fn word(bytes: &[u8]) -> Option<Word> {
    let mut out = [0; 32];
    out.get_mut(32usize.checked_sub(bytes.len())?..)?.copy_from_slice(bytes);
    Some(out)
}
/// The native value a call moves, as a number: an absent value is zero, and
/// leading zero bytes do not count. `None` above 2^256.
fn value(call: &eth::Call) -> Option<Word> {
    let bytes = call.value.as_option().map_or(&[][..], |v| &v.bytes[..]);
    word(&bytes[bytes.iter().take_while(|b| **b == 0).count()..])
}
fn topic_address(t: &[u8]) -> Option<Addr> {
    (t.len() == 32 && t[..12] == [0; 12]).then(|| t[12..].try_into().unwrap())
}
/// The verified 64-byte Keccak preimage of `key` in a frame's hex
/// `hash -> preimage` map. Only 128-character values can hold 64 bytes, so
/// longer recorded preimages are never decoded.
fn preimage64(call: &eth::Call, key: &Word) -> Option<[u8; 64]> {
    let value = call.keccak_preimages.get(&hex::encode(key)).filter(|v| v.len() == 128)?;
    let bytes: [u8; 64] = hex::decode(value).ok()?.try_into().ok()?;
    (hash(&bytes) == *key).then_some(bytes)
}
/// The verified same-frame preimage `pad(holder) || base` of a storage key.
fn preimage(call: &eth::Call, key: &Word) -> Option<(Addr, Word)> {
    let bytes = preimage64(call, key)?;
    (bytes[..12] == [0; 12]).then(|| (bytes[12..32].try_into().unwrap(), bytes[32..].try_into().unwrap()))
}

struct Write {
    frame: Frame,
    key: Word,
    old: Word,
    new: Word,
    ordinal: u64,
}
/// Persisted storage writes of candidate contracts with their frame, from the
/// shared persistence rules.
struct Persisted<'a> {
    tokens: &'a BTreeMap<Addr, BTreeSet<Addr>>,
    writes: BTreeMap<Addr, Vec<Write>>,
    doubt: BTreeSet<Addr>,
}
impl Persisted<'_> {
    fn token(&self, address: &[u8]) -> Option<Addr> {
        Addr::try_from(address).ok().filter(|a| self.tokens.contains_key(a))
    }
}
impl persist::Sink for Persisted<'_> {
    fn storage(&mut self, c: &eth::StorageChange, ctx: persist::Ctx) {
        let Some(address) = self.token(&c.address) else {
            return;
        };
        match (word(&c.key), word(&c.old_value), word(&c.new_value)) {
            (Some(key), Some(old), Some(new)) if c.ordinal > 0 => self.writes.entry(address).or_default().push(Write {
                frame: (ctx.scope == persist::Scope::SystemCall, ctx.tx_index, ctx.call_index),
                key,
                old,
                new,
                ordinal: c.ordinal,
            }),
            _ => {
                self.doubt.insert(address);
            }
        }
    }
    fn balance(&mut self, _: &eth::BalanceChange, _: persist::Ctx) {}
    fn nonce(&mut self, _: &eth::NonceChange, _: persist::Ctx) {}
    /// Removed code: a contract created and destroyed in one transaction.
    fn code(&mut self, c: &eth::CodeChange, _: persist::Ctx) {
        if let Some(address) = self.token(&c.address).filter(|_| c.new_code.is_empty()) {
            self.doubt.insert(address);
        }
    }
}

/// A Tether admin event: the holder whose word it moves, its signed amount,
/// and whether the word must end at zero. `Issue`/`Redeem` move the emitting
/// frame's sender; `DestroyedBlackFunds` zeroes the holder it names (data word
/// 0 for TetherToken, topic 1 for TrueCurrency). An amount of zero, a
/// malformed holder or the zero address gives none.
fn admin(log: &eth::Log, t0: &[u8], sender: impl FnOnce() -> Option<Addr>) -> Option<(Addr, Net, bool)> {
    let (topics, data) = (log.topics.len(), &log.data[..]);
    let (holder, amount, debit, zero) = match (topics, data.len()) {
        (1, 32) if t0 == ISSUE => (sender()?, data, false, false),
        (1, 32) if t0 == REDEEM => (sender()?, data, true, false),
        (1, 64) if t0 == DESTROYED_BLACK_FUNDS => (topic_address(&data[..32])?, &data[32..], true, true),
        (2, 32) if t0 == DESTROYED_BLACK_FUNDS => (topic_address(&log.topics[1])?, data, true, true),
        _ => return None,
    };
    let amount = Word::try_from(amount).ok().filter(|a| *a != [0; 32])?;
    let mut net = Net::default();
    if debit {
        net.sub(&amount);
    } else {
        net.add(&amount);
    }
    (holder != ZERO).then_some((holder, net, zero))
}

/// Net `Transfer` flows per frame of contracts with persisted writes, and the
/// holders that their balance events name or that a Tether admin event may
/// explain. Self-destructing contracts and malformed event addresses are
/// doubted.
fn flows<'a>(block: &'a eth::Block, writes: &BTreeMap<Addr, Vec<Write>>, doubt: &mut BTreeSet<Addr>) -> (Flows<'a>, Named) {
    let mut flows = Flows::new();
    let mut named = Named::new();
    for tx in block.transaction_traces.iter().filter(|tx| tx.status == SUCCEEDED) {
        // Resolved only for a transaction with an `Issue` or `Redeem`.
        let mut senders_of_tx = None;
        for call in tx.calls.iter().filter(|c| !c.state_reverted) {
            if call.suicide {
                // SELFDESTRUCT acts on the storage context: a DELEGATECALL's caller.
                let context = if call.call_type == DELEGATE { &call.caller } else { &call.address };
                if let Ok(address) = Addr::try_from(context.as_slice()) {
                    doubt.insert(address);
                }
            }
            let frame = (false, tx.index, call.index);
            let mut nets = BTreeMap::<Addr, BTreeMap<Addr, Net>>::new();
            for log in &call.logs {
                let Some(token) = Addr::try_from(log.address.as_slice()).ok().filter(|t| writes.contains_key(t)) else {
                    continue;
                };
                let Some(t0) = log.topics.first() else {
                    continue;
                };
                let sender = || Addr::try_from(*senders_of_tx.get_or_insert_with(|| senders(tx)).get(&call.index)?).ok();
                if let Some((holder, amount, zero)) = admin(log, t0, sender) {
                    let entry = named.entry(token).or_default().admin.entry((frame, holder)).or_default();
                    entry.0.merge(&amount);
                    entry.1 |= zero;
                    continue;
                }
                let Ok(value) = Word::try_from(log.data.as_slice()) else {
                    continue;
                };
                let transfer = log.topics.len() == 3 && t0[..] == TRANSFER;
                let credit_debit = log.topics.len() == 2 && CREDITS_DEBITS.iter().any(|t| t[..] == t0[..]);
                if !(transfer || credit_debit) {
                    continue;
                }
                let Some(holders) = log.topics[1..].iter().map(|t| topic_address(t)).collect::<Option<Vec<_>>>() else {
                    doubt.insert(token);
                    continue;
                };
                named.entry(token).or_default().named.extend(holders.iter().map(|h| (frame, *h)));
                if transfer && holders[0] != holders[1] {
                    let net = nets.entry(token).or_default();
                    if holders[0] != ZERO {
                        net.entry(holders[0]).or_default().sub(&value);
                    }
                    if holders[1] != ZERO {
                        net.entry(holders[1]).or_default().add(&value);
                    }
                }
            }
            for (token, mut net) in nets {
                net.retain(|_, n| !n.is_zero());
                if !net.is_empty() {
                    flows.insert((frame, token), (call, net));
                }
            }
        }
    }
    (flows, named)
}

/// Votes for one mapping base of one contract in one block.
#[derive(Clone, Copy, Default)]
struct Tally {
    /// Holder sides whose net persisted delta equals their net event flow;
    /// flows below 10^6 count only while the base has no mismatch.
    exact: u32,
    /// Holder sides with a nonzero flow and a different delta.
    mismatch: u32,
    /// Mismatches whose delta is at least a thousandth of the flow.
    large: u32,
    /// Exact votes for flows below 10^6.
    dust: u32,
    /// The base has its own verified 64-byte preimage: an inner mapping.
    nested: bool,
}
/// The net persisted delta and last new word of each key, per frame and
/// contract. Writes are in execution order.
fn framed(writes: &BTreeMap<Addr, Vec<Write>>) -> Framed {
    let mut framed = Framed::new();
    for (token, ws) in writes {
        for w in ws {
            let (d, last) = framed.entry((w.frame, *token)).or_default().entry(w.key).or_default();
            d.add(&w.new);
            d.sub(&w.old);
            *last = w.new;
        }
    }
    framed
}

/// Frames that vote in a block where their contract has no `Transfer` flow:
/// a CALL to the contract, neither failed nor reverted, whose only log of the
/// contract states an amount that the frame's own execution backs. These are
/// a WETH9 deposit or withdrawal ([`wrapped`]) and a TetherToken destruction
/// ([`destruction`]). Such a frame votes like a `Transfer` frame: the amount
/// is compared with the net change of the holder's word. A block where the
/// contract has a `Transfer` flow keeps only its `Transfer` votes, so a
/// backed frame never outvotes or joins them.
fn backed<'a>(block: &'a eth::Block, framed: &Framed, flows: &mut Flows<'a>) {
    let voted: BTreeSet<Addr> = flows.keys().map(|(_, token)| *token).collect();
    for tx in block.transaction_traces.iter().filter(|tx| tx.status == SUCCEEDED) {
        let mut children = None;
        for call in tx.calls.iter().filter(|c| !c.state_reverted && !c.status_failed && c.call_type == CALL) {
            let frame = (false, tx.index, call.index);
            let Some(token) = Addr::try_from(call.address.as_slice()).ok().filter(|t| !voted.contains(t)) else {
                continue;
            };
            let Some(keys) = framed.get(&(frame, token)) else {
                continue;
            };
            let mut logs = call.logs.iter().filter(|l| l.address[..] == token[..]);
            let (Some(log), None) = (logs.next(), logs.next()) else {
                continue;
            };
            // Every direct child, reverted or not, counts.
            let children = children.get_or_insert_with(|| {
                let mut n = BTreeMap::<u32, Vec<&eth::Call>>::new();
                for c in tx.calls.iter().filter(|c| c.depth > 0) {
                    n.entry(c.parent_index).or_default().push(c);
                }
                n
            });
            let children = children.get(&call.index).map(Vec::as_slice).unwrap_or_default();
            if let Some((holder, amount)) = destruction(call, log, keys, children).or_else(|| wrapped(call, log, keys, children)) {
                flows.insert((frame, token), (call, BTreeMap::from([(holder, amount)])));
            }
        }
    }
}
/// A TetherToken `destroyBlackFunds(holder)` frame: it makes no call, its log
/// is the 1-topic `DestroyedBlackFunds(holder, amount)`, its input names the
/// holder, and its persisted writes of the contract are two keys: the
/// holder's word, whose same-frame preimage is `pad(holder) || base` and which
/// ends at zero, and one word without a 64-byte preimage (the total supply)
/// that falls by the amount. It votes the debit of the amount from the
/// holder. `Issue`/`Redeem` never vote: a deprecated TetherToken still issues
/// to its owner after `balanceOf` moved to the upgraded contract.
fn destruction(call: &eth::Call, log: &eth::Log, keys: &BTreeMap<Word, (Net, Word)>, children: &[&eth::Call]) -> Option<(Addr, Net)> {
    if keys.len() != 2 || log.topics.len() != 1 || log.topics[0][..] != DESTROYED_BLACK_FUNDS || !children.is_empty() {
        return None;
    }
    let (holder, amount, _) = admin(log, &log.topics[0], || None)?;
    let input = &call.input;
    if input.len() < 36 || input[..4] != DESTROY_BLACK_FUNDS || input[4..16] != [0; 12] || input[16..36] != holder {
        return None;
    }
    let backs = |(key, (delta, last)): (&Word, &(Net, Word))| match preimage(call, key) {
        Some((h, _)) => h == holder && *last == [0; 32],
        None => preimage64(call, key).is_none() && *delta == amount,
    };
    let mut owners = keys.keys().filter_map(|key| preimage(call, key)).map(|(h, _)| h);
    (keys.iter().all(backs) && owners.next() == Some(holder) && owners.next().is_none()).then_some((holder, amount))
}
/// A WETH9 deposit or withdrawal frame, called by its holder, whose log is
/// `Deposit(holder, wad)` or `Withdrawal(holder, wad)` with a nonzero wad, and
/// whose persisted writes of the contract are one key with the same-frame
/// preimage `pad(holder) || base`.
///
/// - **Deposit:** its input is empty (the fallback) or starts with
///   `deposit()`, it receives exactly wad in native value, and it makes no
///   call. It votes a credit of wad.
/// - **Withdrawal:** its input starts with `withdraw(wad)`, it receives no
///   value, and its only call pays exactly wad to the holder: a CALL with no
///   input and the 2,300 gas of a Solidity `transfer`, neither failed nor
///   reverted. It votes a debit of wad.
///
/// Wallets and multisigs append bytes to the call data (a tag, a padded
/// word), which WETH9's dispatcher ignores. On 2,238 live WETH and WBNB
/// frames of 124 Ethereum and BSC blocks the fingerprint rejects one genuine
/// deposit: a fallback deposit with 68 bytes of call data.
fn wrapped(call: &eth::Call, log: &eth::Log, keys: &BTreeMap<Word, (Net, Word)>, children: &[&eth::Call]) -> Option<(Addr, Net)> {
    let [t0, t1] = log.topics.as_slice() else {
        return None;
    };
    let (deposit, withdrawal) = (t0[..] == DEPOSIT, t0[..] == WITHDRAWAL);
    if !(deposit || withdrawal) || keys.len() != 1 {
        return None;
    }
    let holder = topic_address(t1).filter(|h| *h != ZERO && call.caller[..] == h[..])?;
    let wad = Word::try_from(log.data.as_slice()).ok().filter(|w| *w != [0; 32])?;
    let key = keys.keys().next()?;
    if preimage(call, key).is_none_or(|(h, _)| h != holder) {
        return None;
    }
    let input = &call.input;
    let mut net = Net::default();
    if deposit {
        if !(input.is_empty() || input.starts_with(&DEPOSIT_CALL)) || value(call) != Some(wad) || !children.is_empty() {
            return None;
        }
        net.add(&wad);
    } else {
        let pays = |c: &eth::Call| {
            c.call_type == CALL
                && c.address[..] == holder[..]
                && value(c) == Some(wad)
                && c.input.is_empty()
                && c.gas_limit == STIPEND
                && !c.status_failed
                && !c.state_reverted
        };
        let [payout] = children else {
            return None;
        };
        if input.len() < 36 || input[..4] != WITHDRAW_CALL || input[4..36] != wad || value(call) != Some([0; 32]) || !pays(payout) {
            return None;
        }
        net.sub(&wad);
    }
    Some((holder, net))
}

fn votes(flows: &Flows, framed: &Framed) -> BTreeMap<Addr, BTreeMap<Word, Tally>> {
    let mut tallies = BTreeMap::<Addr, BTreeMap<Word, Tally>>::new();
    for (key, (call, net)) in flows {
        // Each distinct key, and each base, is resolved once per frame.
        let mut deltas = BTreeMap::<(Addr, Word), Net>::new();
        for (k, (d, _)) in framed.get(key).into_iter().flatten() {
            if let Some(resolved) = preimage(call, k) {
                deltas.entry(resolved).or_default().merge(d);
            }
        }
        let mut nested = BTreeMap::<Word, bool>::new();
        let t = tallies.entry(key.1).or_default();
        for ((holder, base), delta) in deltas {
            let Some(flow) = net.get(&holder) else {
                continue;
            };
            let tally = t.entry(base).or_default();
            tally.nested |= *nested.entry(base).or_insert_with(|| preimage64(call, &base).is_some());
            if *flow == delta {
                tally.exact += 1;
                let f = flow.abs();
                tally.dust += u32::from(f[1..] == [0; 4] && f[0] < 1_000_000);
            } else {
                tally.mismatch += 1;
                tally.large += u32::from(large(flow, &delta));
            }
        }
    }
    // A small exact match cannot tell a stored balance from a scaled one.
    for tally in tallies.values_mut().flat_map(|t| t.values_mut()).filter(|t| t.mismatch > 0) {
        tally.exact -= tally.dust;
    }
    tallies
}
/// The base every keyed `balanceOf` reads, if it has exact votes, or else
/// the base with the most exact votes; a tie chooses none. Inner mappings
/// (allowances) and bases with more large mismatches than exact votes (a
/// packed word whose other field moves) are never chosen.
fn choose(tallies: &BTreeMap<Word, Tally>, reads: &[Read]) -> Option<Word> {
    let plain = |t: &Tally| t.exact > 0 && !t.nested;
    let base = 'base: {
        let mut keyed = reads.iter().filter(|r| r.keyed());
        if let Some(base) = keyed.clone().next().and_then(|r| r.bases.first()) {
            if keyed.all(|r| r.pure(base)) && tallies.get(base).is_some_and(plain) {
                break 'base *base;
            }
        }
        let mut best: Option<(Word, u32)> = None;
        let mut tied = false;
        for (base, t) in tallies.iter().filter(|(_, t)| plain(t)) {
            match best {
                Some((_, n)) if n > t.exact => {}
                Some((_, n)) if n == t.exact => tied = true,
                _ => {
                    best = Some((*base, t.exact));
                    tied = false;
                }
            }
        }
        best.filter(|_| !tied)?.0
    };
    (tallies[&base].large <= tallies[&base].exact).then_some(base)
}

/// A traced `balanceOf(holder)` call to a contract.
struct Read {
    holder: Addr,
    begin: u64,
    end: u64,
    value: Word,
    /// In a non-reverted frame: the value reflects persisted state.
    persisted: bool,
    /// Up to two bases of the verified `pad(holder) || base` preimages
    /// computed in the read's storage context (the frame and its DELEGATECALL
    /// descendants).
    bases: Vec<Word>,
    /// A call out of that storage context.
    external: bool,
}
impl Read {
    fn scan(&mut self, call: &eth::Call) {
        let holder = hex::encode(self.holder);
        for (key, value) in &call.keccak_preimages {
            // Match the hex text of `pad(holder)` before decoding and hashing.
            let v = value.as_bytes();
            if self.bases.len() == 2 || v.len() != 128 || v[..24].iter().any(|c| *c != b'0') || !v[24..64].eq_ignore_ascii_case(holder.as_bytes()) {
                continue;
            }
            let (Ok(bytes), Ok(key)) = (hex::decode(v), hex::decode(key)) else {
                continue;
            };
            let base: Word = bytes[32..].try_into().unwrap();
            if hash(&bytes)[..] == key[..] && !self.bases.contains(&base) {
                self.bases.push(base);
            }
        }
    }
    /// The read computed exactly `pad(holder) || base` and called nothing else.
    fn pure(&self, base: &Word) -> bool {
        self.bases == [*base] && !self.external
    }
    /// The read recorded a `pad(holder) || base` preimage. An unkeyed read
    /// shows neither which mapping it read nor that it read only one.
    fn keyed(&self) -> bool {
        !self.bases.is_empty()
    }
}
/// A traced `balanceOf(address)`: a completed CALL or STATICCALL with
/// selector `0x70a08231`, more input than the selector and a 32-byte return,
/// with the holder of its zero-extended first argument word, as the ABI
/// decoder reads it. The read is clean when its input holds the whole word and
/// the word is a padded address (`input[4..16]` zero); bytes after the word
/// are ignored, as the decoder ignores them. A shorter input, or a longer one
/// with nonzero upper bytes, can succeed only on a token compiled before
/// Solidity 0.5 (no calldata size check, address arguments masked): it is
/// read only to veto. A 36-byte input with nonzero upper bytes stays ignored,
/// as before v0.7.4.
fn balance_of(call: &eth::Call) -> Option<(Addr, bool)> {
    let input = &call.input;
    if call.status_failed
        || !(call.call_type == CALL || call.call_type == STATIC)
        || input.len() <= 4
        || input[..4] != BALANCE_OF
        || call.return_data.len() != 32
        || call.begin_ordinal == 0
    {
        return None;
    }
    let mut argument = [0; 32];
    let n = (input.len() - 4).min(32);
    argument[..n].copy_from_slice(&input[4..4 + n]);
    let clean = input.len() >= 36 && argument[..12] == [0; 12];
    (clean || input.len() != 36).then(|| (argument[12..].try_into().unwrap(), clean))
}
/// Traced `balanceOf(address)` calls of voted contracts (see [`balance_of`])
/// in successful transactions. Only a clean read's storage context is scanned
/// for preimages, so only a clean read can be keyed.
fn reads<T>(block: &eth::Block, tokens: &BTreeMap<Addr, T>) -> BTreeMap<Addr, Vec<Read>> {
    let mut out = BTreeMap::<Addr, Vec<Read>>::new();
    let token = |c: &eth::Call| Addr::try_from(c.address.as_slice()).ok().filter(|t| tokens.contains_key(t));
    for tx in block.transaction_traces.iter().filter(|tx| tx.status == SUCCEEDED) {
        if !tx.calls.iter().any(|c| balance_of(c).is_some() && token(c).is_some()) {
            continue;
        }
        // One pass assigns every call its storage context (itself, or its
        // parent's for a DELEGATECALL) and groups each context's DELEGATECALL
        // members and the other calls made from it.
        let mut context = BTreeMap::<u32, u32>::new();
        let (mut members, mut outside) = (BTreeMap::<u32, Vec<&eth::Call>>::new(), BTreeMap::<u32, Vec<&eth::Call>>::new());
        for c in &tx.calls {
            let parent = context.get(&c.parent_index).copied();
            let own = match parent {
                Some(p) if c.call_type == DELEGATE => {
                    members.entry(p).or_default().push(c);
                    p
                }
                _ => {
                    if let Some(p) = parent {
                        outside.entry(p).or_default().push(c);
                    }
                    c.index
                }
            };
            context.insert(c.index, own);
        }
        for call in &tx.calls {
            let (Some((holder, clean)), Some(token)) = (balance_of(call), token(call)) else {
                continue;
            };
            let mut read = Read {
                holder,
                begin: call.begin_ordinal,
                end: call.end_ordinal,
                value: call.return_data[..].try_into().unwrap(),
                persisted: !call.state_reverted,
                bases: Vec::new(),
                external: false,
            };
            let members = members.get(&call.index).map(Vec::as_slice).unwrap_or_default();
            if clean {
                read.scan(call);
                for c in members {
                    read.scan(c);
                }
            }
            // A call out of the storage context counts unless it only returns
            // the address that its parent then DELEGATECALLs: a beacon lookup.
            let delegates: BTreeSet<_> = members.iter().map(|c| (c.parent_index, c.address.as_slice())).collect();
            read.external = outside.get(&call.index).into_iter().flatten().any(|c| {
                let resolved = (c.return_data.len() == 32 && c.return_data[..12] == [0; 12]).then(|| &c.return_data[12..]);
                resolved.is_none_or(|a| !delegates.contains(&(c.parent_index, a)))
            });
            out.entry(token).or_default().push(read);
        }
    }
    out
}
/// Producers on firehose-tracer 5.5.0 (`call_data_limit.rs` at `70497b7`),
/// which always record `Block.ver` 5, keep only the 4-byte selector of a
/// transaction's later internal calls once those calls' input passes 50 MiB,
/// and leave their return data empty past 25 MiB. A block whose encoding
/// passes 1 GiB is then cut again with both limits halved, until it fits. A
/// cut is therefore possible only in a `Block.ver` 5 transaction whose
/// internal calls (failed, reverted and cut ones included) record more than
/// 25 MiB of input or 12.5 MiB of return data, the limits of the first
/// halving, or in a block of at least 64 MiB or 150M gas, which a later
/// halving needs. Re-review these bounds on every tracer release.
const CUT_TX_INPUT: usize = 25 << 20;
const CUT_TX_RETURN: usize = 25 << 19;
const CUT_BLOCK_LEN: usize = 64 << 20;
const CUT_BLOCK_GAS: u64 = 150_000_000;
/// Whether a producer could have cut call data in `tx`; `block_len` is the
/// length of the block's delivered encoding.
fn cut_possible(block: &eth::Block, block_len: usize, tx: &eth::TransactionTrace) -> bool {
    if block.ver < 5 {
        return false;
    }
    let gas = block.header.as_option().map_or(0, |h| h.gas_used);
    if block_len >= CUT_BLOCK_LEN || gas >= CUT_BLOCK_GAS {
        return true;
    }
    let internal = tx.calls.iter().filter(|c| c.depth > 0);
    let (input, output) = internal.fold((0, 0), |(i, r), c| (i + c.input.len(), r + c.return_data.len()));
    input > CUT_TX_INPUT || output > CUT_TX_RETURN
}
/// Voted contracts with a successful CALL or STATICCALL of `balanceOf` that
/// keeps only its 4-byte selector or has no return data, the shape a cut
/// leaves, in a successful transaction where a producer could have cut it
/// ([`cut_possible`]): it may be a read whose veto was lost. Elsewhere such a
/// call is neither a read nor a doubt: a 4-byte call that a token compiled
/// before Solidity 0.5 answers with `balanceOf(0)`, or a call that returned
/// nothing.
fn truncated<T>(block: &eth::Block, block_len: usize, tokens: &BTreeMap<Addr, T>) -> BTreeSet<Addr> {
    let mut out = BTreeSet::new();
    for tx in block.transaction_traces.iter().filter(|tx| tx.status == SUCCEEDED) {
        let mut possible = None;
        for call in &tx.calls {
            let shape = !call.status_failed
                && (call.call_type == CALL || call.call_type == STATIC)
                && call.input.starts_with(&BALANCE_OF)
                && (call.input.len() == 4 || call.return_data.is_empty());
            let Some(token) = Addr::try_from(call.address.as_slice()).ok().filter(|a| shape && tokens.contains_key(a)) else {
                continue;
            };
            if *possible.get_or_insert_with(|| cut_possible(block, block_len, tx)) {
                out.insert(token);
            }
        }
    }
    out
}

/// The end-of-block balances of one voted contract, or `None` on any doubt.
fn decide(
    token: &Addr,
    tallies: &BTreeMap<Word, Tally>,
    writes: &[Write],
    reads: &[Read],
    holders: &BTreeSet<Addr>,
    names: &Names,
    framed: &Framed,
) -> Option<BTreeMap<Addr, Word>> {
    let base = choose(tallies, reads)?;
    // A Tether admin event explains a holder's write in its frame when the
    // frame's net change of the holder's word equals the amount it states, and
    // a destruction also leaves the word at zero.
    let explained = |frame: Frame, holder: &Addr| {
        names.admin.get(&(frame, *holder)).is_some_and(|(amount, zero)| {
            let written = framed.get(&(frame, *token)).and_then(|keys| keys.get(&mapping(holder, &base)));
            written.is_some_and(|(delta, last)| delta == amount && (!zero || *last == [0; 32]))
        })
    };
    let mut by_key = BTreeMap::<Word, Vec<&Write>>::new();
    for w in writes {
        by_key.entry(w.key).or_default().push(w);
    }
    // Persisted writes to one key must chain in execution order.
    if by_key
        .values()
        .any(|ws| ws.windows(2).any(|p| p[1].ordinal <= p[0].ordinal || p[1].old != p[0].new))
    {
        return None;
    }
    // An unkeyed read still vetoes when it calls out or contradicts the stored
    // word (below), but it never vouches for its holder or supplies a value.
    if reads.iter().any(|r| if r.keyed() { !r.pure(&base) } else { r.external }) {
        return None;
    }
    // Holders with a persisted keyed read, and the latest such read's begin.
    let mut read = BTreeMap::<Addr, u64>::new();
    for r in reads.iter().filter(|r| r.persisted && r.keyed()) {
        let begin = read.entry(r.holder).or_default();
        *begin = r.begin.max(*begin);
    }
    // Other holder mappings that move with the event flows suggest a computed
    // balance. With two or more exact votes and no other exact mapping,
    // counter-sized mismatches are ignored.
    let exact = tallies[&base].exact;
    let others = || tallies.iter().filter(|(b, t)| **b != base && !t.nested).map(|(_, t)| t);
    let large_only = exact >= 2 && others().all(|t| t.exact == 0);
    let shadow: u32 = others().map(|t| if large_only { t.large } else { t.mismatch }).sum();
    // Then a pure `balanceOf` vouches only for its own holder's code path.
    let shadowed = shadow >= exact;
    let mut values = BTreeMap::new();
    for r in reads.iter().filter(|r| r.persisted) {
        match by_key.get(&mapping(&r.holder, &base)) {
            Some(ws) => {
                // Writes to the key are in increasing ordinal order.
                if ws.get(ws.partition_point(|w| w.ordinal <= r.begin)).is_some_and(|w| w.ordinal < r.end) {
                    continue;
                }
                let stored = ws[..ws.partition_point(|w| w.ordinal < r.begin)]
                    .last()
                    .map(|w| w.new)
                    .or_else(|| ws.get(ws.partition_point(|w| w.ordinal <= r.end)).map(|w| w.old));
                if stored.is_some_and(|s| s != r.value) {
                    return None;
                }
            }
            // The holder's mapping word has no persisted write in the block.
            // Keep the mutating `values.insert` out of the match guard.
            #[allow(clippy::collapsible_match)]
            None if holders.contains(&r.holder) && r.keyed() => {
                if values.insert(r.holder, r.value).is_some_and(|v| v != r.value) {
                    return None;
                }
            }
            None => {}
        }
    }
    for holder in holders {
        if let Some(last) = by_key.get(&mapping(holder, &base)).and_then(|ws| ws.last()) {
            // A third party's word changed with no balance event naming or
            // explaining it in that frame (a flag, an admin or a sync write)
            // needs its own read, traced after that write: a read before it
            // validated only the word it saw.
            let vouched = read.get(holder).is_some_and(|begin| *begin > last.ordinal);
            if holder == token || names.named.contains(&(last.frame, *holder)) || explained(last.frame, holder) || vouched {
                values.insert(*holder, last.new);
            }
        }
    }
    // A read at any point shows its holder's code path; after an unexplained
    // write, only a later read kept the holder above.
    if shadowed {
        values.retain(|holder, _| read.contains_key(holder));
    }
    Some(values)
}

/// Inferred end-of-block balances, by `(contract, holder)`. `block_len` is
/// the length of the block's delivered encoding.
pub(crate) fn rows(block: &eth::Block, block_len: usize) -> BTreeMap<(Addr, Addr), Word> {
    let mut out = BTreeMap::new();
    let candidates = candidates(block);
    if candidates.is_empty() {
        return out;
    }
    let mut persisted = Persisted {
        tokens: &candidates,
        writes: BTreeMap::new(),
        doubt: BTreeSet::new(),
    };
    if persist::collect_block(block, &mut persisted).is_err() {
        return out;
    }
    let Persisted { mut writes, mut doubt, .. } = persisted;
    for ws in writes.values_mut() {
        ws.sort_by_key(|w| w.ordinal);
    }
    let framed = framed(&writes);
    let (mut flows, named) = flows(block, &writes, &mut doubt);
    backed(block, &framed, &mut flows);
    let tallies = votes(&flows, &framed);
    let reads = reads(block, &tallies);
    doubt.extend(truncated(block, block_len, &tallies));
    let nobody = Names::default();
    // A contract has tallies only if a nonzero `Transfer` moved a holder or,
    // in a block without one, a backed frame voted.
    for (token, t) in tallies.iter().filter(|(token, _)| !doubt.contains(*token)) {
        let token_reads = reads.get(token).map(Vec::as_slice).unwrap_or_default();
        let names = named.get(token).unwrap_or(&nobody);
        if let Some(values) = decide(token, t, &writes[token], token_reads, &candidates[token], names, &framed) {
            // A balance word at or above 2^128 holds another field next to the
            // balance (a packed timestamp, FiatToken v2.2's blacklist flag in
            // bit 255) that did not move in the block, so the slot is not a
            // plain balance: drop the token's rows for the block.
            if values.values().any(|value| value[..16] != [0; 16]) {
                continue;
            }
            out.extend(values.into_iter().map(|(holder, value)| ((*token, holder), value)));
        }
    }
    out
}
