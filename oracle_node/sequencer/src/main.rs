mod sequencer;
mod pyth_fetch;
mod monitor;
mod zone_state;
mod args;
mod register_contract;
mod binance_fetch;
mod redstone_fetch;

pub mod lon {
    include!(concat!(env!("OUT_DIR"), "/lon.rs"));
}

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;
use std::sync::Arc;
// third-party
use crate::sequencer::Sequencer;
use anyhow::{anyhow, Context};
// use spel_framework::prelude::AccountId;
use clap::Parser;
use dashmap::DashMap;
use rand::Rng;
use tokio::task::JoinSet;
use serde::Deserialize;
use tracing::{
    info,
    debug,
    // warn,
    error,
    level_filters::LevelFilter
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};
use url::Url;
// internal
use crate::args::SequencerArgs;
use crate::monitor::PriceMonitor;
use crate::register_contract::sequencer_register;
use common::{time_info_poll, RegisterContractInfo};
use lb_core::mantle::ops::channel::ChannelId;
use lb_key_management_system_service::keys::{Ed25519Key, ED25519_SECRET_KEY_SIZE};

#[tokio::main]
async fn main() {
    let args = SequencerArgs::parse();
    if let Err(err) = run(args).await {
        error!("[Main] error: {:#?}", err);
    }
}

pub async fn run(args: SequencerArgs) -> anyhow::Result<()> {

    setup_tracing();

    info!("Starting oracle node sequencer...");
    debug!("args: {:?}", &args);

    let key_full_path = args.data_folder.join(&args.key_path);
    let oracle_keypair = load_or_create_signing_key(key_full_path.as_path())?;
    let oracle_pubk = oracle_keypair.public_key();

    let cfg = parse_provider_config(args.provider_config.as_path())
        .context(format!("while parsing provider config: {}", args.provider_config.display()))?;
    
    let provider = "binance";
    // let provider = "redstone";
    let price_feed_normalized = "BTC/USDT";
    let price_feed_url = cfg.endpoints.get(provider).ok_or(anyhow!("Cannot get an url for provider"))?;
    let price_feed_provider = cfg.feeds.get(provider)
        .ok_or(anyhow!("Cannot get feeds for provider"))?
        .get(price_feed_normalized)
        .ok_or(anyhow!("Cannot get price feed provider"))?
        .clone();

    let price_map = Arc::new(DashMap::new());

    let mut sequencer = Sequencer::new(
        &args.node_rest_url,
        args.node_auth_username,
        args.node_auth_password,
        args.data_folder.join(&args.checkpoint_path),
        price_map.clone(),
        price_feed_normalized.to_string(),
        oracle_keypair,
        ChannelId::from(oracle_pubk.to_bytes()),

    )
        .context("Failed to initialize sequencer")?;

    // Setup queues
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Setup price monitor
    let price_monitor = PriceMonitor::new(price_map.clone());

    // Setup time info poll
    let (time_info_tx, _time_info_rx) = tokio::sync::watch::channel(None);
    let poll_interval = Duration::from_millis(20);

    // Register (or check it is already registered) to oracle_register contract
    {
        let file = fs::File::open(args.register_contract_config.as_path())
            .context(format!("Reading {}", args.register_contract_config.as_path().display()))?;
        let reader = std::io::BufReader::new(file);
        let cfg = serde_json::from_reader::<_, RegisterContractInfo>(reader)?;
        debug!("oracle register cfg: {:?}", cfg);
        sequencer_register(cfg, oracle_pubk.as_bytes()).await?;
    }

    let mut set = JoinSet::new();
    set.spawn(async move { time_info_poll( args.node_rest_url.clone(), poll_interval, time_info_tx).await } );

    // SPECDIF: only 1 price source - spec requires at least 3 sources
    match provider {
        "binance" => {
            // Binance uses: "btcusdt"
            let url_str = format!("{}/{}@ticker", price_feed_url, price_feed_provider.to_lowercase());
            let url = Url::parse(&url_str).context("Failed to parse Binance WS URL")?;
            set.spawn(async move {
                binance_fetch::fetch_price(
                    url,
                    price_feed_provider.as_str(),
                    price_feed_normalized,
                    tx
                ).await
            });
        },
        "redstone" => {
            // Can be: redstone, redstone-rapid, redstone-stocks, redstone-custom-urls
            let redstone_provider = "redstone";
            let url_str = format!("https://api.redstone.finance/prices?symbol={}&provider={}", price_feed_provider, redstone_provider);
            let url = Url::parse(&url_str).context("Failed to parse Redstone https URL")?;
            set.spawn(async move {
                redstone_fetch::fetch_price(
                    url,
                    price_feed_provider.as_str(),
                    price_feed_normalized,
                    tx
                ).await
            });
        },
        "pyth" => {
            
            if args.pyth_bearer.is_none() {
                return Err(anyhow!("Pyth bearer token not specified"));
            }
            
            let url_str = format!("{}?ids[]={}", price_feed_url, price_feed_provider);
            let url = Url::parse(&url_str).context("Failed to parse Python SSE URL")?;
            set.spawn(async move {
                pyth_fetch::fetch_price(
                    url,
                    args.pyth_bearer.unwrap().as_str(),
                    price_feed_provider.as_str(),
                    price_feed_normalized,
                    tx
                ).await
            });
        },
        _ => {
            unimplemented!()
        }
    }

    set.spawn(async move { price_monitor.run(&mut rx).await });
    // FIXME: wait_ready ?
    set.spawn(async move { sequencer.run().await });

    while let Some(res) = set.join_next().await {
        match res {
            Ok(Err(e)) => {
                error!("Task error: {:#?}", e);
                error!("Aborting...");
                break;
            },
            Ok(_) => {
                info!("Tasks finished");
                break;
            },
            Err(e) => {
                error!("Join error: {:#?}", e);
                break;
            },
        }
    }

    Ok(())
}

fn setup_tracing() {

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    let console_layer = tracing_subscriber::fmt::layer();

    tracing_subscriber::registry()
        .with(console_layer)
        .with(filter)
        .init();
}

type NormalizedFeed = String;
type ProviderName = String;
type ProviderFeed = String;

#[derive(Debug, Deserialize)]
pub struct PriceProviderConfig {
    pub endpoints: HashMap<String, String>, // TODO: ProviderName, Url
    pub feeds: HashMap<ProviderName, HashMap<NormalizedFeed, ProviderFeed>>,
}

fn parse_provider_config(json_cfg: &Path) -> anyhow::Result<PriceProviderConfig> {

    #[derive(Debug, Deserialize)]
    pub struct PriceProviderConfigRaw {
        pub endpoints: HashMap<String, String>,
        pub feeds: HashMap<NormalizedFeed, HashMap<ProviderName, ProviderFeed>>,
    }

    let json_reader = fs::File::open(json_cfg)?;
    let cfg_raw: PriceProviderConfigRaw = serde_json::from_reader(json_reader)?;

    let cfg = {

        let mut cfg_ = PriceProviderConfig {
            endpoints: cfg_raw.endpoints,
            feeds: HashMap::new(),
        };

        for (normalized_feed, providers) in cfg_raw.feeds {
            for (provider, provider_feed) in providers {
                // Skip empty string entries
                if !provider_feed.trim().is_empty() {
                    cfg_.feeds
                            .entry(provider)
                            .or_default()
                            .insert(normalized_feed.clone(), provider_feed);
                }
            }
        }

        cfg_

    };

    Ok(cfg)
}

fn load_or_create_signing_key(path: &Path) -> anyhow::Result<Ed25519Key> {
    if path.exists() {
        let key_bytes = fs::read(path)
            .context(format!("failed to read key file {}", path.display()))?;
        assert_eq!(key_bytes.len(), ED25519_SECRET_KEY_SIZE,
                   "invalid key file: expected {} bytes, got {}", ED25519_SECRET_KEY_SIZE, key_bytes.len());
        let key_array: [u8; ED25519_SECRET_KEY_SIZE] = key_bytes
            .as_slice()
            .try_into()
            .context("Cannot convert bytes to [u8; 32]")?;
        Ok(Ed25519Key::from_bytes(&key_array))
    } else {
        let mut key_bytes = [0u8; ED25519_SECRET_KEY_SIZE];
        let mut rng = rand::thread_rng();
        rng.fill(&mut key_bytes);
        info!("Start writing key file to: {}", path.display());
        fs::write(path, key_bytes)
            .context(format!("Error while writing key file to {}", path.display()))?
        // .expect("failed to write key file")
        ;
        Ok(Ed25519Key::from_bytes(&key_bytes))
    }
}

