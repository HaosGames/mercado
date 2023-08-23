use chrono::{DateTime, Utc};
use log::debug;
use reqwest::StatusCode;
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
#[derive(Debug, Serialize, Deserialize)]
pub struct CashOutUserRequest {
    pub prediction: RowId,
    pub user: UserPubKey,
    pub invoice: Invoice,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PredictionListItemResponse {
    pub id: RowId,
    pub name: String,
    pub judge_share_ppm: u32,
    pub trading_end: DateTime<Utc>,
    pub decision_period_sec: u32,
    pub bets_true: Sats,
    pub bets_false: Sats,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPredictionOverviewRequest {
    pub prediction: RowId,
    pub user: UserPubKey,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPredictionOverviewResponse {
    pub id: RowId,
    pub name: String,
    pub judge_share_ppm: u32,
    pub trading_end: DateTime<Utc>,
    pub decision_period_sec: u32,
    pub bets_true: Sats,
    pub bets_false: Sats,
    pub user_bets: Vec<Bet>,
}
pub fn map_any_err_and_code(e: anyhow::Error) -> (StatusCode, String) {
    debug!("Error: {:#}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e))
}
pub fn map_any_err(e: anyhow::Error) -> String {
    debug!("Error: {:#}", e);
    format!("{:?}", e)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bet {
    pub(crate) user: UserPubKey,
    pub(crate) prediction: RowId,
    pub(crate) bet: bool,
    pub(crate) amount: Option<Sats>,
    pub(crate) state: BetState,
    pub(crate) fund_invoice: Invoice,
    pub(crate) refund_invoice: Option<Invoice>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetState {
    FundInit,
    Funded,
    RefundInit,
    Refunded,
}
