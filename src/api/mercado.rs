use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};

pub type Sats = i64;
pub type UserPubKey = secp256k1::PublicKey;
pub type RowId = i64;
pub type Payment = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bet {
    pub user: UserPubKey,
    pub prediction: RowId,
    pub bet: bool,
    pub amount: Sats,
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
    WaitingForDecision,
    Resolved(bool),
    Refunded(RefundReason),
    Stopped,
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
pub fn calculate_user_cash_out(
    bet_amount: i64,
    outcome_amount: i64,
    non_outcome_amount: i64,
    judge_share_ppm: u32,
) -> Sats {
    //! If the calculation of shares leads to decimals we truncate to not give
    //! out to many sats by accident which would lead to an insolvent market.
    //! We keep the sats that don't get handed back to the user.
    //!
    //! This ends up being a few sats for calculating the judge share
    //! and usually at least one sat for each user because user_share calculation
    //! almost always leads to decimals.
    let total_amount = Decimal::from(outcome_amount + non_outcome_amount);
    let outcome_amount = Decimal::from(outcome_amount);
    let bet_amount = Decimal::from(bet_amount);
    let user_share = bet_amount / outcome_amount;
    let judge_share = Decimal::new(judge_share_ppm.into(), 6);

    let mut out = (total_amount - total_amount * judge_share).trunc();
    out = (out * user_share).trunc();
    out.to_i64().unwrap()
}
pub fn calculate_judge_cash_out(
    outcome_judges: u32,
    outcome_amount: i64,
    non_outcome_amount: i64,
    judge_share_ppm: u32,
) -> Sats {
    //! See [`calculate_user_cash_out()`]
    let total_amount = Decimal::from(outcome_amount + non_outcome_amount);
    let outcome_judges = Decimal::from(outcome_judges);
    let judge_share = Decimal::new(judge_share_ppm.into(), 6);

    let mut out = (total_amount * judge_share).trunc();
    out = (out / outcome_judges).trunc();
    out.to_i64().unwrap()
}
