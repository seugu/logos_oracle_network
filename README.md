# LON Oracle — Local macOS Setup

A local, end-to-end deployment of the LON oracle stack: LEZ (standalone) +
Logos blockchain (docker) + `oracle_register` + `oracle_prices` + the
`sequencer`/`indexer` off-chain nodes, publishing a live BTC/USDT price.

## What's in here

This repo's own code (`oracle_register/`, `oracle_prices/`, `oracle_node/`,
etc.) lives at the root, same as any other clone. Alongside it:

```
check_prereqs_mac.sh         step -1: verify/install required tools
0_build_toolchain.sh         step 0:  fetch dependencies, build spel / wallet /
                                       LEZ sequencer_service
1_start_lez.sh               step 1:  terminal 1 — native LEZ
2_start_logos_blockchain.sh  step 2:  terminal 2 — Logos blockchain (docker)
3_bootstrap_deploy.sh        step 3:  terminal 3 — one-time: accounts + deploy
4_run_sequencer_indexer.sh   step 4:  terminal 3 — every session: run the nodes
5_check_price.sh             step 5:  verify: read the live price back from LEZ
```

`spel/`, `logos-execution-zone/`, `lez-programs/`, and
`oracle_node/logos-blockchain/` are external dependencies this project
builds against, not part of this repo's own history — `0_build_toolchain.sh`
clones them automatically on first run, straight into the repo root (see
`.gitignore`).

## Setup, in order

```bash
git clone <this repo>
cd <this repo>
chmod +x *.sh

./check_prereqs_mac.sh      # checks Homebrew, Rust, protoc, Docker, RISC0
./0_build_toolchain.sh      # clones spel/logos-execution-zone/lez-programs/
                             # logos-blockchain, then builds spel/wallet/LEZ
```

Then open **three terminals**, all `cd`'d into this same directory:

| Terminal | Command | What it does |
|---|---|---|
| 1 | `./1_start_lez.sh` | Native LEZ (standalone), foreground. Leave running. |
| 2 | `./2_start_logos_blockchain.sh` | Logos blockchain (docker), foreground. Leave running. |
| 3 | `./3_bootstrap_deploy.sh` *(once)*, then `./4_run_sequencer_indexer.sh` | Wallet accounts, token, `oracle_register`, `oracle_prices` deploy — then the sequencer (background) and indexer (foreground). |

`3_bootstrap_deploy.sh` only needs terminal 1 (LEZ) to be up; it does **not**
need terminal 2 yet. Start terminal 2 before running
`4_run_sequencer_indexer.sh`, since that one needs both LEZ and Logos
blockchain.

`3_bootstrap_deploy.sh` is idempotent — every on-chain step checks whether
it already happened before doing it again, so re-running it after a partial
failure is safe.

## Confirming it worked

Once terminal 3 shows lines like:

```
[Feed BTC/USDT] Attested round N: Price ..., Count 1
Successfully published attested price to oracle pices contract :-) :-D !!!
```

open a fourth terminal and run:

```bash
./5_check_price.sh
```

Expected output:

```
[HH:MM:SS] BTC/USDT = $77,175.96   (round 1705, 1 observation(s))

================================================================
 Setup verified end to end: Binance -> sequencer -> Logos
 blockchain -> indexer -> oracle_prices contract on LEZ.
================================================================
```

Add `--watch` to poll every 10 seconds instead of checking once.

## Stopping / restarting

- `Ctrl+C` in terminal 3 stops both sequencer and indexer.
- `Ctrl+C` in terminal 2 stops Logos blockchain (`docker compose down`).
- `Ctrl+C` in terminal 1 stops LEZ.

To start a fresh session later, terminals 1–2 can simply be re-run — LEZ
keeps its chain state (and your registered oracle node) in
`logos-execution-zone/rocksdb`; Logos blockchain resets every time
`2_start_logos_blockchain.sh` runs (it patches genesis to "now" and drops
docker volumes, since a stale genesis makes the chain unusable). Once both
are up, terminal 3 only needs `./4_run_sequencer_indexer.sh` — the one-time
`3_bootstrap_deploy.sh` doesn't need to run again as long as LEZ's state is
intact.

To wipe everything and start completely clean:

```bash
pkill -f sequencer_service
rm -rf logos-execution-zone/rocksdb
rm -rf ~/.lee
rm -rf ~/local_run ~/local_target
cd logos-execution-zone && docker compose down -v
```

## Known environment quirks (already handled by these scripts)

- **PyO3 / Xcode Python** — `spel`/`wallet` depend on `pyo3`, which on macOS
  can try to link against the Python bundled inside Xcode and fail with
  `library 'python3.9' not found`. The scripts auto-detect a Homebrew Python
  and set `PYO3_PYTHON`.
- **Docker on Apple Silicon** — the all-in-one `docker compose` file bundles
  services LEZ builds locally (`indexer_service`, `explorer_service`,
  `risc0_base`); on Apple Silicon the `risc0_base` build falls into a
  build-from-source path that takes ~15 minutes and then fails. The oracle
  only needs `logos-blockchain-node-0` (a pre-built image), so
  `2_start_logos_blockchain.sh` starts only that one service.
- **Genesis time** — `bedrock/deployment-settings.yaml` ships with a fixed
  past genesis time. If left stale, the sequencer tries to backfill millions
  of slots and never finishes. `2_start_logos_blockchain.sh` patches it to
  "now" on every run.
- **`wallet check-health` on a fresh keystore** — prompts for a password on
  stdin; if piped/backgrounded carelessly this hangs forever. The scripts
  poll the TCP port directly instead of calling this command in a loop.
- **No GNU `timeout` on macOS** — used for a short probe run of `sequencer`
  to read its generated channel id. The scripts fall back to `gtimeout` or a
  background-kill shim if `timeout` isn't installed.
- **`indexer` never filled in `feed_price`** — `oracle_node/indexer/src/prices_contract.rs`
  sent `AccountId::default()` instead of computing the real PDA, so every
  `publish_price` call was silently rejected on-chain with
  `PdaMismatch` even though the indexer logged "Successfully published".
  Fixed by calling `oracle_prices_client::compute_feed_price_pda(...)`.
- **`oracle_node/logos-blockchain` pin** — this submodule's exact commit
  wasn't discoverable from a plain zip export of the project, so
  `0_build_toolchain.sh` pins it explicitly
  (`LOGOS_BLOCKCHAIN_PINNED_COMMIT`). If `oracle_node` ever fails to build
  with a manifest/dependency error mentioning `logos-blockchain`, that pin is
  the first thing to check against whatever commit the project's own
  `.gitmodules`/CI actually expects.
