use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type Sats = u32;
pub type UserPubKey = secp256k1::PublicKey;
pub type RowId = i64;

#[derive(Debug, Deserialize, Serialize)]
pub struct NewPredictionRequest {
    pub prediction: String,
    pub judges: Vec<UserPubKey>,
    pub judge_share_ppm: u32,
    pub trading_end: DateTime<Utc>,
    pub decision_period_sec: u32,
    pub judge_count: u32,
    pub bets_true: Sats,
    pub bets_false: Sats,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptNominationRequest {
    pub prediction: RowId,
    pub user: UserPubKey,
}
