use serde::{Serialize, Deserialize};

#[derive(Debug, Deserialize)]
pub struct HermesPriceEvent {
    pub binary: Option<BinaryUpdate>,
    pub parsed: Option<Vec<ParsedUpdate>>,
}

#[derive(Debug, Deserialize)]
pub struct BinaryUpdate {
    pub encoding: String,
    pub data: Vec<String>,
}

#[derive(Default)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedUpdate {
    pub id: String,
    pub price: PriceInfo,
    pub ema_price: Option<PriceInfo>,
}

#[derive(Default)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceInfo {
    pub price: String,
    pub conf: String,
    pub expo: i32,
    pub publish_time: i64,
}
