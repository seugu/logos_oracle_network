//! Shared on-chain types for the `oracle_prices` SPEL program.
//!
//! These live here — rather than only inside the guest binary — so that
//! *consumer* programs (e.g. `swap_demo`) can decode a price-feed account
//! without hand-copying the Borsh layout. The guest keeps its own
//! `#[account_type]`-annotated copies for IDL generation, guarded against
//! drift by a compile-time conversion (see `oracle_prices.rs`).
//!
//! Dependency-free on purpose (borsh only): it links into two different risc0
//! guests that pin different `spel-framework` revisions, so it must not pull
//! `spel-framework` in itself.

use borsh::{BorshDeserialize, BorshSerialize};

/// Seed literal of the singleton `oracle_prices` state account.
pub const ORACLE_PRICES_SEED_LITERAL: &str = "oracle_prices";

/// Seed literal of a single price-feed account. The full PDA is
/// `compute_pda(oracle_prices_program_id, [FEED_SEED_LITERAL, feed_id])`.
pub const FEED_SEED_LITERAL: &str = "oracle_prices__";

/// Registry of the feeds this oracle instance knows about.
#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OraclePricesState {
    pub feeds: Vec<[u8; 32]>,
}

/// One published `AttestedPrice`, as stored in a feed account.
#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PriceState {
    /// Asset-pair identifier, e.g. hash("BTC/USDT").
    pub feed_id: [u8; 32],
    /// Attested median; the real value is `price * 10^(-decimals)`.
    pub price: u64,
    /// Number of decimal places encoded in `price`.
    pub decimals: u32,
    /// Number of observations aggregated into this round.
    pub valid_count: u32,
    /// Round identifier, in Bedrock block-height terms.
    pub round: u64,
    /// Dispersion of the observations, scaled like `price`.
    pub confidence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-level fixture. If this breaks, every already-deployed consumer of
    /// a feed account breaks with it — treat a change here as a migration.
    #[test]
    fn price_state_wire_format_is_stable() {
        let p = PriceState {
            feed_id: [1u8; 32],
            price: 250_000,
            decimals: 2,
            valid_count: 3,
            round: 1000,
            confidence: 42,
        };
        let bytes = borsh::to_vec(&p).unwrap();
        // 32 (feed_id) + 8 (price) + 4 (decimals) + 4 (valid_count)
        //  + 8 (round) + 8 (confidence)
        assert_eq!(bytes.len(), 64);
        assert_eq!(&bytes[..32], &[1u8; 32]);
        assert_eq!(&bytes[32..40], &250_000u64.to_le_bytes());
        assert_eq!(&bytes[40..44], &2u32.to_le_bytes());
        assert_eq!(&bytes[44..48], &3u32.to_le_bytes());
        assert_eq!(&bytes[48..56], &1000u64.to_le_bytes());
        assert_eq!(&bytes[56..64], &42u64.to_le_bytes());

        let back: PriceState = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn default_price_state_round_trips() {
        let p = PriceState::default();
        let bytes = borsh::to_vec(&p).unwrap();
        assert_eq!(borsh::from_slice::<PriceState>(&bytes).unwrap(), p);
    }
}
