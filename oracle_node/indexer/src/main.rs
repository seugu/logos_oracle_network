mod args;
mod indexer;
mod register_contract;
mod prices_contract;

use std::collections::HashSet;
use std::time::Duration;
use anyhow::Context;
// third-party
use clap::Parser as _;
use spel_framework::serde_json;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};
// internal
// use common::time_poller;
use common::{time_info_poll, RegisterContractInfo, PricesContractInfo};
use indexer::Indexer;
use crate::args::IndexerArgs;
use crate::indexer::channel_discover;

#[tokio::main]
async fn main() {
    let args = IndexerArgs::parse();
    drop(run(args).await);
}

pub async fn run(args: IndexerArgs) -> anyhow::Result<()> {

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Oracle Indexer starting up...");
    info!("  Logos blockchain Node: {}", args.node_url);

    // oracle_register contract info
    let oracle_register_cfg = {
        let file = std::fs::File::open(args.register_contract_config.as_path())
            .context(format!("Reading {}", args.register_contract_config.as_path().display()))?;
        let reader = std::io::BufReader::new(file);
        let cfg = serde_json::from_reader::<_, RegisterContractInfo>(reader)?;
        debug!("oracle register contract cfg: {:?}", cfg);
        cfg
    };

    // oracle_prices contract info
    let oracle_prices_cfg = {
        let file = std::fs::File::open(args.prices_contract_config.as_path())
            .context(format!("Reading {}", args.prices_contract_config.as_path().display()))?;
        let reader = std::io::BufReader::new(file);
        let cfg = serde_json::from_reader::<_, PricesContractInfo>(reader)?;
        debug!("oracle prices contract cfg: {:?}", cfg);
        cfg
    };

    /*
    let (watch_channel_ids_tx, watch_channel_ids_rx) = watch::channel(HashSet::new());
    let _channel_discover_handle = tokio::spawn(async move {
        channel_discover(Duration::from_millis(100), watch_channel_ids_tx, oracle_register_cfg).await
    });
    let (watch_time_info_tx, watch_time_info_rx) = watch::channel(None);
    let node_rest_url = args.node_rest_url.clone();
    let _watch_time_info_handle = tokio::spawn(async move  {
        time_info_poll(node_rest_url, Duration::from_millis(100), watch_time_info_tx).await
    });

    // Wait for some initial channel_ids & time_info
    let mut chi = watch_channel_ids_rx.clone();
    let mut ti = watch_time_info_rx.clone();
    let (_first, _second) = tokio::join!(
        chi.wait_for(|state| !state.is_empty()),
        ti.wait_for(|state| state.is_some()),
    );

    info!("Channel ids & time_info working, starting indexer now...");

    // TODO: pass current_slot so indexer can wait 1 / 2 slots to start its work?
    let indexer = match Indexer::new(
        &args.node_rest_url,
        args.node_auth_username,
        args.node_auth_password,
        watch_time_info_rx.clone(),
        watch_channel_ids_rx.clone(),
        oracle_prices_cfg,
    ) {
        Ok(i) => i,
        Err(e) => {
            error!("Indexer initialization failed: {e}");
            std::process::exit(1);
        }
    };

    info!("Launching indexer...");
    // indexer.run().await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    info!("Exiting indexer main...");
    */

    let mut set = JoinSet::new();
    let (watch_time_info_tx, watch_time_info_rx) = watch::channel(None);
    let node_rest_url = args.node_rest_url.clone();
    set.spawn(async move {
        time_info_poll(node_rest_url, Duration::from_millis(100), watch_time_info_tx).await
    });
    let (watch_channel_ids_tx, watch_channel_ids_rx) = watch::channel(HashSet::new());
    set.spawn(async move {
            channel_discover(Duration::from_millis(100), watch_channel_ids_tx, oracle_register_cfg).await
    });

    let indexer = match Indexer::new(
        &args.node_rest_url,
        args.node_auth_username,
        args.node_auth_password,
        watch_time_info_rx.clone(),
        watch_channel_ids_rx.clone(),
        oracle_prices_cfg,
    ) {
        Ok(i) => i,
        Err(e) => {
            error!("Indexer initialization failed: {e}");
            std::process::exit(1);
        }
    };
    
    set.spawn(async move {
        indexer.run().await
    });
    
    info!("Indexer join all...");
    set.join_all().await;
    info!("Indexer stopped.");

    Ok(())
}