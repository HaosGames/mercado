use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone, Eq)]
pub struct PredictionOverviewResponse {
    pub id: RowId,
    pub name: String,
    pub state: MarketState,
    pub judge_share_ppm: u32,
    pub judge_count: u32,
    pub trading_end: DateTime<Utc>,
    pub decision_period_sec: u32,
    pub ratio: (Sats, Sats),
}
#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub struct UserResponse {
    pub user: UserPubKey,
    pub username: Option<String>,
    pub role: UserRole,
}
#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub struct DepositResponse {
    pub invoice: Invoice,
    pub id: RowId,
}
