#!/bin/bash
# Write corrected rows of stale holders into `<chain>:evm-balances-erc20@<VERSION>` on cluster C, at block H.
#
# Run from the token's directory (the one holding corrected_rows.tsv) with admin credentials, step by step:
#   CHAIN=mainnet CH_URL=http://<node>:8123 CH_USER=<admin> CH_PASSWORD=<pw> ../apply.sh <step>
# Steps, in order:
#   create   create the staging table ON CLUSTER and load corrected_rows.tsv into it
#   check    staging counts, and how many staging pairs still have no row after H; nothing is written
#   rows     insert the corrected rows into erc20_balances at H; the materialized view adds their bars
#   verify   read-only checks
#   drop     drop the staging table ON CLUSTER
# A pair the live sink wrote after H keeps that newer row: the insert at H does not replace it. No history bar is
# deleted: a stale pair has no row in H's windows, so the inserted row only adds the corrected close there.
set -euo pipefail
: "${CHAIN:?}" "${CH_URL:?}" "${CH_USER:?}" "${CH_PASSWORD?}"
VERSION=${VERSION-v0.7.4}
DBN="$CHAIN:evm-balances-erc20@$VERSION"
DB="\`$DBN\`"
STN="fix_stale_2026_11"
STAGE="$DB.$STN"
CLUSTER=${CLUSTER-tokenapis-c}
ON_CLUSTER=""; if [ -n "$CLUSTER" ]; then ON_CLUSTER="ON CLUSTER '$CLUSTER'"; fi
STAGE_ENGINE=${STAGE_ENGINE-"ReplicatedMergeTree('/clickhouse/tables/{uuid}/{shard}', '{replica}')"}

ch() {  # ch '<SQL>' [extra url params]
  local out
  if ! out=$(curl -sS --fail-with-body --max-time 7200 -u "$CH_USER:$CH_PASSWORD" "$CH_URL/?default_format=TSVWithNames&${2:-}" --data-binary "$1"); then
    echo "ClickHouse error: $out" >&2; echo "query: $1" >&2; return 1
  fi
  printf '%s\n' "$out"
}

case "${1:-}" in
create)
  ch "CREATE TABLE IF NOT EXISTS $STAGE $ON_CLUSTER (
        contract String, address String, block_num UInt32, block_hash String, timestamp DateTime('UTC'),
        balance UInt256, old_balance UInt256, fix_case LowCardinality(String)
      ) ENGINE = $STAGE_ENGINE ORDER BY (contract, address)" "distributed_ddl_task_timeout=600"
  curl -sS --fail-with-body --max-time 7200 -u "$CH_USER:$CH_PASSWORD" \
    "$CH_URL/?query=$(python3 -c "import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1]))" "INSERT INTO $STAGE (contract, address, block_num, block_hash, timestamp, balance, old_balance, fix_case) FORMAT TSV")" \
    --data-binary @corrected_rows.tsv
  echo "loaded $(wc -l < corrected_rows.tsv) rows into staging"
  ;;
check)
  echo "file rows: $(wc -l < corrected_rows.tsv)"
  ch "SELECT contract, count() AS rows, countIf(old_balance = 0) AS were_zero, countIf(balance = 0) AS now_zero, any(block_num) AS h FROM $STAGE GROUP BY contract ORDER BY contract"
  ch "SELECT 'staging pairs without a row after H' AS k, count() AS n FROM (SELECT contract, address, max(block_num) AS b FROM $DB.erc20_balances WHERE (contract, address) IN (SELECT contract, address FROM $STAGE) GROUP BY contract, address) AS r INNER JOIN $STAGE AS s USING (contract, address) WHERE r.b <= s.block_num"
  ;;
rows)
  ch "INSERT INTO $DB.erc20_balances (block_num, block_hash, timestamp, contract, address, balance) SELECT block_num, block_hash, timestamp, contract, address, balance FROM $STAGE" "max_insert_threads=4&min_insert_block_size_rows=1000000&optimize_on_insert=0"
  echo "inserted; the materialized view wrote the bars"
  ;;
verify)
  ch "SELECT 'staging pairs now current with the corrected balance' AS k, count() AS n FROM (SELECT contract, address, max(block_num) AS b, argMax(balance, block_num) AS v FROM $DB.erc20_balances WHERE (contract, address) IN (SELECT contract, address FROM $STAGE) GROUP BY contract, address) AS r INNER JOIN $STAGE AS s USING (contract, address) WHERE r.b = s.block_num AND r.v = s.balance"
  ch "SELECT 'staging pairs with a newer row' AS k, count() AS n FROM (SELECT contract, address, max(block_num) AS b FROM $DB.erc20_balances WHERE (contract, address) IN (SELECT contract, address FROM $STAGE) GROUP BY contract, address) AS r INNER JOIN $STAGE AS s USING (contract, address) WHERE r.b > s.block_num"
  ;;
drop)
  ch "DROP TABLE IF EXISTS $STAGE $ON_CLUSTER SYNC" "distributed_ddl_task_timeout=600"
  ;;
*)
  sed -n 2,14p "$0"; exit 1 ;;
esac
