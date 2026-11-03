#!/bin/bash
# usage: CH_URL=http://host:8123 CH_USER=... CH_PASSWORD=... sample.sh <db-name> <out.tsv>
# Samples the top 1,000 holders (balance >= 1 native unit) plus up to 900 random addresses per balance stratum
# (zero, <0.01, 0.01-1, 1-100, >100 units) from the 0x00 address slice of <db-name>.native_balances FINAL.
set -euo pipefail
DB="$1"; OUT="$2"
curl -sS --fail-with-body --max-time 3600 -u "${CH_USER:-default}:${CH_PASSWORD-}" "${CH_URL:-http://127.0.0.1:8123}/?default_format=TSVWithNames&readonly=2" --data-binary "
WITH top AS (SELECT address, block_num, balance, 'top' AS stratum FROM \`$DB\`.native_balances FINAL WHERE balance >= toUInt256(1000000000000000000) ORDER BY balance DESC LIMIT 1000),
     slice AS (SELECT address, block_num, balance FROM \`$DB\`.native_balances FINAL WHERE address LIKE '0x00%'),
     rnd AS (SELECT address, block_num, balance, multiIf(balance = 0, 'zero', balance < toUInt256(10000000000000000), 'lt0.01', balance < toUInt256(1000000000000000000), '0.01-1', balance < toUInt256(100) * toUInt256(1000000000000000000), '1-100', 'gt100') AS stratum FROM slice)
SELECT address, block_num, toString(balance) AS balance_wei, stratum FROM top
UNION ALL
SELECT address, block_num, toString(balance), stratum FROM (SELECT *, row_number() OVER (PARTITION BY stratum ORDER BY cityHash64(address)) AS rn FROM rnd) WHERE rn <= 900
FORMAT TSVWithNames" > "$OUT"
