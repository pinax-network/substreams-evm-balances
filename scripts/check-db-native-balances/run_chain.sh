#!/bin/bash
# usage: run_chain.sh <label> <db-name> <rpc-id> <etherscan-chainid>
# Environment: CH_URL, CH_USER, CH_PASSWORD (ClickHouse, read-only), PINAX_KEY (RPC), ETHERSCAN_API_KEY (optional).
# Writes <label>/sample.tsv, <label>/out (Etherscan pass) and <label>/out_rpc (block-exact pass).
set -uo pipefail
L="$1"; DB="$2"; RPC="$3"; CID="$4"; D=$(dirname "$0"); mkdir -p "$D/$L"
echo "== $L  $(date -u '+%H:%M:%S')  head $(curl -sS -u "${CH_USER:-default}:${CH_PASSWORD-}" "${CH_URL:-http://127.0.0.1:8123}/?readonly=2" --data-binary "SELECT max(block_num) FROM \`$DB\`.blocks FORMAT TSV")"
bash "$D/sample.sh" "$DB" "$D/$L/sample.tsv" || { echo "sampling failed"; exit 1; }
echo "sampled $(($(wc -l < "$D/$L/sample.tsv")-1)) rows  $(date -u '+%H:%M:%S')"
DB_NAME="$DB" RPC_ID="$RPC" ES_CHAINID="$CID" python3 -I "$D/compare.py" "$D/$L/sample.tsv" "$D/$L/out" 2>&1 | tail -12
echo "-- block-exact pass"
DB_NAME="$DB" RPC_ID="$RPC" ES_CHAINID="$CID" python3 -I "$D/compare.py" "$D/$L/sample.tsv" "$D/$L/out_rpc" --rpc-only 2>&1 | tail -12
echo "done $L $(date -u '+%H:%M:%S')"
