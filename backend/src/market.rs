use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use surrealdb::sql::{Datetime, Duration, Id};

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
    pub id: String,
    pub user: String,
    pub market: String,
    pub option: String,
    pub amount: Sats,
}
pub struct Market {
    pub assumption: String,
    pub trading_end: Datetime,
    pub decision_period: Duration,
    pub judge_share: f64,
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
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum MercadoError {
    NotEnoughJudges,
    EvenJudgeAmount,
    TradingEndToEarly,
    NotEnoughFunds,
    UserDoesntExist,
    UserAlreadyExists,
    BetDoesntExist,
    MarketDoesntExist,
    MarketAlreadyExists,
    JudgeDoesntExist,
    NominationAlreadyAccepted,
    QueryFailed,
    WrongQueryResponseStructure,
    TradingStopped,
    JudgeShareNotInRange,
    DecisionPeriodToShort,
}
impl Display for MercadoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
