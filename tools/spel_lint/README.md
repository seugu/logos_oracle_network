# spel_lint

A static checker for `#[lez_program]` guest sources. It parses the file with
`syn` and enforces the invariants that otherwise only surface as a guest panic
or a sequencer rejection *after* a full risc0 build:

| Check | What it catches |
|---|---|
| `execute()` account count == declared count | `execute_with_claims` asserts equal length → guest panic; the runtime also rejects a pre/post length mismatch in `validate_execution` |
| `execute()` account order == declaration order | post-states are zipped with pre-states positionally |
| no duplicate account in `execute()` | `validate_uniqueness_of_account_ids` |
| every declared account is used in the body | a declared-but-ignored account (e.g. a price feed that is never read) |
| every instruction arg is used in the body | an arg the caller supplies that the program silently ignores |
| `#[account(init, pda = ..)]` + chained calls | double claim → `InconsistentAccountPreState` |
| return type is `SpelResult` | |

Run it before `make build`; it takes milliseconds where the risc0 build takes minutes.

```bash
cargo run -p spel_lint -- ../../swap_demo/methods/guest/src/bin/swap_demo.rs
```

Exit code is non-zero on any failure, so it drops straight into CI.
