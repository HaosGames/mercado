use crate::db::DB;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use surrealdb::sql::Id;

pub type Sats = i64;

pub struct Funds {
    user: Id,
    market: Option<Id>,
    prediction: Option<bool>,
    amount: Sats,
}
pub enum FundState {
    Deposited,
    Withdrawn,
    BetMade { market: Id, bet: bool },
    BetCancelled,
    Converted,
}
#[derive(Debug, PartialEq)]
pub struct Bet {
    pub user: String,
    pub market: String,
    pub option: String,
    pub amount: Sats,
}
pub struct PredictionMarket {
    assumption: String,
    trading_end: DateTime<Utc>,
    decision_period: Duration,
    state: MarketState,
    judges: Vec<Id>,
}
pub enum MarketState {
    Created,
    WaitingForJudges,
    Trading,
    TradingStop,
    WaitingForDecision,
    Settled,
    Refunded,
}
pub struct Judge {
    market: Id,
    user: Id,
    state: JudgeState,
}
pub enum JudgeState {
    Nominated,
    Accepted,
    Refused,
    Resolved(bool),
}
pub struct User {
    pub id: String,
    pub sats: Sats,
}
impl PredictionMarket {
    fn new(
        question: String,
        trading_end: DateTime<Utc>,
        judges: Vec<Id>,
    ) -> Result<Self, MercadoError> {
        if judges.len() < 3 {
            return Err(MercadoError::NotEnoughJudges);
        }
        if judges.len() % 2 == 0 {
            return Err(MercadoError::EvenJudgeAmount);
        }
        if trading_end < Utc::now() {
            return Err(MercadoError::TradingEndToEarly);
        }
        Ok(Self {
            assumption: question,
            trading_end,
            state: MarketState::Created,
            judges,
            decision_period: todo!(),
        })
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum MercadoError {
    NotEnoughJudges,
    EvenJudgeAmount,
    TradingEndToEarly,
    NotEnoughFunds,
    UserDoesntExist,
    BetDoesntExist,
    MarketDoesntExist,
    JudgeDoesntExist,
    NominationAlreadyAccepted,
    QueryFailed,
    WrongQueryResponseStructure,
}
