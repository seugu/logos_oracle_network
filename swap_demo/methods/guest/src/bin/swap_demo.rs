#![no_main]

use risc0_zkvm::serde::to_vec;
use spel_framework::prelude::*;
use swap_demo_core::{feed_seed, pool_seed, quote, QuoteError, MIN_VALID_COUNT};
use token_core::{Instruction as TokenInstruction, TokenDefinition, TokenHolding};

risc0_zkvm::guest::entry!(main);

/// Singleton configuration for the demo AMM-less swap.
///
/// One instance == one trading pair. That is deliberate: binding `feed_id` to
/// the pair on-chain is what stops a caller from pointing `swap` at some other,
/// more convenient feed. A multi-pair version needs a per-pair PDA holding this
/// same triple; see the note in the README.
#[account_type]
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct SwapDemoState {
    /// Program id of the LEZ token program used for the transfers.
    pub token_program_id: [u32; 8],
    /// Program id of the `oracle_prices` deployment we read prices from.
    pub oracle_program_id: [u32; 8],
    /// The one feed this pair prices against, quote-per-base.
    pub feed_id: [u8; 32],
    /// Token definition account of the base asset (e.g. ETH).
    pub base_definition: [u8; 32],
    /// Token definition account of the quote asset (e.g. USDC).
    pub quote_definition: [u8; 32],
}

// ─── helpers ─────────────────────────────────────────────────────────────

fn holding_of(
    account: &AccountWithMetadata,
    name: &str,
    index: usize,
) -> Result<TokenHolding, SpelError> {
    if account.account == Account::default() {
        return Err(SpelError::AccountNotInitialized {
            account_index: index,
        });
    }
    TokenHolding::try_from(&account.account.data).map_err(|e| SpelError::DeserializationError {
        account_index: index,
        message: format!("{name} is not a token holding account: {e}"),
    })
}

fn fungible_balance(holding: &TokenHolding, name: &str) -> Result<u128, SpelError> {
    match holding {
        TokenHolding::Fungible { balance, .. } => Ok(*balance),
        _ => Err(SpelError::Custom {
            code: 10,
            message: format!("{name} must hold a fungible token"),
        }),
    }
}

fn quote_err(e: QuoteError) -> SpelError {
    match e {
        QuoteError::Overflow => SpelError::Overflow {
            operation: "swap quote".to_string(),
        },
        other => SpelError::Custom {
            code: 11,
            message: other.to_string(),
        },
    }
}

#[lez_program]
mod swap_demo {
    #[allow(unused_imports)]
    use super::*;

    /// Initialize the contract for one trading pair.
    ///
    /// `base`/`quote` are *token definition* accounts, not holdings. They are
    /// passed as accounts rather than raw ids so we can at least check they are
    /// owned by the declared token program and really are fungible definitions.
    #[instruction]
    pub fn initialize(
        #[account(init, pda = literal("swap_demo"))]
        mut swap_state: AccountWithMetadata,
        #[account()]
        base_definition: AccountWithMetadata,
        #[account()]
        quote_definition: AccountWithMetadata,
        token_program_id: [u32; 8],
        oracle_program_id: [u32; 8],
        feed_id: [u8; 32],
    ) -> SpelResult {
        let token_pg_id = ProgramId::from(token_program_id);

        for (idx, (name, acc)) in [
            ("base_definition", &base_definition),
            ("quote_definition", &quote_definition),
        ]
        .into_iter()
        .enumerate()
        {
            if acc.account.program_owner != token_pg_id {
                return Err(SpelError::InvalidAccountOwner {
                    account_index: idx + 1,
                    expected_owner: format!("{token_pg_id:?}"),
                });
            }
            let definition = TokenDefinition::try_from(&acc.account.data).map_err(|e| {
                SpelError::DeserializationError {
                    account_index: idx + 1,
                    message: format!("{name} is not a token definition: {e}"),
                }
            })?;
            if !matches!(definition, TokenDefinition::Fungible { .. }) {
                return Err(SpelError::Custom {
                    code: 12,
                    message: format!("{name} must be a fungible token definition"),
                });
            }
        }

        if base_definition.account_id == quote_definition.account_id {
            return Err(SpelError::Custom {
                code: 13,
                message: "base and quote must be different tokens".to_string(),
            });
        }

        let state = SwapDemoState {
            token_program_id,
            oracle_program_id,
            feed_id,
            base_definition: *base_definition.account_id.value(),
            quote_definition: *quote_definition.account_id.value(),
        };
        let bytes = borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        swap_state.account.data = bytes.try_into().unwrap();

        Ok(SpelOutput::execute(
            vec![swap_state, base_definition, quote_definition],
            vec![],
        ))
    }

    /// Create the pool holding account for one side of the pair.
    ///
    /// Call once per token definition. The pool is a PDA of *this* program
    /// derived from the token definition account, so its address is fully
    /// determined — `pool_account` is verified against that derivation rather
    /// than trusted, and the seed we hand to the token program is recomputed
    /// here instead of being accepted as an argument.
    ///
    /// Note on `#[account(mut)]` rather than `#[account(init, pda = ...)]`:
    /// `init` makes this program claim the account in its own post-state, which
    /// the chained `InitializeAccount` then tries to claim as well. The runtime
    /// rejects that as `InconsistentAccountPreState`. Dropping `init` keeps the
    /// address check (which is the part that matters) and leaves the claim to
    /// the token program, which is the program that actually writes the data.
    #[instruction]
    pub fn initialize_pool(
        ctx: ProgramContext,
        #[account(mut, pda = literal("swap_demo"))]
        swap_state: AccountWithMetadata,
        #[account()]
        token_definition_account: AccountWithMetadata,
        #[account(mut, pda = [literal("swap_demo_pool"), account("token_definition_account")])]
        pool_account: AccountWithMetadata,
    ) -> SpelResult {
        let data: Vec<u8> = swap_state.account.data.clone().into();
        let state: SwapDemoState =
            borsh::from_slice(&data).map_err(|e| SpelError::DeserializationError {
                account_index: 0,
                message: e.to_string(),
            })?;

        let token_pg_id = ProgramId::from(state.token_program_id);
        if token_definition_account.account.program_owner != token_pg_id {
            return Err(SpelError::InvalidAccountOwner {
                account_index: 1,
                expected_owner: format!("{token_pg_id:?}"),
            });
        }

        let definition_id = *token_definition_account.account_id.value();
        if definition_id != state.base_definition && definition_id != state.quote_definition {
            return Err(SpelError::Custom {
                code: 14,
                message: "token definition is not part of this pair".to_string(),
            });
        }

        if pool_account.account != Account::default() {
            return Err(SpelError::AccountAlreadyInitialized { account_index: 2 });
        }

        // Belt and braces: the macro already compared `pool_account.account_id`
        // against this derivation, but we need the raw seed anyway, and
        // recomputing the address from it proves the two agree.
        let seed = pool_seed(&definition_id);
        let expected_pool = compute_pda(&ctx.self_program_id, &[&seed]);
        if pool_account.account_id != expected_pool {
            return Err(SpelError::PdaMismatch {
                account_name: "pool_account".to_string(),
                expected: format!("{expected_pool:?}"),
                actual: format!("{:?}", pool_account.account_id),
            });
        }

        // The pool is a PDA of this program, and we delegate authority over it
        // for the duration of the chained call via `pda_seeds`. The runtime
        // requires the flag on the pre-state to match that delegation exactly —
        // marking it is mandatory, not a workaround.
        let pool_account_authorized = {
            let mut acc = pool_account.clone();
            acc.is_authorized = true;
            acc
        };

        let chained_call_init = ChainedCall {
            program_id: token_pg_id,
            pre_states: vec![
                token_definition_account.clone(),
                pool_account_authorized,
            ],
            instruction_data: to_vec(&TokenInstruction::InitializeAccount).map_err(|e| {
                SpelError::SerializationError {
                    message: e.to_string(),
                }
            })?,
            pda_seeds: vec![PdaSeed::new(seed)],
        };

        Ok(SpelOutput::execute(
            vec![swap_state, token_definition_account, pool_account],
            vec![chained_call_init],
        ))
    }

    /// Swap `amount_in` of one side of the pair for the other, at the oracle price.
    ///
    /// This is a fixed-price swap against a pool, not an AMM: the price comes
    /// entirely from the `oracle_prices` feed, so the pool takes on the full
    /// inventory risk. That is fine for a demo and wrong for anything else.
    ///
    /// * `from` — the caller's holding of the input token. Must be a tx signer.
    /// * `to` — the caller's holding of the output token. Must already exist
    ///   (`spel initialize-account`); we deliberately do not initialize it here,
    ///   because doing so would require the caller to authorize it separately.
    /// * `min_amount_out` — slippage guard, in output-token units.
    /// * `min_round` — freshness guard. The feed's round must be at least this.
    ///   There is no block height available inside the program, so staleness has
    ///   to be asserted by the caller, the same way a DEX deadline works.
    #[instruction]
    pub fn swap(
        ctx: ProgramContext,
        #[account(mut, pda = literal("swap_demo"))]
        swap_state: AccountWithMetadata,
        #[account()]
        price_feed: AccountWithMetadata,
        #[account(signer)]
        from: AccountWithMetadata,
        #[account(mut)]
        to: AccountWithMetadata,
        #[account(mut)]
        pool_in: AccountWithMetadata,
        #[account(mut)]
        pool_out: AccountWithMetadata,
        amount_in: u64,
        min_amount_out: u64,
        min_round: u64,
        base_to_quote: bool,
    ) -> SpelResult {
        // ── 0. contract state ────────────────────────────────────────────
        let data: Vec<u8> = swap_state.account.data.clone().into();
        let state: SwapDemoState =
            borsh::from_slice(&data).map_err(|e| SpelError::DeserializationError {
                account_index: 0,
                message: e.to_string(),
            })?;
        let token_pg_id = ProgramId::from(state.token_program_id);
        let oracle_pg_id = ProgramId::from(state.oracle_program_id);

        // ── 1. the price feed: right program, right PDA, right feed ──────
        if price_feed.account.program_owner != oracle_pg_id {
            return Err(SpelError::InvalidAccountOwner {
                account_index: 1,
                expected_owner: format!("{oracle_pg_id:?}"),
            });
        }
        let expected_feed = compute_pda(
            &oracle_pg_id,
            &[&feed_seed(&state.feed_id)],
        );
        if price_feed.account_id != expected_feed {
            return Err(SpelError::PdaMismatch {
                account_name: "price_feed".to_string(),
                expected: format!("{expected_feed:?}"),
                actual: format!("{:?}", price_feed.account_id),
            });
        }

        let feed_data: Vec<u8> = price_feed.account.data.clone().into();
        let price_state: oracle_prices_core::PriceState = borsh::from_slice(&feed_data)
            .map_err(|e| SpelError::DeserializationError {
                account_index: 1,
                message: format!("price_feed is not a PriceState: {e}"),
            })?;

        if price_state.feed_id != state.feed_id {
            return Err(SpelError::Custom {
                code: 20,
                message: "price feed carries a different feed_id".to_string(),
            });
        }
        if price_state.valid_count < MIN_VALID_COUNT {
            return Err(SpelError::Custom {
                code: 21,
                message: format!(
                    "price aggregates {} observations, need at least {MIN_VALID_COUNT}",
                    price_state.valid_count
                ),
            });
        }
        if price_state.round < min_round {
            return Err(SpelError::Custom {
                code: 22,
                message: format!(
                    "price is stale: round {} < required {min_round}",
                    price_state.round
                ),
            });
        }

        // ── 2. decode every holding, bind it to the pair ─────────────────
        let holding_from = holding_of(&from, "from", 2)?;
        let holding_to = holding_of(&to, "to", 3)?;
        let holding_pool_in = holding_of(&pool_in, "pool_in", 4)?;
        let holding_pool_out = holding_of(&pool_out, "pool_out", 5)?;

        let def_in = holding_pool_in.definition_id();
        let def_out = holding_pool_out.definition_id();

        // The token program would also catch a definition mismatch, but it does
        // so by panicking inside the circuit. Fail with a readable error first.
        if holding_from.definition_id() != def_in {
            return Err(SpelError::Custom {
                code: 23,
                message: "`from` and `pool_in` hold different tokens".to_string(),
            });
        }
        if holding_to.definition_id() != def_out {
            return Err(SpelError::Custom {
                code: 24,
                message: "`to` and `pool_out` hold different tokens".to_string(),
            });
        }

        let (expected_in, expected_out) = if base_to_quote {
            (state.base_definition, state.quote_definition)
        } else {
            (state.quote_definition, state.base_definition)
        };
        if *def_in.value() != expected_in || *def_out.value() != expected_out {
            return Err(SpelError::Custom {
                code: 25,
                message: "pool accounts do not match the requested swap direction".to_string(),
            });
        }

        // ── 3. both pools must be *our* PDAs ─────────────────────────────
        let pool_in_seed = pool_seed(def_in.value());
        let pool_out_seed = pool_seed(def_out.value());
        for (name, account, seed) in [
            ("pool_in", &pool_in, &pool_in_seed),
            ("pool_out", &pool_out, &pool_out_seed),
        ] {
            let expected = compute_pda(&ctx.self_program_id, &[seed]);
            if account.account_id != expected {
                return Err(SpelError::PdaMismatch {
                    account_name: name.to_string(),
                    expected: format!("{expected:?}"),
                    actual: format!("{:?}", account.account_id),
                });
            }
        }

        // ── 4. quote, slippage, liquidity ────────────────────────────────
        let amount_in = u128::from(amount_in);
        let amount_out = quote(
            amount_in,
            price_state.price,
            price_state.decimals,
            base_to_quote,
        )
        .map_err(quote_err)?;

        if amount_out < u128::from(min_amount_out) {
            return Err(SpelError::Custom {
                code: 26,
                message: format!(
                    "slippage: quote {amount_out} is below min_amount_out {min_amount_out}"
                ),
            });
        }

        let from_balance = fungible_balance(&holding_from, "from")?;
        if from_balance < amount_in {
            return Err(SpelError::InsufficientBalance {
                available: from_balance,
                requested: amount_in,
            });
        }
        let pool_out_balance = fungible_balance(&holding_pool_out, "pool_out")?;
        if pool_out_balance < amount_out {
            return Err(SpelError::InsufficientBalance {
                available: pool_out_balance,
                requested: amount_out,
            });
        }
        // Reject a trade the output pool could satisfy but that would also
        // overflow the input pool's balance.
        let pool_in_balance = fungible_balance(&holding_pool_in, "pool_in")?;
        pool_in_balance
            .checked_add(amount_in)
            .ok_or(SpelError::Overflow {
                operation: "pool_in balance".to_string(),
            })?;

        // ── 5. leg 1: from -> pool_in ────────────────────────────────────
        // `from` is a tx signer, so it is already in the caller's authorized
        // set; `pool_in` is merely the recipient of an *initialized* holding
        // and needs no authorization. Both flags are passed through untouched —
        // the runtime checks them for exact agreement in both directions.
        let leg_in = ChainedCall {
            program_id: token_pg_id,
            pre_states: vec![from.clone(), pool_in.clone()],
            instruction_data: to_vec(&TokenInstruction::Transfer {
                amount_to_transfer: amount_in,
            })
            .map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?,
            pda_seeds: vec![],
        };

        // ── 6. leg 2: pool_out -> to ─────────────────────────────────────
        // Here the *sender* is the PDA, so `pool_out` is the account that needs
        // the delegation and the flag. `to` is a plain recipient: leaving its
        // flag as it arrived is what keeps this correct whether or not the
        // caller also signed with it.
        let pool_out_authorized = {
            let mut acc = pool_out.clone();
            acc.is_authorized = true;
            acc
        };
        let leg_out = ChainedCall {
            program_id: token_pg_id,
            pre_states: vec![pool_out_authorized, to.clone()],
            instruction_data: to_vec(&TokenInstruction::Transfer {
                amount_to_transfer: amount_out,
            })
            .map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?,
            pda_seeds: vec![PdaSeed::new(pool_out_seed)],
        };

        // Post-states must list every declared account, in declaration order:
        // the runtime zips pre- and post-states positionally and rejects a
        // length mismatch outright.
        Ok(SpelOutput::execute(
            vec![swap_state, price_feed, from, to, pool_in, pool_out],
            vec![leg_in, leg_out],
        ))
    }
}
