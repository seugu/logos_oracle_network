use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::sync::Arc;
// third-party
use anyhow::Context;
use dashmap::DashMap;
use url::Url;
use prost::Message;
use tracing::{info, debug, error, warn};
use bytes::Bytes;
// third-party - logos
use logos_blockchain_zone_sdk::{
    adapter::NodeHttpClient,
    sequencer::{
        Event, SequencerCheckpoint,
        SequencerClient,
        ZoneSequencer
    },
};
use lb_common_http_client::{BasicAuthCredentials, CommonHttpClient};
use lb_core::mantle::ops::channel::{
    ChannelId,
    inscribe::{Inscription, MAX_BYTES},
};
use lb_key_management_system_service::{
    keys::Ed25519Key,
    keys::secured_key::SecuredKey
};
// internal
use crate::zone_state::InMemoryZoneState;
use common::PartialPriceObservation;
use crate::lon::PriceObservation;

pub struct Sequencer {
    sequencer: ZoneSequencer<NodeHttpClient>,
    client: SequencerClient,
    state: InMemoryZoneState,
    pub checkpoint_path: PathBuf,
    price_map: Arc<DashMap<String, VecDeque<PartialPriceObservation>>>,
    price_feed: String,
    oracle_channel_keypair: Ed25519Key
}

impl Sequencer {

    pub(crate) fn new(
        node_endpoint: &str,
        node_auth_username: Option<String>,
        node_auth_password: Option<String>,
        checkpoint_path: PathBuf,
        price_map: Arc<DashMap<String, VecDeque<PartialPriceObservation>>>,
        price_feed: String,
        oracle_signing_key: Ed25519Key,
        oracle_channel_id: ChannelId,
    ) -> anyhow::Result<Self> {

        let checkpoint = None;
        let signing_key = oracle_signing_key;
        let channel_id = oracle_channel_id;

        info!("Sequence channel id: {}", channel_id);
        info!("Sequence channel id: {:?}", channel_id.as_ref());

        let node_url = Url::parse(node_endpoint)?; // .map_err(|e| anyhow!(e))?;
        let basic_auth = node_auth_username
            .map(|username| BasicAuthCredentials::new(username, node_auth_password));

        let node = NodeHttpClient::new(CommonHttpClient::new(basic_auth), node_url);
        let sequencer = ZoneSequencer::init(channel_id, signing_key.clone(), node, checkpoint);
        let client = sequencer.client();

        Ok(Self {
            sequencer,
            client,
            state: InMemoryZoneState::default(),
            checkpoint_path,
            price_map,
            price_feed,
            oracle_channel_keypair: signing_key
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {

        info!("Starting sequencer...");

        let sequencer_client = self.client.clone();

        let price_map = self.price_map.clone();
        let price_feed = self.price_feed.clone();
        let oracle_channel_keypair = self.oracle_channel_keypair.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_mins(1));

            // Wait for sequencer to be ready
            info!("Waiting for sequencer to be ready...");
            let _ready_rx = sequencer_client.subscribe_ready();
            // TODO / FIXME
            /*
            info!("Waiting for sequencer to be ready 2...");
            let _unused = tokio::time::timeout(
                Duration::from_secs(5),
                ready_rx.wait_for(|r| *r)
            )
                .await
                .inspect_err(|e| info!("e1: {}", e))?
                .inspect_err(|e| info!("e2: {}", e))?;
            // info!("unused: {:?}", _unused);
            drop(_unused);
            */

            debug!("Sequencer ready, loop...");
            let mut round: i64 = 1000;

            loop {

                // take prices from price_map
                let prices = if let Some(mut price_feed_ref_mut) = price_map.get_mut(&price_feed) {
                    std::mem::take(price_feed_ref_mut.value_mut())
                } else {
                    continue
                };

                // info!("price map: {:?}", price_map);
                let Some(price_latest) = prices.back() else {
                    // info!("No prices...");
                    // interval.tick().await;
                    continue
                };

                let obs = {
                    round = round.saturating_add(1);
                    let mut obs = PriceObservation {
                        feed_id: price_latest.feed_id.clone(),
                        price: price_latest.price,
                        decimals: price_latest.decimals,
                        round, // TODO: need Logos RPC doc
                        timestamp: price_latest.timestamp,
                        oracle_id: oracle_channel_keypair.public_key().to_bytes().to_vec(),
                        signature: vec![],
                        membership_proof: vec![], // TODO: need LEZ register contract
                    };

                    let mut to_hash: Vec<u8> = vec![];
                    to_hash.extend(obs.feed_id.as_bytes());
                    to_hash.extend(obs.price.to_le_bytes().as_slice());
                    to_hash.extend(obs.decimals.to_le_bytes().as_slice());
                    to_hash.extend(obs.round.to_le_bytes().as_slice());
                    to_hash.extend(obs.timestamp.to_le_bytes().as_slice());
                    to_hash.extend(obs.oracle_id.clone());
                    // SPECDIF: current impl use sequencer.key (ED25519) to sign the PriceObservation
                    //          spec requires to generate a BIP-340 Schnorr sig
                    match oracle_channel_keypair.sign(&Bytes::from(to_hash)) {
                        Ok(sig) => {
                            obs.signature = sig.to_bytes().to_vec();
                            debug!("price observation: {:?}", obs);
                            obs
                        },
                        Err(e) => {
                            warn!("Unable to sign price observation: {}", e);
                            continue;
                        }
                    }
                };

                let payload_bytes = obs.encode_to_vec();
                debug!("payload bytes len: {}", payload_bytes.len());
                debug!("max bytes for inscription: {:?}", MAX_BYTES);

                let inscription = Inscription::try_from(payload_bytes)
                    .map_err(|e| SequencerError::InscriptionTooLarge(e.to_string()))
                    .unwrap();

                info!("Publishing...");
                let ts = std::time::Instant::now();
                if let Err(e) = sequencer_client.publish(inscription).await {
                    error!("failed to publish batch: {e}");
                } else {
                    let elapsed = ts.elapsed();
                    debug!("Submitted price update in {} milliseconds", elapsed.as_millis());
                }

                // Wait for 1 minutes between 2 prices update
                debug!("Sequencer waiting...");
                interval.tick().await;
            }

            // Ok::<(), anyhow::Error>(())
        });

        loop {
            let event = self.sequencer.next_event().await;
            // println!("Handle event: {:?}", event);
            handle_event(event, &mut self.sequencer, &mut self.state, &self.checkpoint_path)?;
        }
    }

}

/*
fn generate_oracle_id(_path: &Path) -> anyhow::Result<Keypair> {
    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();
    let keypair = Keypair::new(&secp, &mut rng);
    Ok(keypair)
}
*/

#[derive(Debug, thiserror::Error)]
pub enum SequencerError {
    // #[error("URL parse error: {0}")]
    // Url(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Inscription too large: {0}")]
    InscriptionTooLarge(String),
}

fn handle_event(
    event: Event,
    _sequencer: &mut ZoneSequencer<NodeHttpClient>,
    _state: &mut InMemoryZoneState,
    checkpoint_path: &Path,
) -> anyhow::Result<()> {
    match event {
        Event::Ready => {
            info!("Sequencer ready");
        },
        Event::BlocksProcessed { checkpoint, channel_update: _channel_update, finalized: _finalized } => {
            // println!("BlocksProcessed");
            save_checkpoint(checkpoint_path, &checkpoint)?;
        },
        Event::MempoolPending(_) | Event::TurnNotification { .. } => {}
    }

    Ok(())
}

fn save_checkpoint(path: &Path, checkpoint: &SequencerCheckpoint) -> anyhow::Result<()> {
    let data = serde_json::to_vec(checkpoint)
        .context(format!("Failed to serialize checkpoint: {:?}", checkpoint))?;
    fs::write(path, data)
        .context(format!("Failed to write checkpoint to {}", path.display()))?;
    Ok(())
}