//! Shared, host-testable logic for the `swap_demo` SPEL program.
//!
//! Everything in this crate is pure: no zkVM, no SPEL, no account plumbing.
//! The guest binary (`methods/guest/src/bin/swap_demo.rs`) does the account
//! handling and calls in here for the parts that are worth unit-testing on the
//! host, so the pricing maths can be exercised without a sequencer.

use sha2::{Digest, Sha256};

// ─── PDA seed literals ───────────────────────────────────────────────────

/// Seed literal of the singleton `swap_demo` state account.
pub const SWAP_STATE_SEED_LITERAL: &str = "swap_demo";

/// Seed literal of a per-token-definition pool account. The full PDA is
/// `compute_pda(swap_demo_program_id, [POOL_SEED_LITERAL, token_definition_id])`.
pub const POOL_SEED_LITERAL: &str = "swap_demo_pool";

/// Seed literal of an `oracle_prices` price-feed account. The full PDA is
/// `compute_pda(oracle_prices_program_id, [FEED_SEED_LITERAL, feed_id])`.
///
/// Must stay in sync with `oracle_prices::initialize_feed`.
pub const FEED_SEED_LITERAL: &str = "oracle_prices__";

// ─── PDA seed derivation ─────────────────────────────────────────────────

/// SPEL's zero-padded 32-byte seed for a string literal.
///
/// Mirrors `spel_framework::pda::seed_from_str`. Kept here so the derivation
/// can be tested on the host without pulling the framework in.
///
/// # Panics
/// Panics if `s` is longer than 32 bytes, exactly as the framework does.
pub fn seed_from_str(s: &str) -> [u8; 32] {
    let src = s.as_bytes();
    assert!(src.len() <= 32, "seed string '{s}' exceeds 32 bytes");
    let mut bytes = [0u8; 32];
    bytes[..src.len()].copy_from_slice(src);
    bytes
}

/// Raw 32-byte PDA seed for a `[literal, account_id]` two-seed derivation.
///
/// This must equal what `spel_framework::pda::compute_pda` combines internally
/// for two seeds — `SHA-256(seed_a || seed_b)` — because the same value is used
/// twice with different meanings: the framework hashes it again into the
/// account *address*, while `ChainedCall::pda_seeds` needs the pre-image to
/// delegate authority over that address. If the two ever diverge, the runtime
/// refuses the delegation and the transfer fails.
pub fn combined_seed(literal: &str, account_id: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(seed_from_str(literal));
    hasher.update(account_id);
    hasher.finalize().into()
}

/// Seed for the pool account belonging to `token_definition_id`.
pub fn pool_seed(token_definition_id: &[u8; 32]) -> [u8; 32] {
    combined_seed(POOL_SEED_LITERAL, token_definition_id)
}

/// Seed for the `oracle_prices` feed account of `feed_id`.
pub fn feed_seed(feed_id: &[u8; 32]) -> [u8; 32] {
    combined_seed(FEED_SEED_LITERAL, feed_id)
}

// ─── Price policy ────────────────────────────────────────────────────────

/// Smallest number of independent observations an `AttestedPrice` must
/// aggregate before this contract will trade on it.
///
/// A demo value. On a real deployment this should be derived from the
/// registered committee size rather than hard-coded.
pub const MIN_VALID_COUNT: u32 = 1;

/// Upper bound on `PriceState::decimals`, so `10^decimals` always fits in u128.
pub const MAX_DECIMALS: u32 = 38;

// ─── Quote maths ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteError {
    /// `amount_in` was zero.
    ZeroAmountIn,
    /// The feed carries a zero price — treated as "never published".
    ZeroPrice,
    /// `decimals` would make `10^decimals` overflow u128.
    DecimalsTooLarge { decimals: u32 },
    /// Intermediate multiplication overflowed u128.
    Overflow,
    /// The trade is economically empty: it rounds down to zero output.
    RoundsToZero,
}

impl core::fmt::Display for QuoteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            QuoteError::ZeroAmountIn => write!(f, "amount_in must be greater than zero"),
            QuoteError::ZeroPrice => write!(f, "price feed holds a zero price"),
            QuoteError::DecimalsTooLarge { decimals } => {
                write!(f, "price decimals {decimals} exceed the supported maximum")
            }
            QuoteError::Overflow => write!(f, "arithmetic overflow while quoting"),
            QuoteError::RoundsToZero => write!(f, "trade size rounds down to zero output"),
        }
    }
}

/// `10^decimals` as u128, or an error if it would overflow.
fn scale(decimals: u32) -> Result<u128, QuoteError> {
    if decimals > MAX_DECIMALS {
        return Err(QuoteError::DecimalsTooLarge { decimals });
    }
    10u128
        .checked_pow(decimals)
        .ok_or(QuoteError::DecimalsTooLarge { decimals })
}

/// Base → quote: how much quote token is `amount_in` base token worth?
///
/// The feed publishes `price` as *quote per base*, scaled by `10^decimals`
/// (see `oracle_prices::PriceState`), so:
///
/// ```text
/// amount_out = amount_in * price / 10^decimals
/// ```
///
/// Rounds **down**, i.e. in favour of the pool, never the trader.
pub fn quote_base_to_quote(
    amount_in: u128,
    price: u64,
    decimals: u32,
) -> Result<u128, QuoteError> {
    if amount_in == 0 {
        return Err(QuoteError::ZeroAmountIn);
    }
    if price == 0 {
        return Err(QuoteError::ZeroPrice);
    }
    let denom = scale(decimals)?;
    let numer = amount_in
        .checked_mul(u128::from(price))
        .ok_or(QuoteError::Overflow)?;
    let out = numer / denom;
    if out == 0 {
        return Err(QuoteError::RoundsToZero);
    }
    Ok(out)
}

/// Quote → base: the inverse of [`quote_base_to_quote`].
///
/// ```text
/// amount_out = amount_in * 10^decimals / price
/// ```
///
/// Rounds **down** as well, so a round-trip never mints value out of rounding.
pub fn quote_quote_to_base(
    amount_in: u128,
    price: u64,
    decimals: u32,
) -> Result<u128, QuoteError> {
    if amount_in == 0 {
        return Err(QuoteError::ZeroAmountIn);
    }
    if price == 0 {
        return Err(QuoteError::ZeroPrice);
    }
    let factor = scale(decimals)?;
    let numer = amount_in
        .checked_mul(factor)
        .ok_or(QuoteError::Overflow)?;
    let out = numer / u128::from(price);
    if out == 0 {
        return Err(QuoteError::RoundsToZero);
    }
    Ok(out)
}

/// Quote in the direction requested by the caller.
pub fn quote(
    amount_in: u128,
    price: u64,
    decimals: u32,
    base_to_quote: bool,
) -> Result<u128, QuoteError> {
    if base_to_quote {
        quote_base_to_quote(amount_in, price, decimals)
    } else {
        quote_quote_to_base(amount_in, price, decimals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1 ETH at 2500.00 USDC (decimals = 2) → 2500 USDC, in whole units.
    #[test]
    fn base_to_quote_simple() {
        assert_eq!(quote_base_to_quote(1, 250_000, 2), Ok(2_500));
    }

    #[test]
    fn quote_to_base_is_the_inverse() {
        let out = quote_base_to_quote(3, 250_000, 2).unwrap();
        assert_eq!(out, 7_500);
        assert_eq!(quote_quote_to_base(out, 250_000, 2), Ok(3));
    }

    #[test]
    fn decimals_zero_is_a_plain_multiply() {
        assert_eq!(quote_base_to_quote(7, 11, 0), Ok(77));
        assert_eq!(quote_quote_to_base(77, 11, 0), Ok(7));
    }

    #[test]
    fn rounds_down_never_up() {
        // 1 * 150 / 100 = 1.5 → 1
        assert_eq!(quote_base_to_quote(1, 150, 2), Ok(1));
        // 1 * 199 / 100 = 1.99 → 1
        assert_eq!(quote_base_to_quote(1, 199, 2), Ok(1));
    }

    #[test]
    fn round_trip_never_gains() {
        // Anything that survives a there-and-back must not exceed the input.
        for amount in 1u128..=200 {
            let price = 250_000u64;
            let decimals = 2u32;
            if let Ok(out) = quote_base_to_quote(amount, price, decimals) {
                if let Ok(back) = quote_quote_to_base(out, price, decimals) {
                    assert!(
                        back <= amount,
                        "round trip gained value: {amount} -> {out} -> {back}"
                    );
                }
            }
        }
    }

    #[test]
    fn dust_trade_is_rejected_not_silently_zeroed() {
        // 1 unit of base at a price far below 1 quote unit.
        assert_eq!(
            quote_base_to_quote(1, 1, 18),
            Err(QuoteError::RoundsToZero)
        );
    }

    #[test]
    fn zero_inputs_are_rejected() {
        assert_eq!(quote_base_to_quote(0, 100, 2), Err(QuoteError::ZeroAmountIn));
        assert_eq!(quote_base_to_quote(5, 0, 2), Err(QuoteError::ZeroPrice));
        assert_eq!(quote_quote_to_base(0, 100, 2), Err(QuoteError::ZeroAmountIn));
        assert_eq!(quote_quote_to_base(5, 0, 2), Err(QuoteError::ZeroPrice));
    }

    #[test]
    fn overflow_is_an_error_not_a_wrap() {
        assert_eq!(
            quote_base_to_quote(u128::MAX, u64::MAX, 0),
            Err(QuoteError::Overflow)
        );
        assert_eq!(
            quote_quote_to_base(u128::MAX, 1, 38),
            Err(QuoteError::Overflow)
        );
    }

    #[test]
    fn absurd_decimals_are_rejected() {
        assert_eq!(
            quote_base_to_quote(1, 100, 39),
            Err(QuoteError::DecimalsTooLarge { decimals: 39 })
        );
    }

    #[test]
    fn max_decimals_still_works() {
        // 10^38 fits in u128 (u128::MAX ≈ 3.4e38).
        assert!(scale(MAX_DECIMALS).is_ok());
        assert!(scale(MAX_DECIMALS + 1).is_err());
    }


    // ── seed derivation ────────────────────────────────────────────────

    #[test]
    fn seed_from_str_is_right_padded() {
        let s = seed_from_str(POOL_SEED_LITERAL);
        assert_eq!(&s[..POOL_SEED_LITERAL.len()], POOL_SEED_LITERAL.as_bytes());
        assert!(s[POOL_SEED_LITERAL.len()..].iter().all(|b| *b == 0));
    }

    #[test]
    fn seed_literals_fit_in_32_bytes() {
        // seed_from_str panics above 32 bytes, so a too-long literal would be
        // a guest-side panic rather than a clean error.
        for lit in [SWAP_STATE_SEED_LITERAL, POOL_SEED_LITERAL, FEED_SEED_LITERAL] {
            assert!(lit.len() <= 32, "{lit} is too long for a PDA seed");
        }
    }

    #[test]
    fn combined_seed_is_sha256_of_the_two_padded_seeds() {
        let def = [7u8; 32];
        let mut h = Sha256::new();
        h.update(seed_from_str(POOL_SEED_LITERAL));
        h.update(def);
        let expected: [u8; 32] = h.finalize().into();
        assert_eq!(pool_seed(&def), expected);
    }

    #[test]
    fn pool_and_feed_seeds_are_domain_separated() {
        let id = [3u8; 32];
        assert_ne!(pool_seed(&id), feed_seed(&id));
    }

    #[test]
    fn pool_seed_depends_on_the_definition() {
        assert_ne!(pool_seed(&[1u8; 32]), pool_seed(&[2u8; 32]));
    }

    /// Regression vector: the value `pda_seed_tool` prints for a known
    /// definition account must be exactly what the guest recomputes.
    #[test]
    fn pool_seed_matches_pda_seed_tool_formula() {
        // pda_seed_tool: SHA256(seed_from_str("swap_demo_pool") || def_account)
        let def = *b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f";
        let mut tag = [0u8; 32];
        tag[..b"swap_demo_pool".len()].copy_from_slice(b"swap_demo_pool");
        let mut h = Sha256::new();
        h.update(tag);
        h.update(def);
        let tool: [u8; 32] = h.finalize().into();
        assert_eq!(pool_seed(&def), tool);
    }

    #[test]
    fn direction_switch_matches_the_direct_calls() {
        assert_eq!(
            quote(4, 250_000, 2, true),
            quote_base_to_quote(4, 250_000, 2)
        );
        assert_eq!(
            quote(4, 250_000, 2, false),
            quote_quote_to_base(4, 250_000, 2)
        );
    }
}
