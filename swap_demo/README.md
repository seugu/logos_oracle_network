# swap_demo

A SPEL program built with [spel-framework](https://github.com/logos-co/spel).

## Prerequisites

- Rust + [risc0 toolchain](https://dev.risczero.com/api/zkvm/install)
- [LSSA wallet CLI](https://github.com/logos-blockchain/lssa) (`wallet` binary)
- A running sequencer

## Quick Start

```bash
# 1. Build the guest binary
make build

# 2. Generate the IDL (auto-extracts from #[lez_program] annotations)
make idl

# 3. Deploy to sequencer
make deploy

# 4. See available commands (auto-generated from your program)
make cli ARGS="--help"

# 5. Run an instruction (spel.toml provides IDL and binary paths)
make cli ARGS="<command> --arg1 value1 --arg2 value2"

# Dry run (no submission):
make cli ARGS="--dry-run -- <command> --arg1 value1"
```

## Make Targets

| Target | Description |
|--------|-------------|
| `make all` | Full build: guest binary → IDL → FFI → UI scaffold → UI app |
| `make build` | Build the guest binary (risc0) |
| `make idl` | Generate IDL JSON from program source |
| `make cli ARGS="..."` | Run the IDL-driven CLI |
| `make deploy` | Deploy program to sequencer |
| `make inspect` | Show ProgramId for built binary |
| `make setup` | Create accounts via wallet |
| `make status` | Show saved state and binary info |
| `make clean` | Remove saved state |
| `make ffi-gen` | Generate FFI Rust source from IDL |
| `make ffi` | Build FFI shared library (.so) |
| `make ui-gen` | Generate Qt/QML Basecamp module scaffold (first run, overwrites all) |
| `make ui-regen` | Regenerate C++ backend + build files; keep hand-written `qml/Main.qml` |
| `make ui-build` | Build the Qt/QML standalone preview app |
| `make ui-run` | Run the standalone preview app |
| `make install` | Install plugin to Basecamp plugins directory |
| `make lgx` | Build a portable LGX archive for distribution |
| `make lgx-sign` | Sign LGX with a dev key (`lgx keygen --name devkey` first) |
| `python3 scripts/install_lgx.py <f.lgx>` | Direct install (bypasses Basecamp UI) |

## Project Structure

```
swap_demo/
├── swap_demo_core/    # Shared types (used by guest + host)
│   └── src/lib.rs
├── swap_demo_ffi/     # C FFI cdylib (compiled to .so for Qt)
│   ├── src/lib.rs        # includes generated/ at build time
│   └── generated/        # populated by `make ffi-gen` (git-ignored)
├── methods/
│   └── guest/            # RISC Zero guest program (runs on-chain)
│       └── src/bin/swap_demo.rs
├── examples/             # CLI tools
│   └── src/bin/
│       ├── generate_idl.rs    # One-liner IDL generator
│       └── swap_demo_cli.rs # Three-line CLI wrapper
├── spel.toml                         # SPEL CLI config (IDL and binary paths)
├── Makefile
└── swap_demo-idl.json       # Auto-generated IDL
```

## How It Works

The `#[lez_program]` macro in your guest binary defines your on-chain program.
The framework automatically:

1. **Generates an `Instruction` enum** from your function signatures
2. **Generates an IDL** (Interface Description Language) describing your program
3. **Provides a full CLI** for building, inspecting, and submitting transactions

You write the program logic. The framework handles the rest.


## Demo runbook (oracle-priced swap)

One deployment of this program serves **one trading pair**. `feed_id` is bound
to the pair in `initialize`, which is what stops a caller from pointing `swap`
at a different, more convenient feed.

Prerequisites: `oracle_prices` deployed + initialized, a feed initialized and a
price published to it; the LEZ token program deployed with two fungible token
definitions (base and quote).

```bash
# 0. build + regenerate the IDL (the checked-in one is stale after any
#    signature change — `swap` gained args and an account)
cargo run -p spel_lint -- methods/guest/src/bin/swap_demo.rs   # fast pre-flight
make build && make idl && make deploy

# 1. one-time setup: bind the pair and the feed
spel initialize \
  --base-definition  <BASE_TOKEN_DEFINITION_ACCOUNT> \
  --quote-definition <QUOTE_TOKEN_DEFINITION_ACCOUNT> \
  --token-program-id  0,0,... \
  --oracle-program-id 0,0,... \
  --feed-id 0000...0001

# 2. create one pool per side. The pool address is a PDA of this program
#    derived from the token definition; the seed is recomputed inside the
#    guest, so it is NOT passed on the command line any more.
cargo run -p pda_seed_tool -- <BASE_TOKEN_DEFINITION_ACCOUNT>   # prints the address
spel initialize-pool \
  --token-definition-account <BASE_TOKEN_DEFINITION_ACCOUNT> \
  --pool-account <PRINTED_POOL_PDA>
# ... repeat for the quote side

# 3. fund the pools with a plain token transfer (they are ordinary holdings)

# 4. swap
spel swap \
  --price-feed <FEED_PDA> \
  --from <USER_BASE_HOLDING>   \
  --to   <USER_QUOTE_HOLDING>  \
  --pool-in  <BASE_POOL_PDA>   \
  --pool-out <QUOTE_POOL_PDA>  \
  --amount-in 10 --min-amount-out 24000 --min-round 1000 --base-to-quote true
```

### What this demo is not

* **Not an AMM.** The price comes entirely from the oracle, so the pool carries
  all the inventory risk and can be drained at a stale price. `--min-round` is
  a caller-side freshness assertion, not a protocol-level staleness bound —
  there is no block height inside a SPEL program.
* **`oracle_prices` publishing is still permissionless** and `round` is still
  unconstrained. Until `publish_price` checks the caller against
  `oracle_register`, anyone can set the price this contract trades on.
* **Single pair per deployment.** Multi-pair needs a per-pair PDA holding
  `(feed_id, base_definition, quote_definition)` instead of the singleton state.
* **`to` must already be initialized.** Initializing it here would need the
  caller to authorize it separately.
