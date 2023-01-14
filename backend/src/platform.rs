use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};
use thiserror::Error;

pub type Sats = u32;
pub type Username = String;

#[derive(Debug)]
struct Prediction {
    prediction: String,
    bets_true: HashMap<Username, u32>,
    bets_false: HashMap<Username, u32>,
    judges: HashMap<Username, JudgeState>,
    judge_share_ppm: u32,
    state: MarketState,
    trading_end: DateTime<Utc>,
    decision_period: Duration,
    judge_count: u32,
    cash_out: Option<CashOut>,
}
#[derive(PartialEq, Debug, Clone)]
pub enum JudgeState {
    Nominated,
    Accepted,
    Refused,
    Resolved(bool),
}
#[derive(PartialEq, Debug, Clone)]
pub enum MarketState {
    WaitingForJudges,
    Trading,
    TradingStop,
    WaitingForDecision,
    Resolved(bool),
    Refunded(RefundReason),
}
#[derive(PartialEq, Debug, Clone)]
pub enum RefundReason {
    Insolvency,
    TimeForDecisionRanOut,
    Tie,
}
#[derive(Debug)]
pub struct CashOut {
    pub users: HashMap<Username, u32>,
    pub judges: HashMap<Username, u32>,
}

pub struct Backend {
    db: DB,
}

impl Backend {
    pub fn new(db: DB) -> Self {
        Self { db }
    }
    pub fn new_prediction(
        &self,
        prediction: String,
        id: String,
        judges: Vec<Username>,
        judge_count: u32,
        judge_share_ppm: u32,
        trading_end: DateTime<Utc>,
        decision_period: Duration,
    ) -> Result<(), MarketError> {
        if judges.len() < judge_count as usize || judge_count == 0 {
            return Err(MarketCreationError::NotEnoughJudges.into());
        }
        if judge_share_ppm > 1000000 {
            return Err(MarketCreationError::JudgeShareNotInRange.into());
        }
        if trading_end < Utc::now() + Duration::days(2) {
            return Err(MarketCreationError::TradingEndToEarly.into());
        }
        if decision_period < Duration::days(1) {
            return Err(MarketCreationError::DecisionPeriodToShort.into());
        }
        self.db.add_prediction(
            id,
            Prediction {
                prediction,
                bets_true: Default::default(),
                bets_false: Default::default(),
                judges: judges
                    .iter()
                    .map(|user| (user.clone(), JudgeState::Nominated))
                    .collect(),
                judge_count,
                judge_share_ppm,
                trading_end,
                decision_period,
                state: MarketState::WaitingForJudges,
                cash_out: None,
            },
        )?;
        Ok(())
    }
    pub fn accept_nomination(
        &mut self,
        prediction: &String,
        user: &Username,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::WaitingForJudges => {}
            _ => return Err(MarketError::WrongMarketState),
        }
        self.db
            .set_judge_accepted_if_nominated(prediction, user)
            .and_then(|_| Ok(self.try_activate_trading(prediction)?))
    }
    pub fn refuse_nomination(
        &mut self,
        prediction: &String,
        user: &Username,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::WaitingForJudges => {}
            e => return Err(MarketError::WrongMarketState),
        }
        self.db.set_judge_refused_if_nominated(prediction, user)
    }
    pub fn make_decision(
        &mut self,
        prediction: &String,
        user: &Username,
        decision: bool,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Trading => {}
            MarketState::WaitingForDecision => {
                if self.db.get_trading_end(prediction)? + self.db.get_decision_period(prediction)?
                    < Utc::now()
                {
                    self.db.set_prediction_state(
                        prediction,
                        MarketState::Refunded(RefundReason::TimeForDecisionRanOut),
                    )?;
                    return Err(MarketError::WrongMarketState);
                }
            }
            _ => return Err(MarketError::WrongMarketState),
        }
        self.db
            .set_judge_resolved_if_accepted(prediction, user, decision)
            .and_then(|_| self.try_resolve(prediction))
    }
    fn try_resolve(&mut self, prediction: &String) -> Result<(), MarketError> {
        let mut true_count = 0;
        let mut false_count = 0;
        for state in self.db.get_judge_states(prediction)? {
            match state {
                JudgeState::Accepted => {
                    return Ok(());
                }
                JudgeState::Resolved(decision) => {
                    if decision {
                        true_count += 1;
                    } else {
                        false_count += 1;
                    }
                }
                _ => {}
            }
        }
        match true_count.cmp(&false_count) {
            Ordering::Less => self
                .db
                .set_prediction_state(prediction, MarketState::Resolved(false))?,
            Ordering::Equal => {
                self.db
                    .set_prediction_state(prediction, MarketState::Refunded(RefundReason::Tie))?;
                return Err(MarketError::Tie);
            }
            Ordering::Greater => self
                .db
                .set_prediction_state(prediction, MarketState::Resolved(true))?,
        }
        let cash_out = match self.calculate_cash_out(prediction) {
            Err(MarketError::Insolvency) => {
                self.db.set_prediction_state(
                    prediction,
                    MarketState::Refunded(RefundReason::Insolvency),
                )?;
                Err(MarketError::Insolvency)
            }
            e => e,
        }?;
        self.db.set_cash_out(prediction, cash_out)?;
        Ok(())
    }
    fn calculate_cash_out(&self, prediction: &String) -> Result<CashOut, MarketError> {
        if let MarketState::Resolved(outcome) = self.db.get_prediction_state(prediction)? {
            let bets = self.db.get_bets(prediction, outcome)?;
            let outcome_amount = self.get_bets_amount(prediction, outcome)?;
            let non_outcome_amount = self.get_bets_amount(prediction, !outcome)?;

            // Calculate users
            let mut user_cash_outs = HashMap::new();
            let mut user_cash_out_amount = 0;
            for (user, bet_amount) in bets {
                let cash_out = Self::calculate_user_cash_out(
                    bet_amount,
                    outcome_amount,
                    non_outcome_amount,
                    self.db.get_judge_share_ppm(prediction)?,
                );
                user_cash_out_amount += cash_out;
                user_cash_outs.insert(user.clone(), cash_out);
            }

            // Calculate judges
            let mut judge_cash_outs = HashMap::new();
            let mut judge_cash_out_amount = 0;
            let judge_outcome_count = self.get_outcome_judge_count(prediction)?;
            for (judge, state) in self.db.get_judges(prediction)? {
                if let JudgeState::Resolved(decision) = state {
                    if decision == outcome {
                        let cash_out = Self::calculate_judge_cash_out(
                            judge_outcome_count,
                            outcome_amount,
                            non_outcome_amount,
                            self.db.get_judge_share_ppm(prediction)?,
                        );
                        judge_cash_out_amount += cash_out;
                        judge_cash_outs.insert(judge.clone(), cash_out);
                    }
                }
            }

            // Check solvency after calculation
            if user_cash_out_amount + judge_cash_out_amount > outcome_amount + non_outcome_amount {
                return Err(MarketError::Insolvency);
            }
            Ok(CashOut {
                users: user_cash_outs,
                judges: judge_cash_outs,
            })
        } else {
            Err(MarketError::MarketNotResolved)
        }
    }
    pub fn calculate_user_cash_out(
        bet_amount: u32,
        outcome_amount: u32,
        non_outcome_amount: u32,
        judge_share_ppm: u32,
    ) -> u32 {
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
        out.to_u32().unwrap()
    }
    pub fn calculate_judge_cash_out(
        outcome_judges: u32,
        outcome_amount: u32,
        non_outcome_amount: u32,
        judge_share_ppm: u32,
    ) -> u32 {
        //! See [`calculate_user_cash_out()`]
        let total_amount = Decimal::from(outcome_amount + non_outcome_amount);
        let outcome_judges = Decimal::from(outcome_judges);
        let judge_share = Decimal::new(judge_share_ppm.into(), 6);

        let mut out = (total_amount * judge_share).trunc();
        out = (out / outcome_judges).trunc();
        out.to_u32().unwrap()
    }
    fn try_activate_trading(&mut self, prediction: &String) -> Result<(), MarketError> {
        let mut accepted_count = 0;
        for state in self.db.get_judge_states(prediction)? {
            if state == JudgeState::Accepted {
                accepted_count += 1;
            }
        }
        if accepted_count == self.db.get_judge_count(prediction)? {
            self.db
                .set_prediction_state(prediction, MarketState::Trading)?;
        }
        Ok(())
    }
    pub fn add_bet(
        &mut self,
        prediction: &String,
        user: &Username,
        bet: bool,
        amount: u32,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Trading => {
                if self.db.get_trading_end(prediction)? < Utc::now() {
                    self.db
                        .set_prediction_state(prediction, MarketState::WaitingForDecision)?;
                    return Err(MarketError::WrongMarketState);
                }
            }
            e => return Err(MarketError::WrongMarketState),
        }
        self.db.add_bet_amount(prediction, user, bet, amount)
    }
    pub fn cancel_bet(
        &mut self,
        prediction: &String,
        user: &Username,
        bet: bool,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Trading => {
                if self.db.get_trading_end(prediction)? < Utc::now() {
                    self.db
                        .set_prediction_state(prediction, MarketState::WaitingForDecision)?;
                    return Err(MarketError::WrongMarketState);
                }
            }
            _ => return Err(MarketError::WrongMarketState),
        }
        self.db.remove_bets(prediction, user, bet)?;
        Ok(())
    }
    pub fn refund_user(
        &mut self,
        prediction: &String,
        user: &Username,
    ) -> Result<u32, MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Refunded(_) => {
                let mut out = 0;
                out += self.db.remove_bets(prediction, user, true)?;
                out += self.db.remove_bets(prediction, user, false)?;
                Ok(out)
            }
            _ => Err(MarketError::WrongMarketState),
        }
    }
    pub fn cash_out_user(
        &mut self,
        prediction: &String,
        user: &Username,
    ) -> Result<u32, MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Resolved { .. } => self.db.remove_cash_out_user(prediction, user),
            _ => Err(MarketError::WrongMarketState),
        }
    }
    pub fn cash_out_judge(
        &mut self,
        prediction: &String,
        judge: &Username,
    ) -> Result<u32, MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Resolved { .. } => self.db.remove_cash_out_judge(prediction, judge),
            _ => Err(MarketError::WrongMarketState),
        }
    }
    fn get_outcome_judge_count(&self, prediction: &String) -> Result<u32, MarketError> {
        if let MarketState::Resolved(outcome) = self.db.get_prediction_state(prediction)? {
            let mut count = 0;
            for state in self.db.get_judge_states(prediction)? {
                if let JudgeState::Resolved(decision) = state {
                    if decision == outcome {
                        count += 1;
                    }
                }
            }
            return Ok(count);
        }
        Err(MarketError::WrongMarketState)
    }
    fn get_bets_amount(&self, prediction: &String, bet: bool) -> Result<u32, MarketError> {
        let bets = self.db.get_bets(prediction, bet)?;
        Ok(bets.values().sum())
    }
}

pub struct User {
    sats: u32,
}
#[derive(Error, PartialEq, Debug)]
pub enum MarketError {
    #[error("")]
    NotEnoughFunds,
    #[error("")]
    UserDoesntExist,
    #[error("")]
    UserAlreadyExists,
    #[error("")]
    BetDoesntExist,
    #[error("")]
    MarketDoesntExist,
    #[error("")]
    JudgeDoesntExist,
    #[error("")]
    NominationAlreadyAccepted,
    #[error("")]
    QueryFailed,
    #[error("")]
    WrongQueryResponseStructure,
    #[error("")]
    TradingStopped,
    #[error("")]
    JudgeHasWrongState,
    #[error("")]
    NotEnoughAcceptedNominations,
    #[error("")]
    JudgesAlreadyLockedIn,
    #[error("")]
    MarketNotResolved,
    #[error("")]
    Insolvency,
    #[error("No definite decision could be made by the judges")]
    Tie,
    #[error("The market is already trading")]
    TradingActive,
    #[error("The market has the wrong State to execute this operation")]
    WrongMarketState,
    #[error("There is no CashOut for '{0}'")]
    NoCashOutFor(Username),
    #[error("")]
    MarketCreation(MarketCreationError),
}
impl From<MarketCreationError> for MarketError {
    fn from(e: MarketCreationError) -> Self {
        Self::MarketCreation(e)
    }
}
#[derive(Error, PartialEq, Debug)]
pub enum MarketCreationError {
    #[error("")]
    NotEnoughJudges,
    #[error("")]
    EvenJudgeAmount,
    #[error("")]
    TradingEndToEarly,
    #[error("")]
    JudgeShareNotInRange,
    #[error("")]
    DecisionPeriodToShort,
    #[error("")]
    MarketAlreadyExists,
}

pub enum DB {
    Test(Rc<Mutex<TestDB>>),
}
#[derive(Debug, Default)]
pub struct TestDB {
    predictions: HashMap<String, Prediction>,
}
impl DB {
    fn add_prediction(
        &self,
        id: String,
        prediction: Prediction,
    ) -> Result<(), MarketCreationError> {
        match self {
            DB::Test(db) => {
                let mut db = db.lock().unwrap();
                if !db.predictions.contains_key(&id) {
                    db.predictions.insert(id, prediction);
                    Ok(())
                } else {
                    Err(MarketCreationError::MarketAlreadyExists)
                }
            }
        }
    }
    fn get_mut_prediction<'a>(
        db: &'a mut MutexGuard<TestDB>,
        prediction: &'_ String,
    ) -> Result<&'a mut Prediction, MarketError> {
        if let Some(market) = db.predictions.get_mut(prediction) {
            Ok(market)
        } else {
            Err(MarketError::MarketDoesntExist)
        }
    }
    fn get_prediction_state(&self, prediction: &String) -> Result<MarketState, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.state.clone())
            }
        }
    }
    fn set_prediction_state(
        &self,
        prediction: &String,
        state: MarketState,
    ) -> Result<(), MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                prediction.state = state;
                Ok(())
            }
        }
    }
    fn set_judge_accepted_if_nominated(
        &self,
        prediction: &String,
        judge: &Username,
    ) -> Result<(), MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                if let Some(state) = prediction.judges.get_mut(judge) {
                    if *state == JudgeState::Nominated {
                        *state = JudgeState::Accepted;
                        Ok(())
                    } else {
                        Err(MarketError::JudgeHasWrongState)
                    }
                } else {
                    Err(MarketError::JudgeDoesntExist)
                }
            }
        }
    }
    fn set_judge_refused_if_nominated(
        &self,
        prediction: &String,
        judge: &Username,
    ) -> Result<(), MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                if let Some(state) = prediction.judges.get_mut(judge) {
                    if *state == JudgeState::Nominated {
                        *state = JudgeState::Refused;
                        Ok(())
                    } else {
                        Err(MarketError::JudgeHasWrongState)
                    }
                } else {
                    Err(MarketError::JudgeDoesntExist)
                }
            }
        }
    }
    fn set_judge_resolved_if_accepted(
        &self,
        prediction: &String,
        judge: &Username,
        decision: bool,
    ) -> Result<(), MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                if let Some(state) = prediction.judges.get_mut(judge) {
                    if *state == JudgeState::Accepted {
                        *state = JudgeState::Resolved(decision);
                        Ok(())
                    } else {
                        Err(MarketError::JudgeHasWrongState)
                    }
                } else {
                    Err(MarketError::JudgeDoesntExist)
                }
            }
        }
    }
    fn get_trading_end(&self, prediction: &String) -> Result<DateTime<Utc>, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.trading_end.clone())
            }
        }
    }
    fn get_decision_period(&self, prediction: &String) -> Result<Duration, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.decision_period.clone())
            }
        }
    }
    fn get_judges(
        &self,
        prediction: &String,
    ) -> Result<HashMap<Username, JudgeState>, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.judges.clone())
            }
        }
    }
    fn get_judge_states(&self, prediction: &String) -> Result<Vec<JudgeState>, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.judges.values().cloned().collect())
            }
        }
    }
    fn set_cash_out(&self, prediction: &String, cash_out: CashOut) -> Result<(), MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                prediction.cash_out = Some(cash_out);
                Ok(())
            }
        }
    }
    fn get_judge_share_ppm(&self, prediction: &String) -> Result<u32, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.judge_share_ppm)
            }
        }
    }
    fn get_judge_count(&self, prediction: &String) -> Result<u32, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.judge_count)
            }
        }
    }
    fn add_bet_amount(
        &self,
        prediction: &String,
        user: &Username,
        bet: bool,
        amount: Sats,
    ) -> Result<(), MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                let bets = if bet {
                    &mut prediction.bets_true
                } else {
                    &mut prediction.bets_false
                };
                if let Some(bet_amount) = bets.get_mut(user) {
                    *bet_amount += amount;
                } else {
                    bets.insert(user.clone(), amount);
                }
                Ok(())
            }
        }
    }
    fn remove_bets(
        &self,
        prediction: &String,
        user: &Username,
        bet: bool,
    ) -> Result<Sats, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                let bets = if bet {
                    &mut prediction.bets_true
                } else {
                    &mut prediction.bets_false
                };
                if let Some(bet_amount) = bets.remove(user) {
                    Ok(bet_amount)
                } else {
                    Ok(0)
                }
            }
        }
    }
    fn remove_cash_out_user(
        &self,
        prediction: &String,
        user: &Username,
    ) -> Result<Sats, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                if let Some(cash_out) = &mut prediction.cash_out {
                    cash_out
                        .users
                        .remove(user)
                        .ok_or(MarketError::NoCashOutFor(user.clone()))
                } else {
                    Err(MarketError::WrongMarketState)
                }
            }
        }
    }
    fn remove_cash_out_judge(
        &self,
        prediction: &String,
        user: &Username,
    ) -> Result<Sats, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                if let Some(cash_out) = &mut prediction.cash_out {
                    cash_out
                        .judges
                        .remove(user)
                        .ok_or(MarketError::NoCashOutFor(user.clone()))
                } else {
                    Err(MarketError::WrongMarketState)
                }
            }
        }
    }
    fn get_bets(
        &self,
        prediction: &String,
        outcome: bool,
    ) -> Result<HashMap<Username, u32>, MarketError> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().unwrap();
                let prediction = Self::get_mut_prediction(&mut db, prediction)?;
                if outcome {
                    Ok(prediction.bets_true.clone())
                } else {
                    Ok(prediction.bets_false.clone())
                }
            }
        }
    }
}

#[allow(unused)]
#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn it_works() {
        let mut market = Backend::new(DB::Test(Rc::new(Mutex::new(TestDB::default()))));
        let prediction = "it_works".to_string();
        market
            .new_prediction(
                "It works".to_string(),
                prediction.clone(),
                vec!["one".to_string(), "two".to_string(), "three".to_string()],
                3,
                100000,
                Utc::now() + Duration::days(3),
                Duration::days(1),
            )
            .unwrap();
        market
            .accept_nomination(&prediction, &"one".to_string())
            .unwrap();
        market
            .accept_nomination(&prediction, &"two".to_string())
            .unwrap();
        market
            .accept_nomination(&prediction, &"three".to_string())
            .unwrap();
        market
            .add_bet(&prediction, &"user1".to_string(), true, 100)
            .unwrap();
        market
            .add_bet(&prediction, &"user2".to_string(), true, 100)
            .unwrap();
        market
            .add_bet(&prediction, &"user3".to_string(), true, 100)
            .unwrap();
        market
            .make_decision(&prediction, &"one".to_string(), true)
            .unwrap();
        market
            .make_decision(&prediction, &"two".to_string(), true)
            .unwrap();
        market
            .make_decision(&prediction, &"three".to_string(), true)
            .unwrap();
        assert_eq!(
            market.cash_out_user(&prediction, &"user1".to_string()),
            Ok(89)
        );
        assert_eq!(
            market.cash_out_user(&prediction, &"user2".to_string()),
            Ok(89)
        );
        assert_eq!(
            market.cash_out_user(&prediction, &"user3".to_string()),
            Ok(89)
        );
        assert_eq!(
            market.cash_out_judge(&prediction, &"one".to_string()),
            Ok(10)
        );
        assert_eq!(
            market.cash_out_judge(&prediction, &"two".to_string()),
            Ok(10)
        );
        assert_eq!(
            market.cash_out_judge(&prediction, &"three".to_string()),
            Ok(10)
        );
    }
}
