use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type Sats = u32;
pub type UserPubKey = secp256k1::PublicKey;
pub type RowId = i64;
pub type Invoice = String;

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
#[derive(Debug, Serialize, Deserialize)]
pub struct AddBetRequest {
    pub prediction: RowId,
    pub user: UserPubKey,
    pub bet: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PayBetRequest {
    pub invoice: Invoice,
    pub amount: Sats,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct MakeDecisionRequest {
    pub prediction: RowId,
    pub judge: UserPubKey,
    pub decision: bool,
}
