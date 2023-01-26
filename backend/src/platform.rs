use crate::db::{Prediction, DB};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::collections::HashMap;
use thiserror::Error;

pub type Sats = u32;
pub type Username = String;

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
        //TODO Cash out all users and judges:
        // Adjust the balances of all users to reflect the cash out
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
            _ => return Err(MarketError::WrongMarketState),
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
    fn cash_out_user(&mut self, prediction: &String, user: &Username) -> Result<u32, MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Resolved { .. } => {
                let cash_out = self.db.remove_cash_out_user(prediction, user)?;
                let (bet_true, bet_false) =
                    self.db.get_user_bets_of_prediction(user, prediction)?;
                let bets = bet_true.unwrap_or(0) + bet_false.unwrap_or(0);
                self.db.remove_balance(user, bets)?;
                self.db.add_balance(user, cash_out)?;
                Ok(cash_out)
            }
            _ => Err(MarketError::WrongMarketState),
        }
    }
    fn cash_out_judge(
        &mut self,
        prediction: &String,
        judge: &Username,
    ) -> Result<u32, MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Resolved { .. } => {
                let cash_out = self.db.remove_cash_out_judge(prediction, judge)?;
                self.db.add_balance(judge, cash_out)?;
                Ok(cash_out)
            }
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
    pub fn create_user(&self, user: Username) -> Result<(), MarketError> {
        self.db.create_user(user)
    }
    pub fn delete_user(&self, user: &Username) -> Result<(), MarketError> {
        self.db.delete_user(user)
    }
    pub fn deposit(&self, user: &Username, amount: Sats) -> Result<(), MarketError> {
        self.db.add_balance(user, amount)
    }
    pub fn withdraw(&self, user: &Username, amount: Sats) -> Result<(), MarketError> {
        self.db.remove_balance(user, amount)
    }
    /// Returns balance excluding locked funds
    pub fn get_balance(&self, user: &Username) -> Result<Sats, MarketError> {
        self.db.get_balance(user)
    }
    /// Returns balance that is locked in bets
    pub fn get_locked_balance(&self, user: &Username) -> Result<Sats, MarketError> {
        self.db.get_locked_balance(user)
    }
    #[cfg(test)]
    fn force_decision_period(&self, prediction: &String) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction)? {
            MarketState::Trading => self.db.set_prediction_state(prediction, MarketState::WaitingForDecision),
            _ => Err(MarketError::WrongMarketState),
        }
    }
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

#[allow(unused)]
#[cfg(test)]
mod test {
    use super::*;
    use crate::db::TestDB;
    use std::rc::Rc;
    use std::sync::Mutex;
    #[test]
    fn it_works() {
        let (u1, u2, u3, j1, j2, j3) = (
            "user1".to_string(),
            "user2".to_string(),
            "user3".to_string(),
            "judge1".to_string(),
            "judge2".to_string(),
            "judge3".to_string(),
        );
        let mut market = Backend::new(DB::Test(Rc::new(Mutex::new(TestDB::default()))));
        let prediction = "it_works".to_string();
        market
            .new_prediction(
                "It works".to_string(),
                prediction.clone(),
                vec![j1.clone(), j2.clone(), j3.clone()],
                3,
                100000,
                Utc::now() + Duration::days(3),
                Duration::days(1),
            )
            .unwrap();
        market.create_user(u1.clone()).unwrap();
        market.create_user(u2.clone()).unwrap();
        market.create_user(u3.clone()).unwrap();
        market.create_user(j1.clone()).unwrap();
        market.create_user(j2.clone()).unwrap();
        market.create_user(j3.clone()).unwrap();
        market.accept_nomination(&prediction, &j1).unwrap();
        market.accept_nomination(&prediction, &j2).unwrap();
        market.accept_nomination(&prediction, &j3).unwrap();
        market.deposit(&u1, 100).unwrap();
        market.deposit(&u2, 100).unwrap();
        market.deposit(&u3, 100).unwrap();
        market.add_bet(&prediction, &u1, true, 100).unwrap();
        market.add_bet(&prediction, &u2, true, 100).unwrap();
        market.add_bet(&prediction, &u3, true, 100).unwrap();
        market.force_decision_period(&prediction).unwrap();
        market.make_decision(&prediction, &j1, true).unwrap();
        market.make_decision(&prediction, &j2, true).unwrap();
        market.make_decision(&prediction, &j3, true).unwrap();
        assert_eq!(market.cash_out_user(&prediction, &u1), Ok(89));
        assert_eq!(market.cash_out_user(&prediction, &u2), Ok(89));
        assert_eq!(market.cash_out_user(&prediction, &u3), Ok(89));
        assert_eq!(market.cash_out_judge(&prediction, &j1), Ok(10));
        assert_eq!(market.cash_out_judge(&prediction, &j2), Ok(10));
        assert_eq!(market.cash_out_judge(&prediction, &j3), Ok(10));
    }
}
