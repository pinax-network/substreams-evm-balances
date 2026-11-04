#!/bin/bash
# Apply corrected seeded ERC-20 rows to `<chain>:evm-balances-erc20@v0.7.4` on cluster C.
#
# Run from the chain's directory (the one holding corrected_rows.tsv) with admin credentials, step by step:
#   CHAIN=arbitrum-one CH_URL=http://<node>:8123 CH_USER=<admin> CH_PASSWORD=<pw> ../apply.sh <step>
# Steps, in order:
#   create   create the staging table ON CLUSTER and load corrected_rows.tsv into it (one replica, replication copies)
#   check    show staging counts; nothing is written
#   bars     submit the deletion of the seeded history bars in the corrected rows' windows (one mutation on the
#            replicated table) and return; it rewrites the whole state table on each replica, which takes hours
#   progress bars still to delete and the mutation's state on each replica; read-only
#   rows     insert the corrected rows into erc20_balances; the materialized view rebuilds their bars. Safe while
#            the mutation runs: a mutation only touches parts that existed when it was submitted
#   verify   read-only checks
#   drop     drop the staging table ON CLUSTER; refused while the mutation, whose subquery reads it, is not done
# Every corrected (contract, holder) is current below the start block S, so it has no row or bar at or after S;
# the live sink, which writes from S on, keeps running throughout.
set -euo pipefail
: "${CHAIN:?}" "${CH_URL:?}" "${CH_USER:?}" "${CH_PASSWORD?}"
case "$CHAIN" in
  arbitrum-one) S=22208000; BOUNDARY_TS="'2022-08-31 17:00:00'" ;;   # the hour after block 22,207,999 (16:26:49 UTC)
  optimism)     S=105236000; BOUNDARY_TS="'2023-06-06 17:00:00'" ;;  # the hour after block 105,235,999 (16:59:35 UTC)
  *) echo "unknown CHAIN $CHAIN"; exit 1 ;;
esac
DBN="$CHAIN:evm-balances-erc20@v0.7.4"
DB="\`$DBN\`"
STN="fix_seed_2026_11"
STAGE="$DB.$STN"
CLUSTER=${CLUSTER-tokenapis-c}
ON_CLUSTER=""; if [ -n "$CLUSTER" ]; then ON_CLUSTER="ON CLUSTER '$CLUSTER'"; fi
STAGE_ENGINE=${STAGE_ENGINE-"ReplicatedMergeTree('/clickhouse/tables/{uuid}/{shard}', '{replica}')"}
MUT_SRC="system.mutations"; if [ -n "$CLUSTER" ]; then MUT_SRC="clusterAllReplicas('$CLUSTER', system.mutations)"; fi
WINDOWS="SELECT interval_min, address, contract, toDateTime(intDiv(toUInt32(timestamp), interval_min * 60) * interval_min * 60) FROM $STAGE ARRAY JOIN [60, 240, 1440, 10080] AS interval_min"

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
  ch "SELECT fix_case, count() AS rows, countIf(old_balance = 0) AS were_zero, uniq(contract) AS tokens FROM $STAGE GROUP BY fix_case ORDER BY fix_case"
  ch "SELECT 'staging pairs still current below S' AS k, count() AS n FROM (SELECT contract, address, max(block_num) AS b FROM $DB.erc20_balances WHERE (contract, address) IN (SELECT contract, address FROM $STAGE) GROUP BY contract, address) WHERE b < $S"
  ch "SELECT 'bars the mutation will delete' AS k, count() AS n FROM $DB.historical_erc20_balances_state WHERE timestamp < $BOUNDARY_TS AND (interval_min, address, contract, timestamp) IN ($WINDOWS)"
  ;;
bars)
  # The subquery reads the replicated staging table, so every replica evaluates the same set once replication has
  # caught up: check that first, then allow the "nondeterministic" mutation. mutations_sync=2 waits for all replicas.
  if [ -n "$CLUSTER" ]; then
    ch "SELECT hostName() AS host, sum(rows) AS staging_rows FROM clusterAllReplicas('$CLUSTER', system.parts) WHERE database = '$DBN' AND table = '$STN' AND active GROUP BY host ORDER BY host" | tee /dev/stderr \
      | awk -F'\t' 'NR>1{c[$2]++} END{if (length(c) != 1) {print "staging row counts differ between replicas; wait for replication and rerun"; exit 1}}'
  fi
  ch "ALTER TABLE $DB.historical_erc20_balances_state DELETE WHERE timestamp < $BOUNDARY_TS AND (interval_min, address, contract, timestamp) IN ($WINDOWS)" "mutations_sync=0&allow_nondeterministic_mutations=1"
  echo "mutation submitted; follow it with: ../apply.sh progress"
  ;;
progress)
  ch "SELECT 'bars still to delete' AS k, count() AS n FROM $DB.historical_erc20_balances_state WHERE timestamp < $BOUNDARY_TS AND (interval_min, address, contract, timestamp) IN ($WINDOWS)"
  ch "SELECT hostName() AS host, mutation_id, create_time, is_done, parts_to_do, latest_fail_reason FROM $MUT_SRC WHERE database = '$DBN' AND table = 'historical_erc20_balances_state' ORDER BY create_time DESC LIMIT 4"
  ;;
rows)
  ch "INSERT INTO $DB.erc20_balances (block_num, block_hash, timestamp, contract, address, balance) SELECT block_num, block_hash, timestamp, contract, address, balance FROM $STAGE" "max_insert_threads=4&min_insert_block_size_rows=1000000&optimize_on_insert=0"
  echo "inserted; the materialized view wrote the bars"
  ;;
verify)
  ch "SELECT 'rows below S' AS k, count() AS rows, countIf(balance = 0) AS zero_rows FROM $DB.erc20_balances FINAL WHERE block_num < $S"
  ch "SELECT 'staging rows now current with the corrected balance' AS k, count() AS n FROM (SELECT contract, address, max(block_num) AS b, argMax(balance, block_num) AS v FROM $DB.erc20_balances WHERE (contract, address) IN (SELECT contract, address FROM $STAGE) GROUP BY contract, address) AS r INNER JOIN $STAGE AS s USING (contract, address) WHERE r.b = s.block_num AND r.v = s.balance"
  ch "SELECT contract, address, block_num, toString(balance) AS balance FROM $DB.erc20_balances FINAL WHERE (contract, address) IN (SELECT contract, address FROM $STAGE ORDER BY old_balance DESC LIMIT 5)"
  ;;
drop)
  pending=$(ch "SELECT count() FROM $MUT_SRC WHERE database = '$DBN' AND table = 'historical_erc20_balances_state' AND NOT is_done" | tail -n1)
  if [ "$pending" != "0" ]; then echo "$pending mutation(s) on historical_erc20_balances_state not done; their subquery reads the staging table, so it stays. Rerun after ../apply.sh progress shows is_done = 1 on every replica."; exit 1; fi
  ch "DROP TABLE IF EXISTS $STAGE $ON_CLUSTER SYNC" "distributed_ddl_task_timeout=600"
  ;;
*)
  sed -n 2,18p "$0"; exit 1 ;;
esac
