use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RedstonePriceEvent {
    /// The asset symbol (e.g., "BTC")
    pub symbol: String,

    /// The oracle provider (e.g., "redstone")
    pub provider: String,

    /// The resolved price (RedStone sends this as an f64)
    pub value: f64,

    /// Timestamp of the price resolution (Unix epoch in milliseconds)
    pub timestamp: u64,
}