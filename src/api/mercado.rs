use serde::{Deserialize, Serialize};

pub type Sats = u32;
pub type UserPubKey = secp256k1::PublicKey;
pub type RowId = i64;
pub type Payment = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bet {
    pub user: UserPubKey,
    pub prediction: RowId,
    pub bet: bool,
    pub amount: Option<Sats>,
    pub state: BetState,
    pub fund_invoice: Payment,
    pub refund_invoice: Option<Payment>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BetState {
    FundInit,
    Funded,
    RefundInit,
    Refunded,
}
#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
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
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Copy)]
pub enum JudgeState {
    Nominated,
    Accepted,
    Refused,
    Resolved(bool),
}
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Copy)]
pub enum MarketState {
    WaitingForJudges,
    Trading,
    TradingStop,
    WaitingForDecision,
    Resolved(bool),
    Refunded(RefundReason),
}
#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RefundReason {
    Insolvency,
    TimeForDecisionRanOut,
    Tie,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UserRole {
    User,
    Admin,
    Root,
}
