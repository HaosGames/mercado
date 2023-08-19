use serde::{Deserialize, Serialize};

pub type Sats = u32;
pub type UserPubKey = secp256k1::PublicKey;

#[derive(Debug, Deserialize, Serialize)]
pub struct PublicPrediction {
    pub prediction: String,
    pub judges: Vec<UserPubKey>,
    pub judge_share_ppm: u32,
    pub trading_end: i64,
    pub decision_period_sec: u32,
    pub judge_count: u32,
    pub bets_true: Sats,
    pub bets_false: Sats,
}
