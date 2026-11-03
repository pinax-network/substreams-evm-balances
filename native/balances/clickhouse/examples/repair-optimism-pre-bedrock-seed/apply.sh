#!/bin/bash
# Apply the corrected pre-Bedrock rows to `optimism:evm-balances-native@v0.7.11` on cluster C.
#
# Run from this directory with admin credentials, step by step:
#   CH_URL=http://ch-node890h.riv.eosn.io:8123 CH_USER=<admin> CH_PASSWORD=<pw> ./apply.sh <step>
# Steps, in order:
#   create   create the staging table ON CLUSTER and load corrected_rows.tsv into it (one replica, replication copies)
#   check    show staging counts against the file; nothing is written
#   bars     delete the seeded history bars that the corrected rows will rebuild (one mutation on the replicated
#            table, waits for both replicas); must finish before `rows`
#   rows     insert the corrected rows into native_balances; the materialized view rebuilds their bars
#   verify   read-only checks: zero rows below Bedrock, a few corrected addresses, their bars
#   drop     drop the staging table ON CLUSTER, after verification
# The live sink keeps running throughout: it writes blocks at or after 105,236,000 only.
set -euo pipefail
: "${CH_URL:?}" "${CH_USER:?}" "${CH_PASSWORD?}"
DB='`optimism:evm-balances-native@v0.7.11`'
STAGE="$DB.fix_pre_bedrock_2026_11"
CLUSTER=${CLUSTER-tokenapis-c}
ON_CLUSTER=""; if [ -n "$CLUSTER" ]; then ON_CLUSTER="ON CLUSTER '$CLUSTER'"; fi
BEDROCK_TS="'2023-06-06 17:00:00'"   # block 105,236,000; every seeded bar starts before it
STAGE_ENGINE=${STAGE_ENGINE-"ReplicatedMergeTree('/clickhouse/tables/{uuid}/{shard}', '{replica}')"}

ch() {  # ch '<SQL>' [extra url params]
  local out
  if ! out=$(curl -sS --fail-with-body --max-time 3600 -u "$CH_USER:$CH_PASSWORD" "$CH_URL/?default_format=TSVWithNames&${2:-}" --data-binary "$1"); then
    echo "ClickHouse error: $out" >&2; echo "query: $1" >&2; return 1
  fi
  printf '%s\n' "$out"
}

case "${1:-}" in
create)
  ch "CREATE TABLE IF NOT EXISTS $STAGE $ON_CLUSTER (
        address String, block_num UInt32, block_hash String, timestamp DateTime('UTC'),
        balance UInt256, old_balance UInt256, fix_case LowCardinality(String)
      ) ENGINE = $STAGE_ENGINE ORDER BY address" "distributed_ddl_task_timeout=600"
  curl -sS --fail-with-body --max-time 3600 -u "$CH_USER:$CH_PASSWORD" \
    "$CH_URL/?query=$(python3 -c "import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1]))" "INSERT INTO $STAGE (address, block_num, block_hash, timestamp, balance, old_balance, fix_case) FORMAT TSV")" \
    --data-binary @corrected_rows.tsv
  echo "loaded $(wc -l < corrected_rows.tsv) rows into staging"
  ;;
check)
  echo "file rows: $(wc -l < corrected_rows.tsv)"
  ch "SELECT fix_case, count() AS rows, countIf(old_balance = 0) AS were_zero, round(sum(toFloat64(balance))/1e18, 4) AS eth_after, round(sum(toFloat64(old_balance))/1e18, 4) AS eth_before FROM $STAGE GROUP BY fix_case ORDER BY fix_case"
  ch "SELECT 'staging addresses also current below Bedrock' AS k, count() AS n FROM $DB.native_balances FINAL WHERE block_num < 105236000 AND address IN (SELECT address FROM $STAGE)"
  ch "SELECT 'bars the mutation will delete' AS k, count() AS n FROM $DB.historical_native_balances_state WHERE timestamp < $BEDROCK_TS AND (interval_min, address, timestamp) IN (SELECT interval_min, address, toDateTime(intDiv(toUInt32(timestamp), interval_min * 60) * interval_min * 60) FROM $STAGE ARRAY JOIN [60, 240, 1440, 10080] AS interval_min)"
  ;;
bars)
  # The subquery reads the replicated staging table, so every replica evaluates the same set once replication has
  # caught up: check that first, then allow the "nondeterministic" mutation. Submitted on one replica, replicated to
  # the others; mutations_sync=2 waits for all of them.
  if [ -n "$CLUSTER" ]; then
    ch "SELECT hostName() AS host, sum(rows) AS staging_rows FROM clusterAllReplicas('$CLUSTER', system.parts) WHERE database = 'optimism:evm-balances-native@v0.7.11' AND table = 'fix_pre_bedrock_2026_11' AND active GROUP BY host ORDER BY host" | tee /dev/stderr \
      | awk -F'\t' 'NR>1{c[$2]++} END{if (length(c) != 1) {print "staging row counts differ between replicas; wait for replication and rerun"; exit 1}}'
  fi
  ch "ALTER TABLE $DB.historical_native_balances_state DELETE WHERE timestamp < $BEDROCK_TS AND (interval_min, address, timestamp) IN (SELECT interval_min, address, toDateTime(intDiv(toUInt32(timestamp), interval_min * 60) * interval_min * 60) FROM $STAGE ARRAY JOIN [60, 240, 1440, 10080] AS interval_min)" "mutations_sync=2&allow_nondeterministic_mutations=1"
  MUT_SRC="system.mutations"; if [ -n "$CLUSTER" ]; then MUT_SRC="clusterAllReplicas('$CLUSTER', system.mutations)"; fi
  ch "SELECT hostName() AS host, mutation_id, is_done, parts_to_do, latest_fail_reason FROM $MUT_SRC WHERE database = 'optimism:evm-balances-native@v0.7.11' AND table = 'historical_native_balances_state' ORDER BY create_time DESC LIMIT 4"
  ;;
rows)
  ch "INSERT INTO $DB.native_balances (block_num, block_hash, timestamp, address, balance) SELECT block_num, block_hash, timestamp, address, balance FROM $STAGE" "max_insert_threads=4&min_insert_block_size_rows=1000000&optimize_on_insert=0"
  echo "inserted; the materialized view wrote the bars"
  ;;
verify)
  ch "SELECT 'rows below Bedrock' AS k, count() AS rows, countIf(balance = 0) AS zero_rows FROM $DB.native_balances FINAL WHERE block_num < 105236000"
  ch "SELECT 'staging rows now current with the corrected balance' AS k, count() AS n FROM (SELECT address, block_num, balance FROM $DB.native_balances FINAL WHERE address IN (SELECT address FROM $STAGE)) AS b INNER JOIN $STAGE AS s USING (address) WHERE b.block_num = s.block_num AND b.balance = s.balance"
  ch "SELECT address, block_num, toString(balance) AS balance FROM $DB.native_balances FINAL WHERE address IN (SELECT address FROM $STAGE ORDER BY cityHash64(address) LIMIT 5)"
  ch "SELECT interval_min, address, timestamp, min_block_num, max_block_num, toString(open) AS open, toString(low) AS low, toString(high) AS high, toString(close) AS close, transactions FROM $DB.historical_native_balances WHERE address IN (SELECT address FROM $STAGE ORDER BY cityHash64(address) LIMIT 2) ORDER BY address, interval_min, timestamp"
  ;;
drop)
  ch "DROP TABLE IF EXISTS $STAGE $ON_CLUSTER SYNC" "distributed_ddl_task_timeout=600"
  ;;
*)
  sed -n 2,16p "$0"; exit 1 ;;
esac
