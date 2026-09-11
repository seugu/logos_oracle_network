use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinancePriceEvent {
    /// Event type (usually "24hrTicker")
    #[serde(rename = "e")]
    pub event_type: String,

    /// Event time (Unix epoch in milliseconds)
    #[serde(rename = "E")]
    pub event_time: u64,

    /// Symbol (e.g., "BTCUSDT")
    #[serde(rename = "s")]
    pub symbol: String,

    /// Last price (Binance sends prices as strings to prevent float precision loss)
    #[serde(rename = "c")]
    pub last_price: String,

    /// Price change
    #[serde(rename = "p")]
    pub price_change: String,

    /// Price change percent
    #[serde(rename = "P")]
    pub price_change_percent: String,
}
