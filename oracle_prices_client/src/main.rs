// use std::str::FromStr;
use anyhow::{anyhow, Context};
use borsh::{BorshDeserialize, BorshSerialize};
use sequencer_service_rpc::RpcClient as _;
// use nssa::{
//     AccountId,
//     ProgramId, PublicTransaction,
//     public_transaction::{Message, WitnessSet},
// };
// use spel_framework_core::prelude::AccountId;
// use borsh::{BorshSerialize, BorshDeserialize};
use nssa::program::Program;
use spel_framework::account_type;
use wallet::WalletCore;
use oracle_prices_client::OraclePricesClient;
// use serde::{Deserialize, Serialize};
// use wallet::WalletCore;

#[account_type]
#[derive(BorshSerialize, BorshDeserialize, Default, Debug)]
pub struct OraclePricesState {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let program_path_ = std::env::args().into_iter().nth(1);

    println!("program path: {:?}", program_path_);
    let program_path = program_path_.ok_or(anyhow!("No args"))?;
    let program_bytecode = std::fs::read(program_path.clone())
        .context(format!("Error while reading: {}", program_path))?;
    let program = Program::new(program_bytecode.into())?;
    // println!("program: {:?}", program);
    let pid = program.id();
    println!("pid: {:?}", pid);
    let program_id_hex_str: String = pid
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .map(|b| format!("{:02x}", b))
        .collect();
    println!("pid: {}", program_id_hex_str);

    let wallet_core = WalletCore::from_env()?;
    let client = OraclePricesClient::new(&wallet_core, pid);
    println!("client program id: {:?}", client.program_id);
    // println!("client wallet: {:?}", client.wallet);

    let data = client.fetch_oracle_prices_account::<OraclePricesState>().await
        .map_err(|err| anyhow!(err))?;
    println!("oracle price state: {:?}", data);

    for price_feed_id in data.feeds {
        let data = client.fetch_feed_price::<PriceState>(&price_feed_id).await
            .map_err(|err| anyhow!(err))?;
        println!("price feed: {:?}", data);
    }


    Ok(())
}