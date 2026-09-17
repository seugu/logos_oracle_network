# swap_demo — oracle-priced swap: what changed and how to apply it

Demo only. Nothing here is meant to hold real value, and several gaps below are
left open deliberately because closing them is out of scope for a demo.

## Apply

Copy the files in this archive over the repo, then:

```bash
cd swap_demo
rm -f swap_demo-idl.json          # stale: `swap` and `initialize` both changed shape
cargo run -p spel_lint --manifest-path ../tools/spel_lint/Cargo.toml -- \
    methods/guest/src/bin/swap_demo.rs
make build && make idl && make ffi && make deploy
```

`swap_demo-idl.json` must be regenerated before `make ffi-gen` / `make ui-gen`
or the generated CLI/UI will emit calls with the old argument list.

## What was broken

| | |
|---|---|
| `swap` never read the price | `price_feed` was declared and never touched; `amount_eth`/`amount_usdt` were hard-coded 1 and 10 |
| `swap` ignored `amount` | the arg was in the signature and unused in the body |
| `swap` would panic before doing anything | it listed 5 accounts in `SpelOutput::execute` while declaring 6; the macro rewrites that into `execute_with_claims`, which `assert_eq!`s the lengths, and the runtime independently rejects a pre/post-state length mismatch |
| second transfer leg commented out | `// FIXME chained_call_transfer_2` — the user's token went into pool A and nothing came back |
| `is_authorized` flags inverted in `swap` | `to` was marked authorized and `pool_b` was not. The sender is `pool_b`, and it is the account delegated through `pda_seeds`. Marking `to` would be rejected as `InvalidAccountAuthorization`; leaving `pool_b` unmarked as `AuthorizedAccountMarkedAsNotAuthorized` |
| pool address was trusted, not verified | `pool_account` was passed raw with an off-chain-computed seed |
| no slippage, liquidity or freshness checks | |

The `is_authorized` assignment in `initialize_pool` was **not** a hack — the
runtime requires the flag to agree exactly with the `pda_seeds` delegation in
both directions (`validated_state_diff.rs`). That code was right.

`InconsistentAccountPreState` on `#[account(init, pda = ...)]` is a double
claim: this program claims the account in its own post-state, then the chained
`InitializeAccount` tries to claim the same account, whose `program_owner` in
the accumulated state diff no longer matches. Dropping `init` and keeping
`#[account(mut, pda = [...])]` keeps the address check and leaves the claim to
the token program. **This is the one inference that still needs a live
sequencer run to confirm.**

## What is new

* `swap_demo_core` — pure pricing + PDA seed derivation, dependency-free except
  `sha2`. 17 unit tests, all passing on the host.
* `oracle_prices_core` — `PriceState` hoisted out of the guest binary so
  consumers decode it instead of copying the layout. Byte-level wire-format
  test. The guest keeps its `#[account_type]` copy for IDL generation, with a
  `const _` conversion that turns any divergence into a compile error.
* `tools/spel_lint` — static checker for `#[lez_program]` sources. Catches the
  account-count, ordering, unused-arg and double-claim classes above in
  milliseconds instead of after a risc0 build. Non-zero exit for CI.
* `pda_seed_tool` — now derives from `swap_demo_core` rather than its own copy
  of the literal, so it cannot drift from the contract. Also: real usage/error
  messages instead of `unwrap()`, and edition 2021 like the rest.
* `Makefile` — `VARIANT` used a hard-coded `linux-` prefix while computing `OS`
  and never using it (wrong on macOS). `SPEL_CLIENT_GEN` defaulted to an
  absolute `/home/ubuntu/...` path; now resolved from `PATH` and overridable.

## Not verified

The guest was never compiled: this environment had no toolchain new enough for
risc0 3.0.5 / spel v0.6.0. Verified instead by reading `spel` v0.6.0 and
`logos-execution-zone` v0.2.0 sources, by the host-side unit tests, and by
`spel_lint`. Expect small type-level fixes on the first `make build`.

## Deliberately left open (demo scope)

* Fixed-price swap against a pool, no AMM: the pool carries all inventory risk.
* `oracle_prices::publish_price` is still permissionless and `round` is still
  unconstrained. `swap` mitigates on the read side (`--min-round`,
  `MIN_VALID_COUNT`) but cannot fix it.
* No on-chain staleness bound — a SPEL program has no access to block height,
  so freshness is asserted by the caller, like a DEX deadline.
* One trading pair per deployment.
* `to` must already be initialized; the swap does not create it.
