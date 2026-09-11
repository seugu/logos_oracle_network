#!/usr/bin/env bash
#
# Terminal 3, step 1 (one-time): wallet accounts + token + oracle_register +
# oracle_prices deploy
# -----------------------------------------------------------------------------
# Run this AFTER 0_build_toolchain.sh, and while 1_start_lez.sh (terminal 1)
# is running. Does NOT need Logos blockchain (terminal 2) to be up.
#
# Every on-chain step is idempotent: it checks whether the thing already
# exists before creating it, so a re-run after a failure is safe.
#
# When done: run ./4_run_sequencer_indexer.sh in this same terminal.
#
# Usage:
#   ./3_bootstrap_deploy.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"
LEZ_DIR="$PROJECT_ROOT/logos-execution-zone"
SPEL_DIR="$PROJECT_ROOT/spel"
LEZ_PROGRAMS_DIR="$PROJECT_ROOT/lez-programs"
ORACLE_NODE_DIR="$PROJECT_ROOT/oracle_node"
ORACLE_REGISTER_DIR="$PROJECT_ROOT/oracle_register"
ORACLE_PRICES_DIR="$PROJECT_ROOT/oracle_prices"

DATA_FOLDER="$HOME/local_run/oracle_node/sequencer"
NODE_URL="http://localhost:18080"
LEZ_ADDR="http://127.0.0.1:3040"
WALLET_PW="localdev"
LON_STAKE_HEADROOM=100
FEED_NAME="BTC/USDT"

# Separate target dirs so the three cargo workspaces don't stomp on each other.
TARGET_ORACLE_NODE="$HOME/local_target/oracle_node"
TARGET_ORACLE_REGISTER="$HOME/local_target/oracle_register"
TARGET_ORACLE_PRICES="$HOME/local_target/oracle_prices"

export PATH="$HOME/.risc0/bin:$LEZ_DIR/target/release:$SPEL_DIR/target/debug:$PATH"
mkdir -p "$DATA_FOLDER"

# spel/wallet (pulled in by oracle_register's own cargo build) depend on
# pyo3, which on macOS can try to link against the Python bundled inside
# Xcode and fail with "library 'python3.9' not found". Point it at a real
# Homebrew Python instead.
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
        echo ">> PYO3_PYTHON=$PYO3_PYTHON"
    fi
fi

say()  { echo ""; echo ">> $1"; }
fail() { echo "ERROR: $1"; exit 1; }

# macOS has no GNU `timeout`. Use it if present (or gtimeout from coreutils),
# otherwise fall back to a background-kill shim.
run_limited() {
    local secs="$1"; shift
    if command -v timeout >/dev/null 2>&1; then
        timeout "$secs" "$@" || true
    elif command -v gtimeout >/dev/null 2>&1; then
        gtimeout "$secs" "$@" || true
    else
        "$@" &
        local pid=$!
        ( sleep "$secs"; kill "$pid" >/dev/null 2>&1 ) &
        local killer=$!
        wait "$pid" >/dev/null 2>&1 || true
        kill "$killer" >/dev/null 2>&1 || true
    fi
}

extract_pubkey() {
    grep -oE 'Public/[1-9A-HJ-NP-Za-km-z]+' | head -1 | cut -d/ -f2
}

# On a fresh ~/.lee every wallet command prompts "Input password:" on stdin.
# Feed it from a pipe so nothing ever blocks waiting for a human.
wallet_q() {
    wallet "$@" < <(yes "$WALLET_PW" 2>/dev/null)
}

wallet_new() {
    # $1 = label. Idempotent: reuses an existing labeled account if present.
    local label="$1" existing
    existing=$(wallet_q account list 2>/dev/null | grep -F "[$label]" \
        | grep -oE 'Public/[1-9A-HJ-NP-Za-km-z]+' | head -1 | cut -d/ -f2 || true)
    if [ -n "$existing" ]; then
        echo "   (already exists) $label -> $existing" >&2
        echo "$existing"
        return
    fi
    wallet_q account new public --label "$label" | tee /dev/stderr | extract_pubkey
}

# "Account data is empty" is what spel prints for an account that doesn't
# exist yet, so it's our "not created" sentinel.
account_is_empty() {
    # $1 = account id, $2 = idl path, $3 = type name
    local out
    out=$(spel inspect "$1" --idl "$2" --type "$3" 2>&1 || true)
    [ -z "$1" ] && return 0
    echo "$out" | grep -q "Account data is empty"
}

command -v wallet >/dev/null 2>&1 || fail "wallet not found on PATH. Run ./0_build_toolchain.sh first."
command -v spel   >/dev/null 2>&1 || fail "spel not found on PATH. Run ./0_build_toolchain.sh first."
command -v cargo  >/dev/null 2>&1 || fail "cargo not found on PATH."
command -v cargo-risczero >/dev/null 2>&1 || cargo risczero --version >/dev/null 2>&1 \
    || fail "cargo risczero not available. Run ./check_prereqs_mac.sh."

# ---------------------------------------------------------------------------
say "[1/5] Waiting for LEZ (terminal 1) to be ready"
# Poll the TCP port directly. Do NOT use `wallet check-health` here: on a
# fresh ~/.lee it prompts for a password and would hang forever.
ready=0
for _ in $(seq 1 60); do
    if curl -s -o /dev/null --max-time 2 "$LEZ_ADDR" 2>/dev/null; then
        ready=1
        break
    fi
    sleep 2
done
[ "$ready" -eq 1 ] || fail "LEZ isn't reachable on $LEZ_ADDR after 2 minutes. Is ./1_start_lez.sh running in another terminal?"
echo "   LEZ is up on $LEZ_ADDR."

# ---------------------------------------------------------------------------
say "[2/5] Creating wallet accounts"

OWNER=$(wallet_new owner)
TOKEN_DEF=$(wallet_new lon_token_def_account)
TOKEN_HOLD=$(wallet_new lon_token_hold_account)
ORACLE_FUNDING=$(wallet_new oracle_funding)

for v in OWNER TOKEN_DEF TOKEN_HOLD ORACLE_FUNDING; do
    [ -n "${!v}" ] || fail "$v account was not created, check the wallet output above."
done

echo "   owner:                  $OWNER"
echo "   lon_token_def_account:  $TOKEN_DEF"
echo "   lon_token_hold_account: $TOKEN_HOLD"
echo "   oracle_funding:         $ORACLE_FUNDING"

# ---------------------------------------------------------------------------
say "[3/5] Deploying the LON token program"

cd "$LEZ_PROGRAMS_DIR" || fail "cannot cd to $LEZ_PROGRAMS_DIR"
mkdir -p artifacts
cargo risczero build --manifest-path ./programs/token/methods/guest/Cargo.toml

TOKEN_BIN="programs/token/methods/guest/target/riscv32im-risc0-zkvm-elf/docker/token.bin"
[ -f "$TOKEN_BIN" ] || fail "token.bin not found: $TOKEN_BIN"

wallet_q deploy-program "$TOKEN_BIN" || echo "   (already deployed, continuing)"
spel generate-idl programs/token/methods/guest/src/bin/token.rs > artifacts/token-idl.json
TOKEN_IDL="$LEZ_PROGRAMS_DIR/artifacts/token-idl.json"

TOKEN_PROGRAM_HEX=$(spel program-id "$TOKEN_BIN" | grep "ImageID (hex bytes):" | awk -F': ' '{print $2}' | tr -d ' ')
[ -n "$TOKEN_PROGRAM_HEX" ] || fail "Could not extract the token program id."
echo "   token program hex: $TOKEN_PROGRAM_HEX"

if account_is_empty "$TOKEN_DEF" "$TOKEN_IDL" TokenDefinition; then
    spel --idl "$TOKEN_IDL" -p "$TOKEN_BIN" -- new-fungible-definition \
        --name "LON" --total-supply 21000 \
        --definition-target-account "$TOKEN_DEF" \
        --holding-target-account "$TOKEN_HOLD" \
        --mint-authority none
else
    echo "   (LON token already defined, skipping)"
fi

if account_is_empty "$ORACLE_FUNDING" "$TOKEN_IDL" TokenHolding; then
    spel --idl "$TOKEN_IDL" -p "$TOKEN_BIN" -- initialize-account \
        --account-to-initialize "$ORACLE_FUNDING" \
        --definition-account "$TOKEN_DEF"
    spel --idl "$TOKEN_IDL" -p "$TOKEN_BIN" -- transfer \
        --sender "$TOKEN_HOLD" \
        --recipient "$ORACLE_FUNDING" \
        --amount-to-transfer "$LON_STAKE_HEADROOM"
    echo "   Funded oracle_funding with $LON_STAKE_HEADROOM LON."
else
    echo "   (oracle_funding already initialized, skipping)"
fi

# ---------------------------------------------------------------------------
say "[4/5] Building, deploying, initializing oracle_register"

cd "$ORACLE_REGISTER_DIR" || fail "cannot cd to $ORACLE_REGISTER_DIR"
CARGO_TARGET_DIR="$TARGET_ORACLE_REGISTER" RISC0_USE_DOCKER=0 cargo build -j 8 --release

OR_BIN="methods/guest/target/riscv32im-risc0-zkvm-elf/docker/oracle_register.bin"
mkdir -p "$(dirname "$OR_BIN")"
cp -v "$TARGET_ORACLE_REGISTER/riscv-guest/oracle_register-methods/oracle_register-guest/riscv32im-risc0-zkvm-elf/release/oracle_register.bin" "$OR_BIN"

spel generate-idl methods/guest/src/bin/oracle_register.rs > oracle_register-idl.json
OR_IDL="$ORACLE_REGISTER_DIR/oracle_register-idl.json"
make deploy || echo "   (already deployed, continuing)"

OR_PROGRAM_HEX=$(spel program-id "$OR_BIN" | grep "ImageID (hex bytes):" | awk -F': ' '{print $2}' | tr -d ' ')
[ -n "$OR_PROGRAM_HEX" ] || fail "Could not extract the oracle_register program id."
echo "   oracle_register program hex: $OR_PROGRAM_HEX"

# Convert the token program id into the [u32; 8] form `initialize` expects.
cd "$ORACLE_NODE_DIR" || fail "cannot cd to $ORACLE_NODE_DIR"
TOKEN_PROGRAM_ARRAY=$(CARGO_TARGET_DIR="$TARGET_ORACLE_NODE" \
    cargo run -q -p common --example print_program_id -- "$TOKEN_PROGRAM_HEX" \
    | grep "hex words:" | sed -E 's/.*\[(.*)\]/\1/' | tr -d ' ')
[ -n "$TOKEN_PROGRAM_ARRAY" ] || fail "Could not convert the token program id to [u32;8]."
echo "   token program [u32;8]: $TOKEN_PROGRAM_ARRAY"

cd "$ORACLE_REGISTER_DIR" || fail "cannot cd to $ORACLE_REGISTER_DIR"
OR_ACCOUNT=$(spel pda register --idl "$OR_IDL" 2>/dev/null | tail -1 | tr -d '[:space:]' || true)
if account_is_empty "$OR_ACCOUNT" "$OR_IDL" RegisterState; then
    OR_INIT_OUT=$(spel --idl "$OR_IDL" -p "$OR_BIN" -- initialize \
        --owner "$OWNER" \
        --token-program-id "$TOKEN_PROGRAM_ARRAY")
    echo "$OR_INIT_OUT"
    OR_ACCOUNT=$(echo "$OR_INIT_OUT" | grep "register →" | awk '{print $4}')
else
    echo "   (oracle_register already initialized, skipping)"
fi
[ -n "$OR_ACCOUNT" ] || fail "Could not determine the oracle_register PDA."
echo "   oracle_register PDA: $OR_ACCOUNT"

say "   Generating the node's own key (channel id) via a short probe run..."
cd "$ORACLE_NODE_DIR" || fail "cannot cd to $ORACLE_NODE_DIR"
CARGO_TARGET_DIR="$TARGET_ORACLE_NODE" cargo build -p sequencer -p indexer
SEQ_BIN="$TARGET_ORACLE_NODE/debug/sequencer"
[ -x "$SEQ_BIN" ] || fail "sequencer binary not found at $SEQ_BIN"

run_limited 10 "$SEQ_BIN" \
    --data-folder "$DATA_FOLDER" \
    --node-url "$NODE_URL" \
    --node-rest-url "$NODE_URL" \
    > /tmp/seq_channel_probe.log 2>&1

CHANNEL_ID=$(grep -m1 -oE 'Sequence channel id: [0-9a-f]{64}' /tmp/seq_channel_probe.log | awk '{print $NF}')
[ -n "$CHANNEL_ID" ] || fail "Could not read the channel id. See /tmp/seq_channel_probe.log."
echo "   Channel id (oracle node id): $CHANNEL_ID"

cd "$ORACLE_REGISTER_DIR/oracle_helper_1" || fail "cannot cd to oracle_helper_1"
HELPER_OUT=$(CARGO_TARGET_DIR="$TARGET_ORACLE_REGISTER" cargo run -q "$CHANNEL_ID")
echo "$HELPER_OUT"
VAULT_PDA=$(echo "$HELPER_OUT"  | grep "compute vault pda:"        | awk '{print $NF}')
VAULT_SEED=$(echo "$HELPER_OUT" | grep "vault pda seed bytes hex:" | sed -E 's/.*"([0-9a-f]+)".*/\1/')
[ -n "$VAULT_PDA" ]  || fail "Could not extract the vault PDA."
[ -n "$VAULT_SEED" ] || fail "Could not extract the vault seed."
echo "   vault PDA:  $VAULT_PDA"
echo "   vault seed: $VAULT_SEED"

cat > "$ORACLE_NODE_DIR/resources/register_contract_config.json" <<EOF
{
  "oracle_register_program_id": "$OR_PROGRAM_HEX",
  "oracle_node_id": "$CHANNEL_ID",
  "oracle_register_account": "$OR_ACCOUNT",
  "oracle_node_funding_account": "$ORACLE_FUNDING",
  "oracle_register_to": "$VAULT_PDA",
  "token_definition_account": "$TOKEN_DEF",
  "oracle_register_to_pda_seed": "$VAULT_SEED"
}
EOF
echo "   register_contract_config.json written."

# ---------------------------------------------------------------------------
say "[5/5] Building, deploying, initializing oracle_prices, opening the $FEED_NAME feed"

cd "$ORACLE_PRICES_DIR" || fail "cannot cd to $ORACLE_PRICES_DIR"
CARGO_TARGET_DIR="$TARGET_ORACLE_PRICES" RISC0_USE_DOCKER=0 cargo build -j 8 --release

OP_BIN="methods/guest/target/riscv32im-risc0-zkvm-elf/docker/oracle_prices.bin"
mkdir -p "$(dirname "$OP_BIN")"
cp -v "$TARGET_ORACLE_PRICES/riscv-guest/oracle_prices-methods/oracle_prices-guest/riscv32im-risc0-zkvm-elf/release/oracle_prices.bin" "$OP_BIN"

spel generate-idl methods/guest/src/bin/oracle_prices.rs > oracle_prices-idl.json
OP_IDL="$ORACLE_PRICES_DIR/oracle_prices-idl.json"
make deploy || echo "   (already deployed, continuing)"

OP_PROGRAM_HEX=$(spel program-id "$OP_BIN" | grep "ImageID (hex bytes):" | awk -F': ' '{print $2}' | tr -d ' ')
[ -n "$OP_PROGRAM_HEX" ] || fail "Could not extract the oracle_prices program id."
echo "   oracle_prices program hex: $OP_PROGRAM_HEX"

OP_ACCOUNT=$(spel pda oracle_prices_account --idl "$OP_IDL" 2>/dev/null | tail -1 | tr -d '[:space:]' || true)
if account_is_empty "$OP_ACCOUNT" "$OP_IDL" OraclePricesState; then
    spel --idl "$OP_IDL" -p "$OP_BIN" -- initialize
else
    echo "   (oracle_prices already initialized, skipping)"
fi

FEED_ID=$(python3 -c "import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest())" "$FEED_NAME")
echo "   $FEED_NAME feed_id: $FEED_ID"

FEED_PDA=$(spel pda feed_price --idl "$OP_IDL" --feed-id "$FEED_ID" 2>/dev/null | tail -1 | tr -d '[:space:]' || true)
if account_is_empty "$FEED_PDA" "$OP_IDL" PriceState; then
    OP_FEED_OUT=$(spel --idl "$OP_IDL" -p "$OP_BIN" -- initialize-feed --feed-id "$FEED_ID")
    echo "$OP_FEED_OUT"
    FEED_PDA=$(echo "$OP_FEED_OUT" | grep "feed_price →" | awk '{print $4}')
else
    echo "   ($FEED_NAME feed already open, skipping)"
fi
[ -n "$FEED_PDA" ] || fail "Could not determine the feed PDA."
echo "   $FEED_NAME feed PDA: $FEED_PDA"

cat > "$ORACLE_NODE_DIR/resources/prices_contract_config.json" <<EOF
{
  "oracle_prices_program_id": "$OP_PROGRAM_HEX"
}
EOF
echo "   prices_contract_config.json written."

# Save everything worth remembering so step 4 and manual queries can use it.
cat > "$SCRIPT_DIR/.deploy_info" <<EOF
OWNER=$OWNER
TOKEN_DEF=$TOKEN_DEF
TOKEN_HOLD=$TOKEN_HOLD
ORACLE_FUNDING=$ORACLE_FUNDING
CHANNEL_ID=$CHANNEL_ID
OR_PROGRAM_HEX=$OR_PROGRAM_HEX
OR_ACCOUNT=$OR_ACCOUNT
OP_PROGRAM_HEX=$OP_PROGRAM_HEX
FEED_NAME=$FEED_NAME
FEED_ID=$FEED_ID
FEED_PDA=$FEED_PDA
OP_IDL=$OP_IDL
EOF

# ---------------------------------------------------------------------------
say "Bootstrap complete."
echo ""
echo "================================================================"
echo " Summary (also saved to $SCRIPT_DIR/.deploy_info):"
echo "   owner:              $OWNER"
echo "   oracle_funding:     $ORACLE_FUNDING"
echo "   channel/oracle id:  $CHANNEL_ID"
echo "   oracle_register:    $OR_ACCOUNT  (program $OR_PROGRAM_HEX)"
echo "   oracle_prices:      program $OP_PROGRAM_HEX"
echo "   $FEED_NAME feed PDA:  $FEED_PDA"
echo ""
echo " Check the price at any time with:"
echo "   spel inspect $FEED_PDA --idl $OP_IDL --type PriceState"
echo ""
echo " Next: make sure ./2_start_logos_blockchain.sh (terminal 2) is running,"
echo " then in THIS terminal run:"
echo "   ./4_run_sequencer_indexer.sh"
echo "================================================================"
