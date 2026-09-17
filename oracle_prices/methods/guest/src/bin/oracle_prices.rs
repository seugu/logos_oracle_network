#![no_main]

use spel_framework::prelude::*;

risc0_zkvm::guest::entry!(main);

// The canonical, cross-program layouts live in `oracle_prices_core` so that
// consumers (e.g. swap_demo) can decode a feed account without copying the
// Borsh layout by hand. The `#[account_type]` copies below exist only so the
// SPEL IDL generator — which scans *this file* — keeps emitting them, which is
// what makes `spel inspect <PDA> --type PriceState` work.
//
// The `const _` blocks at the end of this comment block's section turn any
// divergence between the two into a compile error rather than a silent
// wire-format break.

#[account_type]
#[derive(BorshSerialize, BorshDeserialize, Default)]
pub struct OraclePricesState {
    // TODO: for now, everybody can initialize a feed
    //       idea: restrict initialize_feed to registered oracle nodes
    // owner: [u8; 32],
    feeds: Vec<[u8; 32]>,
}

#[account_type]
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct PriceState {
    feed_id: [u8; 32], // asset pair identifier, e.g. hash("BTC/USDT")
    price: u64,        // attested median, real value = price * 10^(-decimals)
    decimals: u32,     // number of decimal places in `price`
    valid_count: u32,  // number of observations aggregated in this round
    round: u64,        // round identifier, in Bedrock block-height terms
    confidence: u64,   // OPTIONAL: dispersion of observations, scaled like `price`
}

// ─── Drift guards ────────────────────────────────────────────────────────
// These never run; they fail to compile if a field is added, removed,
// renamed or retyped on either side.

const _: () = {
    #[allow(dead_code)]
    fn price_state_matches_core(c: oracle_prices_core::PriceState) -> PriceState {
        PriceState {
            feed_id: c.feed_id,
            price: c.price,
            decimals: c.decimals,
            valid_count: c.valid_count,
            round: c.round,
            confidence: c.confidence,
        }
    }
    #[allow(dead_code)]
    fn oracle_prices_state_matches_core(c: oracle_prices_core::OraclePricesState) -> OraclePricesState {
        OraclePricesState { feeds: c.feeds }
    }
};

#[lez_program]
mod oracle_prices {
    #[allow(unused_imports)]
    use super::*;

    /// Initialize the contract
    #[instruction]
    pub fn initialize(
        #[account(init, pda = literal("oracle_prices"))]
        mut oracle_prices_account: AccountWithMetadata,
    ) -> SpelResult {
        let state = OraclePricesState { feeds: vec![] };
        let bytes = borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        oracle_prices_account.account.data = bytes.try_into().unwrap();

        Ok(SpelOutput::execute(vec![oracle_prices_account], vec![]))
    }

    /// Initialize a price feed (to receive AttestedPrice from indexers)
    #[instruction]
    pub fn initialize_feed(
        #[account(mut, pda = [literal("oracle_prices")])]
        mut oracle_prices_account: AccountWithMetadata,
        #[account(init, pda = [literal("oracle_prices__"), arg("feed_id")])]
        mut feed_price: AccountWithMetadata,
        feed_id: [u8; 32],
    ) -> SpelResult {
        // Seed the feed with its own id so a consumer can cross-check that the
        // account it was handed really is the feed it asked for, even before
        // the first price lands.
        let price = PriceState {
            feed_id,
            ..PriceState::default()
        };
        let bytes = borsh::to_vec(&price).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        feed_price.account.data = bytes.try_into().unwrap();

        // Add feed to oracle prices state
        let data: Vec<u8> = oracle_prices_account.account.data.clone().into();
        let mut state: OraclePricesState = borsh::from_slice(&data).map_err(|e| {
            SpelError::DeserializationError {
                account_index: 0,
                message: e.to_string(),
            }
        })?;
        if state.feeds.contains(&feed_id) {
            return Err(SpelError::Custom {
                code: 1,
                message: "feed already registered".to_string(),
            });
        }
        state.feeds.push(feed_id);
        let bytes = borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        oracle_prices_account.account.data = bytes.try_into().unwrap();

        Ok(SpelOutput::execute(vec![oracle_prices_account, feed_price], vec![]))
    }

    /// Publish an AttestedPrice to a price feed (requires an initialized feed)
    #[instruction]
    pub fn publish_price(
        #[account(mut, pda = [literal("oracle_prices__"), arg("feed_id")])]
        mut feed_price: AccountWithMetadata,
        feed_id: [u8; 32],
        price: u64,
        decimals: u32,
        valid_count: u32,
        round: u64,
        confidence: u64,
    ) -> SpelResult {
        // TODO (unchanged from the original): publishing is still permissionless
        // and `round` is still unconstrained. Monotonicity alone is not enough —
        // an attacker can jump `round` arbitrarily high and freeze the feed — so
        // this needs the registered-indexer check from `oracle_register` before
        // anything but a demo consumes it. `swap_demo` mitigates on the read
        // side (min_round / min_valid_count), it does not fix it here.
        let previous: Vec<u8> = feed_price.account.data.clone().into();
        let previous: PriceState =
            borsh::from_slice(&previous).map_err(|e| SpelError::DeserializationError {
                account_index: 0,
                message: e.to_string(),
            })?;
        if previous.feed_id != feed_id {
            return Err(SpelError::Custom {
                code: 2,
                message: "feed_id does not match the feed account".to_string(),
            });
        }

        let price = PriceState {
            feed_id,
            price,
            decimals,
            valid_count,
            round,
            confidence,
        };

        let bytes = borsh::to_vec(&price).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        feed_price.account.data = bytes.try_into().unwrap();

        Ok(SpelOutput::execute(vec![feed_price], vec![]))
    }
}
