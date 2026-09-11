#!/usr/bin/env bash
#
# Step 5: check the live BTC/USDT price on-chain
# --------------------------------------------------
# Run this any time (in a fourth terminal, or in terminal 3 after stopping
# 4_run_sequencer_indexer.sh) to confirm the whole pipeline is actually
# working end to end: Binance -> sequencer -> Logos blockchain -> indexer ->
# oracle_prices contract on LEZ.
#
# Requires 1_start_lez.sh (terminal 1) to be running, and
# 3_bootstrap_deploy.sh to have completed at least once.
#
# Usage:
#   ./5_check_price.sh              # check once
#   ./5_check_price.sh --watch      # check every 10s until Ctrl+C

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"
LEZ_DIR="$PROJECT_ROOT/logos-execution-zone"
SPEL_DIR="$PROJECT_ROOT/spel"
DEPLOY_INFO="$SCRIPT_DIR/.deploy_info"

export PATH="$HOME/.risc0/bin:$LEZ_DIR/target/release:$SPEL_DIR/target/debug:$PATH"

fail() { echo "ERROR: $1"; exit 1; }

command -v spel >/dev/null 2>&1 || fail "spel not found on PATH. Run ./0_build_toolchain.sh first."
[ -f "$DEPLOY_INFO" ] || fail "$DEPLOY_INFO not found. Run ./3_bootstrap_deploy.sh first."

# shellcheck disable=SC1090
source "$DEPLOY_INFO"

[ -n "${FEED_PDA:-}" ]  || fail "FEED_PDA missing from $DEPLOY_INFO."
[ -n "${OP_IDL:-}" ]    || fail "OP_IDL missing from $DEPLOY_INFO."
[ -n "${FEED_NAME:-}" ] && FEED_NAME="$FEED_NAME" || FEED_NAME="the feed"

check_once() {
    local out
    out=$(spel inspect "$FEED_PDA" --idl "$OP_IDL" --type PriceState 2>&1)
    local price round valid_count decimals

    price=$(echo "$out" | grep '"price"' | grep -oE '[0-9]+' | head -1)
    round=$(echo "$out" | grep '"round"' | grep -oE '[0-9]+' | head -1)
    valid_count=$(echo "$out" | grep '"valid_count"' | grep -oE '[0-9]+' | head -1)
    decimals=$(echo "$out" | grep '"decimals"' | grep -oE '[0-9]+' | head -1)

    if [ -z "$price" ] || [ "${valid_count:-0}" -eq 0 ] 2>/dev/null; then
        echo "[$(date '+%H:%M:%S')] Not published yet (round=${round:-?}, valid_count=${valid_count:-0})."
        echo "   Make sure terminal 1, 2, and 4_run_sequencer_indexer.sh are all running."
        return 1
    fi

    local human_price
    human_price=$(python3 -c "print(f'{int(\"$price\") / (10 ** int(\"$decimals\")):,.2f}')" 2>/dev/null || echo "$price")

    echo "[$(date '+%H:%M:%S')] $FEED_NAME = \$$human_price   (round $round, $valid_count observation(s))"
    return 0
}

if [ "${1:-}" = "--watch" ]; then
    echo ">> Watching $FEED_NAME price every 10s. Ctrl+C to stop."
    echo ""
    while true; do
        check_once
        sleep 10
    done
else
    if check_once; then
        echo ""
        echo "================================================================"
        echo " Setup verified end to end: Binance -> sequencer -> Logos"
        echo " blockchain -> indexer -> oracle_prices contract on LEZ."
        echo "================================================================"
    else
        exit 1
    fi
fi
