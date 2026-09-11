#!/usr/bin/env bash
#
# Terminal 2: Logos blockchain (bedrock node only)
# ---------------------------------------------------
# Run this in its own terminal and leave it running. Ctrl+C stops it and runs
# `docker compose down`.
#
# IMPORTANT - why only ONE service:
#   The all-in-one compose file bundles four services. Three of them
#   (sequencer_service, indexer_service, explorer_service, plus their
#   risc0_base builder image) are LEZ's own services that get BUILT LOCALLY.
#   We don't need any of them:
#     - sequencer_service  -> that's LEZ, which we run natively in terminal 1
#                             (and it would fight over port 3040)
#     - indexer_service    -> LEZ's own indexer, only used by the explorer UI
#     - explorer_service   -> web UI, not used by the oracle
#     - risc0_base         -> builder image for the two above; on Apple
#                             Silicon its Dockerfile falls into a
#                             build-RISC0-from-source path that takes ~15 min
#                             and then fails in cc-rs/C++
#   The oracle only ever talks to the bedrock node on port 18080, and
#   logos-blockchain-node-0 uses a PRE-BUILT ghcr.io image with no depends_on,
#   so starting just that one service builds nothing at all.
#
# What this script does:
#   1) Patches genesis_time in bedrock/deployment-settings.yaml to 'now'
#      (it's hardcoded to a fixed past date; if left stale the zone-sdk tries
#      to backfill millions of slots and never finishes)
#   2) Resets docker volumes, so the chain matches the fresh genesis
#   3) Starts only logos-blockchain-node-0
#
# Usage:
#   ./2_start_logos_blockchain.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"
LEZ_DIR="$PROJECT_ROOT/logos-execution-zone"
SETTINGS="$LEZ_DIR/bedrock/deployment-settings.yaml"

# Only the bedrock node. See the comment block above.
DOCKER_SERVICE="logos-blockchain-node-0"

[ -d "$LEZ_DIR" ]  || { echo "ERROR: not found: $LEZ_DIR"; exit 1; }
[ -f "$SETTINGS" ] || { echo "ERROR: not found: $SETTINGS"; exit 1; }

cleanup() {
    echo ""
    echo ">> Shutting down Logos blockchain..."
    (cd "$LEZ_DIR" && docker compose down >/dev/null 2>&1) || true
    exit 0
}
trap cleanup INT TERM

if ! docker info >/dev/null 2>&1; then
    echo "ERROR: Docker isn't running. Open Docker Desktop and try again."
    exit 1
fi

echo ">> Patching genesis time to 'now'..."
python3 - "$SETTINGS" <<'PYEOF'
import re, sys, time

path = sys.argv[1]
with open(path) as f:
    content = f.read()

# The genesis channel inscription is a hex blob laid out as:
#   [0:8]   chain_id_len (u64 LE)
#   [8:20]  chain_id ("logos-devnet", 12 bytes)
#   [20:28] genesis_time (u64 LE, unix seconds)   <- the part we rewrite
#   [28:60] epoch_nonce (32 zero bytes)
matches = re.findall(r"inscription: '([0-9a-f]+)'", content)
target = None
for h in matches:
    b = bytes.fromhex(h)
    if len(b) >= 28 and int.from_bytes(b[0:8], "little") == 12 and b[8:20] == b"logos-devnet":
        target = h
        break

if target is None:
    print("   WARNING: genesis inscription not recognised, skipping patch.")
    print("   (If the chain later reports a huge current_slot, this is why.)")
    sys.exit(0)

b = bytearray.fromhex(target)
old_time = int.from_bytes(b[20:28], "little")
new_time = int(time.time()) - 30
b[20:28] = new_time.to_bytes(8, "little")
content = content.replace(target, bytes(b).hex())
with open(path, "w") as f:
    f.write(content)
print(f"   genesis_time: {old_time} -> {new_time} (unix seconds)")
PYEOF

echo ""
echo ">> Resetting docker volumes for a fresh chain..."
cd "$LEZ_DIR" || { echo "ERROR: cannot cd to $LEZ_DIR"; exit 1; }
docker compose down -v >/dev/null 2>&1 || true

echo ""
echo ">> Starting $DOCKER_SERVICE (REST API on port 18080). Leave this terminal open."
echo ">> Ctrl+C here stops it."
echo ""

docker compose up "$DOCKER_SERVICE"
