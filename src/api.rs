use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use anyhow::bail;
use chrono::{DateTime, Utc};
use log::debug;
use reqwest::StatusCode;
use secp256k1::ecdsa::Signature;
use serde::{Deserialize, Serialize};

pub type Sats = u32;
pub type UserPubKey = secp256k1::PublicKey;
pub type RowId = i64;
pub type Invoice = String;

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
    pub invoice: Invoice,
    pub amount: Sats,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CancelBetRequest {
    pub invoice: Invoice,
    pub refund_invoice: Invoice,
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

// Responses
#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub struct PredictionOverviewResponse {
    pub id: RowId,
    pub name: String,
    pub state: MarketState,
    pub judge_share_ppm: u32,
    pub judge_count: u32,
    pub trading_end: DateTime<Utc>,
    pub decision_period_sec: u32,
}
#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub struct UserResponse {
    pub user: UserPubKey,
    pub username: Option<String>,
    pub role: UserRole,
}

// helper functions
pub fn map_any_err_and_code(e: anyhow::Error) -> (StatusCode, String) {
    debug!("Error: {:#}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e))
}
pub fn map_any_err(e: anyhow::Error) -> String {
    debug!("Error: {:#}", e);
    format!("{:?}", e)
}

// Types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bet {
    pub user: UserPubKey,
    pub prediction: RowId,
    pub bet: bool,
    pub amount: Option<Sats>,
    pub state: BetState,
    pub fund_invoice: Invoice,
    pub refund_invoice: Option<Invoice>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetState {
    FundInit,
    Funded,
    RefundInit,
    Refunded,
}
impl Display for BetState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::FundInit => "FundInit",
            Self::Funded => "Funded",
            Self::RefundInit => "RefundInit",
            Self::Refunded => "Refunded",
        };
        write!(f, "{}", output)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Judge {
    pub user: UserPubKey,
    pub prediction: RowId,
    pub state: JudgeState,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgePublic {
    pub user: UserPubKey,
    pub prediction: RowId,
}
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum JudgeState {
    Nominated,
    Accepted,
    Refused,
    Resolved(bool),
}
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum MarketState {
    WaitingForJudges,
    Trading,
    TradingStop,
    WaitingForDecision,
    Resolved(bool),
    Refunded(RefundReason),
}
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum RefundReason {
    Insolvency,
    TimeForDecisionRanOut,
    Tie,
}
impl Display for JudgeState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::Nominated => "Nominated",
            Self::Accepted => "Accepted",
            Self::Refused => "Refused",
            Self::Resolved(_) => "Resolved",
        };
        write!(f, "{}", output)
    }
}
impl Display for MarketState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::WaitingForJudges => "WaitingForJudges",
            Self::Trading => "Trading",
            Self::TradingStop => "TradingStop",
            Self::WaitingForDecision => "WaitingForDecision",
            Self::Resolved(_) => "Resolved",
            Self::Refunded(_) => "Refunded",
        };
        write!(f, "{}", output)
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UserRole {
    User,
    Admin,
    Root,
}
impl Display for UserRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::User => "User",
            Self::Admin => "Admin",
            Self::Root => "Root",
        };
        write!(f, "{}", output)
    }
}
impl FromStr for UserRole {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "User" => Ok(Self::User),
            "Admin" => Ok(Self::Admin),
            "Root" => Ok(Self::Root),
            e => bail!("Couldn't deserialize to UserRole: {}", e),
        }
    }
}
impl Default for UserRole {
    fn default() -> Self {
        UserRole::User
    }
}
