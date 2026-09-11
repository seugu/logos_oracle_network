#!/usr/bin/env bash
#
# Terminal 1: native LEZ (standalone)
# -------------------------------------
# Run this in its own terminal and leave it running. Ctrl+C here stops LEZ.
# LEZ listens on port 3040 and keeps its chain state in
# logos-execution-zone/rocksdb (delete that directory for a clean chain).
#
# Usage:
#   ./1_start_lez.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"
LEZ_DIR="$PROJECT_ROOT/logos-execution-zone"

[ -d "$LEZ_DIR" ] || { echo "ERROR: not found: $LEZ_DIR"; exit 1; }

# Refuse to start if something already holds LEZ's port, otherwise you get a
# confusing half-broken setup.
if command -v lsof >/dev/null 2>&1 && lsof -nP -iTCP:3040 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "ERROR: port 3040 is already in use. Another LEZ is probably running."
    echo "       Inspect with: lsof -nP -iTCP:3040 -sTCP:LISTEN"
    echo "       Stop it with: pkill -f sequencer_service"
    exit 1
fi

echo ">> Starting native LEZ (standalone) on port 3040. Leave this terminal open."
echo ">> Ctrl+C here stops LEZ."
echo ""

cd "$LEZ_DIR" || { echo "ERROR: cannot cd to $LEZ_DIR"; exit 1; }

exec env RUST_LOG=info cargo run --release --features standalone -p sequencer_service -- \
    lez/sequencer/service/configs/debug/sequencer_config.json
