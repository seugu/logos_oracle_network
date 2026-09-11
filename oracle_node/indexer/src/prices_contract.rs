use anyhow::{anyhow, Context};
// third-party - LEZ
use wallet::WalletCore;
// internal
use common::PricesContractInfo;
use oracle_prices_client::{OraclePricesClient, PublishPriceAccounts};
use crate::indexer::lon::AttestedPrice;

pub async fn publish_attested_price(pc_info: &PricesContractInfo, attested_price: AttestedPrice) -> anyhow::Result<()> {

    let wallet_core = WalletCore::from_env()
        .context("Getting wallet accounts from env")?;
    let client = OraclePricesClient::new(&wallet_core, pc_info.oracle_prices_program_id);

    let feed_id: [u8; 32] = attested_price.feed_id.as_slice().try_into()?;
    let accounts = PublishPriceAccounts {
        feed_price: oracle_prices_client::compute_feed_price_pda(&pc_info.oracle_prices_program_id, &feed_id),
    };

    client.publish_price(accounts,
                         feed_id,
                         attested_price.price.try_into()?,
                         attested_price.decimals.try_into()?,
                         attested_price.valid_count,
                         attested_price.round.try_into()?,
                         attested_price.confidence.try_into()?,
    )
        .await
        .map_err(|err| anyhow!(err))?;

    Ok(())
}
