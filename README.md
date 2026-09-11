# LON Oracle — macOS Local Setup

This repo is forked from the original [sydhds/logos_oracle_network](https://github.com/sydhds/logos_oracle_network). This `dogfooding` branch only adds scripts for an easy local setup on macOS — no other changes to the upstream project's intent.

## Clone

```bash
git clone -b dogfooding https://github.com/seugu/logos_oracle_network.git
cd logos_oracle_network
chmod +x *.sh
```

## Check prerequisites

```bash
./check_prereqs_mac.sh
```

## Build toolchain

```bash
./0_build_toolchain.sh
```

Open **three terminals**, all `cd`'d into this same directory.

## Terminal 1 — LEZ

```bash
./1_start_lez.sh
```

## Terminal 2 — Logos blockchain

```bash
./2_start_logos_blockchain.sh
```

## Terminal 3 — deploy (once)

```bash
./3_bootstrap_deploy.sh
```

## Terminal 3 — run (every session)

```bash
./4_run_sequencer_indexer.sh
```

## Verify — check the live price

In a fourth terminal:

```bash
./5_check_price.sh
```

Add `--watch` to poll every 10 seconds:

```bash
./5_check_price.sh --watch
```

## Stopping

`Ctrl+C` in each terminal stops that terminal's process (terminal 3 stops both sequencer and indexer).

## Fresh restart

Terminals 1 and 2 can just be re-run. LEZ keeps its state in `logos-execution-zone/rocksdb`; Logos blockchain resets every time terminal 2 runs. `3_bootstrap_deploy.sh` doesn't need to run again as long as LEZ's state is intact.

## Wipe everything

```bash
pkill -f sequencer_service
rm -rf logos-execution-zone/rocksdb
rm -rf ~/.lee
rm -rf ~/local_run ~/local_target
cd logos-execution-zone && docker compose down -v && cd ..
```

## Notes

- `spel/`, `logos-execution-zone/`, `lez-programs/`, `oracle_node/logos-blockchain/` are external dependencies, not part of this repo's history — `0_build_toolchain.sh` clones them automatically.
- All scripts are idempotent and safe to re-run after a partial failure.
- Fixes included in this branch: BTC/USDT feed wiring, correct node REST port (18080), and an `indexer` bug where `feed_price` was sent as `AccountId::default()` instead of the computed PDA, causing every `publish_price` call to be silently rejected on-chain (`PdaMismatch`) even though the indexer logged success.
