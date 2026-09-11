use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::connect_async;
use tracing::{info, error};
use url::Url;

use common::{BinancePriceEvent, PartialPriceObservation};

pub async fn fetch_price(
    url: Url,
    price_feed: &str,
    price_feed_normalized: &str,
    price_update_queue: UnboundedSender<PartialPriceObservation>) -> anyhow::Result<()> {

    let price_feed_normalized = price_feed_normalized.to_string();

    let mut retry_delay = Duration::from_secs(1);

    loop {
        info!("Connecting to Binance WebSocket stream ({}) for price feed: {}...", url, price_feed);

        match connect_async(url.as_str()).await {
            Ok((ws_stream, _)) => {
                info!("Binance connection opened successfully!");
                retry_delay = Duration::from_secs(1); // Reset backoff on success

                let (_, mut read) = ws_stream.split();

                while let Some(message) = read.next().await {
                    match message {
                        Ok(msg) => {
                            if msg.is_text() {
                                let text = msg.to_text().unwrap_or_default();

                                match serde_json::from_str::<BinancePriceEvent>(text) {
                                    Ok(update_event) => {

                                        if update_event.symbol != price_feed {
                                            error!("Binance price event symbol: {}, expected: {}", update_event.symbol, price_feed);
                                            continue;
                                        }

                                        match PartialPriceObservation::try_from(
                                            (price_feed_normalized.to_string(), update_event)) {
                                            Ok(price_obs) => {
                                                if let Err(e) = price_update_queue.send(price_obs) {
                                                    error!("Error while sending price update event: {}", e);
                                                }
                                            },
                                            Err(e) => {
                                                error!("Error while converting event to price update event: {}", e);
                                            }
                                        }
                                    },
                                    Err(err) => {
                                        error!("Failed to parse Binance JSON: {}\nRaw data: {}", err, text);
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            error!("Fatal stream error: {}", err);
                            break; // Break to trigger reconn
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to connect to Binance: {:#?}", e);
            }
        }

        info!("Reconnecting in {} seconds...", retry_delay.as_secs());
        sleep(retry_delay).await;

        // Exponential backoff capped at 30 seconds[cite: 3]
        retry_delay = std::cmp::min(retry_delay * 2, Duration::from_secs(30));
    }
}