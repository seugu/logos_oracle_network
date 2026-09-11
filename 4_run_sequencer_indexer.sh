#!/usr/bin/env bash
#
# Terminal 3, step 2 (every session): run sequencer + indexer
# -----------------------------------------------------------------
# Run this AFTER ./3_bootstrap_deploy.sh has completed at least once, and
# while ./1_start_lez.sh (terminal 1) and ./2_start_logos_blockchain.sh
# (terminal 2) are both running.
#
# sequencer runs in the background writing to its own log file; indexer runs
# in the foreground so you watch the attestations live. Ctrl+C stops both.
#
# Expected flow once running:
#   sequencer.log -> "Already registered" -> "Binance connection opened"
#                    -> "Publishing..." -> "Submitted price update in N ms"
#   this terminal -> "Obs: PriceObservation { feed_id: \"BTC/USDT\", ... }"
#                    -> "Attested round N: Price ..., Count 1"
#                    -> "Successfully published attested price ..."
#   ...repeating roughly once a minute.
#
# Usage:
#   ./4_run_sequencer_indexer.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"
LEZ_DIR="$PROJECT_ROOT/logos-execution-zone"
SPEL_DIR="$PROJECT_ROOT/spel"
ORACLE_NODE_DIR="$PROJECT_ROOT/oracle_node"

LOG_DIR="$HOME/local_run/logs"
DATA_FOLDER="$HOME/local_run/oracle_node/sequencer"
TARGET_ORACLE_NODE="$HOME/local_target/oracle_node"

NODE_URL="http://localhost:18080"
NODE_REST_URL="http://localhost:18080"
LEZ_ADDR="http://127.0.0.1:3040"

export PATH="$HOME/.risc0/bin:$LEZ_DIR/target/release:$SPEL_DIR/target/debug:$PATH"
export CARGO_TARGET_DIR="$TARGET_ORACLE_NODE"
mkdir -p "$LOG_DIR" "$DATA_FOLDER"

# Same pyo3/Xcode-Python fix as the other scripts, in case anything pulled
# into this build depends on it too.
if [ "$(uname)" = "Darwin" ]; then
    PY=""
    if command -v brew >/dev/null 2>&1; then
        for v in 3.13 3.12 3.11 3.10; do
            if brew --prefix "python@$v" >/dev/null 2>&1; then
                candidate="$(brew --prefix "python@$v")/bin/python$v"
                if [ -x "$candidate" ]; then PY="$candidate"; break; fi
            fi
        done
    fi
    if [ -z "$PY" ] && command -v python3 >/dev/null 2>&1; then
        PY="$(command -v python3)"
    fi
    [ -n "$PY" ] && export PYO3_PYTHON="$PY"
fi

SEQ_PID=""
cleanup() {
    echo ""
    echo ">> Stopping sequencer and indexer..."
    [ -n "$SEQ_PID" ] && kill "$SEQ_PID" >/dev/null 2>&1
    exit 0
}
trap cleanup INT TERM

fail() { echo "ERROR: $1"; exit 1; }

[ -f "$ORACLE_NODE_DIR/resources/register_contract_config.json" ] \
    || fail "register_contract_config.json missing. Run ./3_bootstrap_deploy.sh first."

# Poll the port rather than `wallet check-health`, which can block on a
# password prompt when the keystore is fresh.
echo ">> Waiting for LEZ (terminal 1)..."
ready=0
for _ in $(seq 1 30); do
    if curl -s -o /dev/null --max-time 2 "$LEZ_ADDR" 2>/dev/null; then ready=1; break; fi
    sleep 2
done
[ "$ready" -eq 1 ] || fail "LEZ isn't reachable on $LEZ_ADDR. Is ./1_start_lez.sh running?"
echo "   LEZ is up."

echo ">> Waiting for Logos blockchain (terminal 2)..."
ready=0
for _ in $(seq 1 60); do
    code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 2 "$NODE_REST_URL/time/info" 2>/dev/null)
    if [ "$code" = "200" ]; then ready=1; break; fi
    sleep 2
done
[ "$ready" -eq 1 ] || fail "Logos blockchain isn't reachable on $NODE_REST_URL. Is ./2_start_logos_blockchain.sh running?"

TIME_INFO=$(curl -s "$NODE_REST_URL/time/info")
echo "   Logos blockchain is up: $TIME_INFO"

# A stale genesis makes the zone-sdk try to backfill millions of slots and it
# never finishes. 2_start_logos_blockchain.sh patches this, so warn loudly if
# the chain still reports an absurd slot.
CURRENT_SLOT=$(echo "$TIME_INFO" | sed -E 's/.*"current_slot":([0-9]+).*/\1/')
if [ -n "$CURRENT_SLOT" ] && [ "$CURRENT_SLOT" -gt 1000000 ] 2>/dev/null; then
    echo ""
    echo "   WARNING: current_slot is $CURRENT_SLOT, which is very large."
    echo "   The chain's genesis time is stale, so the sequencer will try to"
    echo "   backfill from slot 0 and will appear to hang at 'Publishing...'."
    echo "   Fix: restart terminal 2 with ./2_start_logos_blockchain.sh"
    echo "        (it patches genesis to 'now' and resets the chain)."
    echo ""
fi

echo ""
echo ">> Building sequencer and indexer..."
cd "$ORACLE_NODE_DIR" || fail "cannot cd to $ORACLE_NODE_DIR"
cargo build -p sequencer -p indexer || fail "build failed"

echo ""
echo ">> Starting sequencer in the background (log: $LOG_DIR/sequencer.log)"
RUST_LOG="info,sequencer::sequencer=debug" nohup cargo run -q -p sequencer -- \
    --data-folder "$DATA_FOLDER" \
    --node-url "$NODE_URL" \
    --node-rest-url "$NODE_REST_URL" \
    > "$LOG_DIR/sequencer.log" 2>&1 &
SEQ_PID=$!
echo "   sequencer PID: $SEQ_PID"
echo "   Watch it with: tail -f $LOG_DIR/sequencer.log"

echo "   Waiting 20s for register + Binance connection..."
sleep 20

if ! kill -0 "$SEQ_PID" >/dev/null 2>&1; then
    echo ""
    echo "ERROR: the sequencer exited early. Last lines of its log:"
    tail -30 "$LOG_DIR/sequencer.log"
    exit 1
fi

echo ""
echo ">> Starting indexer in the foreground. Ctrl+C stops both."
echo ""
cargo run -q -p indexer -- \
    --node-url "$NODE_URL" \
    --node-rest-url "$NODE_REST_URL"

cleanup
