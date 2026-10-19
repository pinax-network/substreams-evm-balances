# Top tokens

A closer look at USDT, USDC and WETH on Ethereum, and at the wrapped native
token of every chain with a backfilled native database: WETH, WBNB, WPOL and
WHYPE. BSC's USDT, USDC and WBNB balances follow when the BSC ERC-20 backfill
completes ([Deployment status](deployment-status.md#bsc-erc-20-v074)).

The new databases are `mainnet:evm-balances-erc20@v0.7.4` (cutoff
26,214,999, 2026-10-17 21:35 UTC) and the native databases of the
[native page](quality-native.md). The reference is the RPC-era
`<chain>:evm-balances@v0.3.3`. The queries ran on 2026-10-19 and were
read-only, with no RPC or Firehose call. Each analysis was rechecked by a
second pass, and this page gives the corrected results.
[Methods](quality-methods.md) has the SQL.

## Summary

| Token | Comparable holders | Non-zero match | Value match | Top 1,000 | Supply | Wrong in the new database |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| USDT | 78,272,145 | 99.990763% | 99.997905% | 1,000 | new total = supply minus the zero address, to the unit | zero address (by design), 3 dust holders unresolved |
| USDC | 36,888,869 | 99.990900% | 99.992018% | 1,000 | new total = event-log supply + 0.2 USDC | 1 holder, 0.2 USDC |
| WETH | 6,648,512 | 99.9919% | 99.99981% | 999 | contract's ETH − holders = zero address + 0.488 WETH + about 0.001 | zero address (by design), 12 holders, 0.488 WETH |

- **USDT, USDC and WETH can be served from the new database**, top holders
  included. Each has at most a dozen wrong holders, and those have small
  balances. Almost every other difference is an RPC-era error.
- **The zero address is never a holder in the new database.** USDT and WETH
  credit transfers to `0x0`, so it holds 39,902.54 USDT and 1,088.92 WETH
  there. The Token API should either take that one row from elsewhere or
  document it.
- **The supplies close.** The sum of the new balances equals each token's
  independent supply, less the zero address and the listed errors. The
  RPC-era's sum over-counts USDT by 901,603.62 USDT.
- **History is better in the new database.** The RPC-era backfill kept 0.1%
  to 1.2% of an active holder's changes, so its historical top holders are
  often hours stale. The new database's history follows the event log, except
  for WETH in 2018 and 2019.
- **Every chain's wrapped native contract has the same native balance in
  both databases, to the wei.**

## USDT

`0xdac17f958d2ee523a2206206994597c13d831ec7`, 6 decimals.

| Count at the cutoff | Value |
| --- | ---: |
| Comparable | 78,272,145 |
| Match | 78,270,744 (99.998210%) |
| Non-zero comparable / match | 15,167,866 / 15,166,465 (99.990763%) |
| Mismatch: new older / same block / new newer | 3 / 36 / 1,362 |
| Value match (comparable holders, 59% of supply) | 99.997905% |
| After the cutoff | 224,208 |
| Only RPC-era | 11,611,766, all zero |
| Only new: zero / non-zero | 395 / 135 (16,527.61 USDT) |
| Top 10 / 100 / 1,000 | 10 / 100 / 1,000, value 100%, no phantom holder |

Across all 90,280,237 holders, compared at the cutoff hour (2026-10-17 21:00
UTC, using hourly closes for holders changed after the cutoff), 1,537
differ. The value match is 99.998715%.

| Class | Holders | RPC-era minus new | Wrong side | Evidence |
| --- | ---: | ---: | --- | --- |
| RPC-era lost the row (new newer) | 1,362 | +872,799.58 USDT | RPC-era | New minus RPC-era equals the net `Transfer` flow between the two rows, for 1,362 of 1,362 |
| RPC-era lost the row (only new) | 135 non-zero | −16,527.61 USDT | RPC-era | The net flow equals the new balance for all 530 only-new holders |
| RPC-era stored the pre-block value (same block) | 36 | +45,331.67 USDT | RPC-era | New minus RPC-era equals the holder's one transfer in the block, 36 of 36 |
| Zero address | 1 | +39,902.54 USDT | new, by design | |
| RPC-era wrote 0 with no event (new older) | 3 | −0.02 USDT | needs RPC | No transfer of any token touches them in between |

The lost rows are mostly in blocks 24,527,334 to 24,527,826 (2026-02-24
14:24 to 16:00 UTC) and 24,635,701 to 24,635,725 (2026-03-11 around 17:25
UTC), plus block 24,626,677. The RPC-era has every block there; it wrote no
row for these holders. Its hourly row counts for the busiest holders show
more heavy losses in the live part, which affect history only:
2026-02-09 16:00 to 02-10 02:00, 02-20 19:00 to 20:00, 05-06 15:00, 05-28
07:00, 08-25 21:00 and 09-10 10:00 to 14:00 UTC.

**Supply.** USDT's supply is its initial 100,000 USDT (1e11 units; the
deployer's balance confirms it), plus `Issue`, less `Redeem` and
`DestroyedBlackFunds`. At the cutoff hour that is 88,302,633,055.702652 USDT.
The new total is exactly the supply minus the zero address's 39,902.544679
USDT. The RPC-era total is the supply plus 901,603.620259 USDT, and the
classes above account for the difference to the unit.

**History.** Top 100 holders at 00:00 UTC on each date, from hourly bars:

| Date | RPC-era top 100 matched | Top 10 matched | Top-100 value match |
| --- | ---: | ---: | ---: |
| 2018-06-01 | 100 | 10 | 100.00% |
| 2019-01-01 | 98 | 8 | 83.25% |
| 2020-06-01 | 81 | 5 | 72.48% |
| 2021-06-01 | 64 | 4 | 56.98% |
| 2022-06-01 | 68 | 6 | 64.27% |
| 2023-06-01 | 80 | 9 | 83.34% |
| 2024-06-01 | 78 | 8 | 78.81% |
| 2025-06-01 | 85 | 8 | 91.88% |
| 2026-06-01 | 100 | 10 | 100.00% |

In all 153 differing entries, the new database's last change is later, and
new minus RPC-era equals the net `Transfer` flow in between. The RPC-era's
backfill kept about one row per holder per write batch, so its closes are
up to 2,176 blocks stale. A 1-in-2,000 random holder sample differs in at
most one holder per date.

## USDC

`0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48`, 6 decimals. Recounted on
2026-10-19 with `argMax` instead of `FINAL`:

| Count at the cutoff | Value |
| --- | ---: |
| Comparable | 36,888,869 |
| Match | 36,888,050 (99.997780%) |
| Non-zero comparable / match | 8,999,692 / 8,998,873 (99.990900%) |
| Mismatch: new older / same block / new newer | 1 / 11 / 807 |
| Value match (comparable holders, 32.8B of 48.0B USDC) | 99.992018% |
| After the cutoff | 185,621 |
| Only RPC-era | 8,170,188, all zero |
| Only new: zero / non-zero | 237 / 97 (18,548.46 USDC) |
| Top 10 / 100 / 1,000 (run of 2026-10-18) | 10 / 100 / 1,000, no phantom holder |

| Class | Holders | Wrong side | Evidence |
| --- | ---: | --- | --- |
| RPC-era lost the row (new newer) | 807 | RPC-era | The net flow between the rows equals the difference for 807 of 807: 667 in blocks 24,527,337 to 24,527,814, 47 in 24,635,700 to 24,635,725, 93 elsewhere in 24,503,379 to 24,632,118 |
| RPC-era lost the row (only new) | 334 (97 non-zero) | RPC-era | The net flow since 24,420,000 equals the new balance for all 334 |
| RPC-era stored the pre-block value (same block) | 11 | RPC-era | Blocks 23,220,071 to 23,338,667, as for USDT |
| New row stale (new older) | 1 | new | `0x9999fa87f5a1d1c64e7c709838b92006ab0cc1ad`: 0.2 USDC since 18,963,384; two later transfers net −0.2 |

119,894 holders have an RPC-era row only after the cutoff and no new row
(19,424 non-zero). The 31,419 of them with bars before the cutoff hour were
all zero there, so none hides a balance at the cutoff.

**Supply.** `Transfer` from `0x0` minus `Transfer` to `0x0` gives
48,049,938,040.151212 USDC, once duplicate rows are removed (below). The new
total is 48,049,938,040.351212 USDC: 0.2 USDC more, the stale holder above.

**Blacklist bit.** FiatToken sets bit 255 of a blacklisted holder's word
([limits](../erc20/balances/README.md#limits)). No balance in either
database reaches 2^128. The largest is 4,192,783,701.439372 USDC, at
`0x37305b1cd40574e4c5ce33f8e8306be057fd7341`.

## WETH

`0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2`, WETH9.

| Count at the cutoff | Value |
| --- | ---: |
| Comparable | 6,648,512 |
| Match | 6,648,245 (99.9960%) |
| Non-zero comparable / match | 3,316,317 / 3,316,050 (99.9919%) |
| Mismatch: new older / same block / new newer | 14 / 253 / 0 |
| Value match (comparable holders) | 745,207.378 of 745,208.778 WETH (99.99981%) |
| After the cutoff | 27,526; 55,712 of 55,714 changed since the cutoff hour are equal at it |
| Only RPC-era: zero / non-zero | 19,223,776 / 11 |
| Only new | 0 |
| Top 10 / 100 / 1,000 | 10 / 100 / 999 (the zero address, rank 130), value 99.946% |

| Class | Holders | Amount | Wrong side |
| --- | ---: | ---: | --- |
| Zero address | 1 | 1,088.924170800877388827 WETH | new, by design |
| Credited only by `Deposit`, in a block whose only WETH events are deposits (2018-04 to 2019-08) | 12 (11 only RPC-era, 1 new older) | 0.488013932 WETH | new |
| Dust transfer stored before the transfer (blocks 23,307,096 and 23,296,064) | 254 | 0.000604 WETH | RPC-era |
| RPC-era wrote 0 on 2026-02-24, 14:28 to 15:54 UTC | 13 | 0.000176 WETH | RPC-era likely, needs RPC |

The 12 deposit-only holders never moved again. Since v0.7.4 a `Deposit`
frame without a `Transfer` counts only when it passes the backed-deposit
check, and these frames evidently fail it. Which condition fails needs the
Extended blocks. The dust transaction at 23,307,096
(`0xd22b359d7d8661fcd7fb4a966e8f5ec610c0376e345983ea272aca2fddc579df`) sends
1e11 wei to 635 addresses; the RPC-era stored the pre-transfer balance for
268 of them.

**Supply.** WETH9's `totalSupply()` is its own ETH balance. The contract's
ETH balance in the native database minus the sum of the new WETH balances is
1,089.413184732877412499 ETH, both at block 26,197,999 and at the cutoff
hour, identical to the wei. That is the zero address, the 12 missed
deposits, and a residual of 0.000985 to 0.001 ETH, depending on which value
of the zero address is right. The residual fits ETH sent to the contract
without a deposit, but it was not measured. For the RPC-era database the gap
is 0.000572 ETH.

**History.** The new database's history follows the `Transfer`, `Deposit`
and `Withdrawal` events, except in 2018 and 2019. On 2019-08-19 it skipped 32
of 137 deposit-only holder-blocks, so 32 of the 759 holders that moved that
day carried a wrong change (226.02 WETH) until their next write. In the
windows tested in 2021, 2023 and 2026 it matches the events for every
holder. The RPC-era's backfilled history matches them for 64.3% (2021) and
31.5% (2023) of the holders that moved.

| Date | RPC-era top 100 matched | Top 10 matched | Top-100 value match |
| --- | ---: | ---: | ---: |
| 2018-06-01 | 97 | 9 | 98.53% |
| 2019-01-01 | 86 | 6 | 4.29% |
| 2020-06-01 | 66 | 3 | 5.31% |
| 2021-06-01 | 40 | 1 | 8.92% |
| 2022-06-01 | 49 | 1 | 12.17% |
| 2023-06-01 | 62 | 5 | 67.22% |
| 2024-06-01 | 59 | 6 | 68.72% |
| 2025-06-01 | 60 | 5 | 62.87% |
| 2026-06-01 | 100 | 10 | 100.00% |

In every differing pair, the RPC-era's last change before the hour is
earlier than the new database's. Where both last changes are at the same
block, the values are equal. Very active holders have 3 to 5 hourly bars a
day in the RPC-era's backfill, against 23 or 24 in the new database.

## Wrapped native tokens on every chain

Every RPC-era row of these contracts is after the cutoff, so they are
compared at the start of the cutoff's hour (H) from hourly bars.

| Chain | Token | Contract | Contract's native balance at H, both databases | Native balance − RPC-era holder sum |
| --- | --- | --- | ---: | ---: |
| Ethereum | WETH | `0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2` | 2,218,037.701258277986841060 | 0.000572092148695827 |
| BSC | WBNB | `0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c` | 1,547,839.324610474773389070 | −250.083918561925075354 |
| Base | WETH | `0x4200000000000000000000000000000000000006` | 279,987.532918005106225555 | 0 |
| Optimism | WETH | `0x4200000000000000000000000000000000000006` | 19,176.571910300465429016 | 1 wei |
| Unichain | WETH | `0x4200000000000000000000000000000000000006` | 2,759.006521949911073920 | 0 |
| Polygon | WPOL | `0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270` | 160,677,221.843449373848996516 | −42.713585340815414220 |
| HyperEVM | WHYPE | `0x5555555555555555555555555555555555555555` | 3,385,289.581273615125155108 | 1.000000000000000000 |

- **Native balances agree.** On all seven chains the contract's native
  balance at H is the same to the wei in both databases. Each new row at the
  cutoff also equals the close at H plus the net native flow to the cutoff
  in `<chain>:evm-transfers@v0.3.3`.
- **Native history agrees.** On six chains, no hourly bar ending at the same
  block differs. On Ethereum, 274 do, all in the RPC-era's first live weeks
  (2026-02-10 to 2026-02-23), and 272 of them hold the pre-block balance
  ([native](quality-native.md#errors-in-the-rpc-era-databases)).
- **A holder sum cannot exceed the contract's balance**, since every WETH9
  token is backed by native currency. So a negative gap is an RPC-era
  error:
  - BSC: 73 WBNB holders whose last change fell in the 2,063 blocks the
    RPC-era never wrote are stale. Their missed changes sum to
    −250.083920231925075354 WBNB, which leaves +0.00000167 WBNB. For the five
    largest (72% of the amount), the superseded
    `bsc:evm-balances-erc20@v0.7.3`, also built from Extended blocks, holds
    exactly the RPC-era value minus the missed change.
  - Polygon: the excess is the same to the wei since at least 2026-03-01,
    13 days after the RPC-era's live start. It is not missing blocks, the
    zero address or a holder left stale in the live part. The holders are not
    identified yet; a rebuild from `polygon:evm-transfers@v0.3.3` with
    duplicate rows removed should find them without RPC.
- **Small positive gaps** fit native currency sent without a deposit:
  0.000572 ETH on Ethereum, and exactly 1 WHYPE on HyperEVM, a step visible
  in both databases whose block was not isolated. On Base, Optimism and
  Unichain the sum equals the balance, to 1 wei.
- **RPC-era hourly closes lag.** In its live part, the RPC-era's hourly
  close of the wrapped contract ends before the hour's last change in 95% of
  the hours on BSC (median 11 blocks) and 82% on Base (median 6 blocks). The
  comparisons at H above happen to be free of this lag.
- **New ERC-20 database on Ethereum.** WETH holders sum to
  1,089.413184732877412499 ETH below the contract's balance (the zero address
  and the missed deposits, [above](#weth)). The top 20 holders are equal in
  both ERC-20 databases.

The transfer databases used as evidence here have duplicate rows and
other gaps; see [Methods](quality-methods.md#evidence-databases) before
summing them.
