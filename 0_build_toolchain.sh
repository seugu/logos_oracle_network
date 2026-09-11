#!/usr/bin/env bash
#
# Step 0 (run once, any terminal): fetch dependencies, build spel / wallet /
# LEZ sequencer_service
# -------------------------------------------------------------------------------
# Run this once before opening the three terminals. Safe to re-run: it only
# clones/builds what's missing, everything else finishes in seconds.
#
# Usage:
#   ./0_build_toolchain.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"
LEZ_DIR="$PROJECT_ROOT/logos-execution-zone"
SPEL_DIR="$PROJECT_ROOT/spel"
LEZ_PROGRAMS_DIR="$PROJECT_ROOT/lez-programs"
ORACLE_NODE_DIR="$PROJECT_ROOT/oracle_node"

# Pinned so the version of logos-blockchain matches what oracle_node's
# Cargo.toml actually expects (the project's own git submodule reference).
LOGOS_BLOCKCHAIN_PINNED_COMMIT="8784b837c558b037bf691b0cb720d1c0c20db245"

say()  { echo ""; echo ">> $1"; }
fail() { echo "ERROR: $1"; exit 1; }

[ -d "$ORACLE_NODE_DIR" ] || fail "not found: $ORACLE_NODE_DIR (run this from the repo root)"
command -v git >/dev/null 2>&1 || fail "git not found."

# ---------------------------------------------------------------------------
# spel, logos-execution-zone, lez-programs, and logos-blockchain are NOT part
# of this project's own git history - they're external dependencies this
# project builds against. On a fresh `git clone` of this repo they won't be
# present yet, so fetch them here. Nothing here ever gets committed (see
# .gitignore).
say "[1/6] Fetching vendored dependencies"

clone_if_missing() {
    # $1 = target dir, $2 = repo url, $3... = extra git-clone args
    local dir="$1" url="$2"
    shift 2
    if [ -d "$dir" ] && [ -n "$(ls -A "$dir" 2>/dev/null)" ]; then
        echo "   (already present) $dir"
        return
    fi
    rm -rf "$dir"
    git clone "$@" "$url" "$dir"
}

clone_if_missing "$SPEL_DIR" https://github.com/logos-co/spel.git --depth 1 --branch v0.6.0
clone_if_missing "$LEZ_DIR"  https://github.com/logos-blockchain/logos-execution-zone.git --depth 1 --branch v0.2.0
clone_if_missing "$LEZ_PROGRAMS_DIR" https://github.com/logos-blockchain/lez-programs.git --depth 1

LOGOS_BLOCKCHAIN_DIR="$ORACLE_NODE_DIR/logos-blockchain"
if [ -d "$LOGOS_BLOCKCHAIN_DIR" ] && [ -n "$(ls -A "$LOGOS_BLOCKCHAIN_DIR" 2>/dev/null)" ]; then
    echo "   (already present) $LOGOS_BLOCKCHAIN_DIR"
else
    rm -rf "$LOGOS_BLOCKCHAIN_DIR"
    git init -q "$LOGOS_BLOCKCHAIN_DIR"
    (cd "$LOGOS_BLOCKCHAIN_DIR" \
        && git remote add origin https://github.com/logos-blockchain/logos-blockchain.git \
        && git fetch --depth 1 origin "$LOGOS_BLOCKCHAIN_PINNED_COMMIT" \
        && git checkout -q FETCH_HEAD)
    echo "   cloned logos-blockchain @ $LOGOS_BLOCKCHAIN_PINNED_COMMIT"
fi

# ---------------------------------------------------------------------------
say "[2/6] Building spel / wallet / LEZ sequencer_service (may take a few minutes)"

# spel depends on pyo3, which on macOS can try to link against the Python
# bundled inside Xcode and fail with "library 'python3.9' not found".
# Point it at a real Homebrew Python instead.
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
    if [ -n "$PY" ]; then
        export PYO3_PYTHON="$PY"
        echo "   PYO3_PYTHON=$PYO3_PYTHON"
    fi
fi

cd "$SPEL_DIR"
cargo build -p spel-framework -p spel-framework-core -p spel-framework-macros -p spel-client-gen -p spel

cd "$LEZ_DIR"
cargo build --release --features standalone -p sequencer_service
cargo build --release -p wallet

export PATH="$HOME/.risc0/bin:$LEZ_DIR/target/release:$SPEL_DIR/target/debug:$PATH"
command -v wallet >/dev/null 2>&1 || fail "wallet not found on PATH after build."
command -v spel   >/dev/null 2>&1 || fail "spel not found on PATH after build."

echo ""
echo "================================================================"
echo " Toolchain ready. Next steps:"
echo "   Terminal 1: ./1_start_lez.sh"
echo "   Terminal 2: ./2_start_logos_blockchain.sh"
echo "   Terminal 3: ./3_bootstrap_deploy.sh        (one-time deploy)"
echo "               then ./4_run_sequencer_indexer.sh  (every session)"
echo "================================================================"
