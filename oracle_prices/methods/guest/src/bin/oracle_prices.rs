#![no_main]

use spel_framework::prelude::*;

risc0_zkvm::guest::entry!(main);

#[account_type]
#[derive(BorshSerialize, BorshDeserialize, Default)]
pub struct OraclePricesState {
    // TODO: for now, everybody can initialize a feed
    //       idea: restrict initialize_feed to registered oracle node
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

        // println!("initialize...");

        let state = OraclePricesState {
            feeds: vec![],
        };
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
        let price = PriceState::default();
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
        state.feeds.push(feed_id);
        let bytes = borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        oracle_prices_account.account.data = bytes.try_into().unwrap();

        Ok(SpelOutput::execute(vec![oracle_prices_account, feed_price], vec![]))
    }

    /// Publish an AttestedPrice to a price feed (require an initialized price feed)
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

        // TODO:
        // check "round" (only allow increasing round)?
        // pb: how to avoid an attacker to set a round too high?

        let price = PriceState {
            feed_id,
            price,
            decimals,
            valid_count,
            round,
            confidence,
        };

        // println!("publish price: {:?}", price);

        let bytes = borsh::to_vec(&price).map_err(|e| SpelError::SerializationError {
            message: e.to_string(),
        })?;
        feed_price.account.data = bytes.try_into().unwrap();

        Ok(SpelOutput::execute(vec![feed_price], vec![]))
    }
}