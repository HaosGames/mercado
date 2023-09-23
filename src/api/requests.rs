use chrono::{DateTime, Utc};
use secp256k1::ecdsa::Signature;
use serde::{Deserialize, Serialize};

use super::*;

// Requests
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AccessRequest {
    pub user: UserPubKey,
    pub challenge: String,
    pub sig: Signature,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PostRequest<T> {
    pub access: AccessRequest,
    pub data: T,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoginRequest {
    pub user: UserPubKey,
    pub challenge: String,
    pub sig: Signature,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateUserRequest {
    pub user: UserPubKey,
    pub username: Option<String>,
}
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct NewPredictionRequest {
    pub prediction: String,
    pub judges: Vec<UserPubKey>,
    pub judge_share_ppm: u32,
    pub trading_end: DateTime<Utc>,
    pub decision_period_sec: u32,
    pub judge_count: u32,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NominationRequest {
    pub prediction: RowId,
    pub user: UserPubKey,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddBetRequest {
    pub prediction: RowId,
    pub user: UserPubKey,
    pub bet: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct PayBetRequest {
    pub invoice: Payment,
    pub amount: Sats,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CancelBetRequest {
    pub invoice: Payment,
    pub refund_invoice: Payment,
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
    pub invoice: Payment,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PredictionRequest {
    pub prediction: RowId,
    pub user: Option<UserPubKey>,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PredictionUserRequest {
    pub prediction: Option<RowId>,
    pub user: Option<UserPubKey>,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct JudgeRequest {
    pub prediction: RowId,
    pub user: UserPubKey,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CashOutRequest {
    pub prediction: RowId,
    pub user: UserPubKey,
}
