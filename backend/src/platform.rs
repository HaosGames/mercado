use crate::db::DB;
use crate::funding_source::{FundingSource, Wallet, WalletAccess};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::collections::HashMap;
use thiserror::Error;
use crate::hello_world::GetPredictionResponse;

pub type Sats = u32;
pub type UserPubKey = nostr_sdk::prelude::PublicKey;

#[derive(Debug)]
pub struct Prediction {
    pub prediction: String,
    pub bets_true: HashMap<UserPubKey, Sats>,
    pub bets_false: HashMap<UserPubKey, Sats>,
    pub judges: HashMap<UserPubKey, JudgeState>,
    pub judge_share_ppm: u32,
    pub state: MarketState,
    pub trading_end: DateTime<Utc>,
    pub decision_period: Duration,
    pub judge_count: u32,
    pub cash_out: Option<CashOut>,
    pub wallet: WalletAccess,
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
    pub users: HashMap<UserPubKey, u32>,
    pub judges: HashMap<UserPubKey, u32>,
}

pub struct Mercado {
    db: DB,
    funding: FundingSource,
}

impl Mercado {
    pub fn new(db: DB, funding: FundingSource) -> Self {
        Self { db, funding }
    }
    pub async fn new_prediction(
        &self,
        prediction: String,
        id: String,
        judges: Vec<UserPubKey>,
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
        self.db
            .add_prediction(
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
                    wallet: self.funding.new_wallet(),
                },
            )
            .await?;
        Ok(())
    }
    pub async fn accept_nomination(
        &mut self,
        prediction: &String,
        judge: &UserPubKey,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::WaitingForJudges => {}
            _ => return Err(MarketError::WrongMarketState),
        }
        //TODO Check if judge accepted via Nostr
        match self
            .db
            .set_judge_accepted_if_nominated(prediction, judge)
            .await
        {
            Ok(_) => self.try_activate_trading(prediction).await,
            e => e,
        }
    }
    pub async fn refuse_nomination(
        &mut self,
        prediction: &String,
        user: &UserPubKey,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::WaitingForJudges => {}
            _ => return Err(MarketError::WrongMarketState),
        }
        //TODO Check if judge refused via Nostr
        self.db
            .set_judge_refused_if_nominated(prediction, user)
            .await
    }
    pub async fn make_decision(
        &mut self,
        prediction: &String,
        user: &UserPubKey,
        decision: bool,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::WaitingForDecision => {
                if self.db.get_trading_end(prediction).await?
                    + self.db.get_decision_period(prediction).await?
                    < Utc::now()
                {
                    self.db
                        .set_prediction_state(
                            prediction,
                            MarketState::Refunded(RefundReason::TimeForDecisionRanOut),
                        )
                        .await?;
                    return Err(MarketError::WrongMarketState);
                }
            }
            _ => return Err(MarketError::WrongMarketState),
        }
        //TODO Check if judge made decision via Nostr
        match self
            .db
            .set_judge_resolved_if_accepted(prediction, user, decision)
            .await
        {
            Ok(_) => self.try_resolve(prediction).await,
            e => e,
        }
    }
    async fn try_resolve(&mut self, prediction: &String) -> Result<(), MarketError> {
        let mut true_count = 0;
        let mut false_count = 0;
        for state in self.db.get_judge_states(prediction).await? {
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
            Ordering::Less => {
                self.db
                    .set_prediction_state(prediction, MarketState::Resolved(false))
                    .await?
            }
            Ordering::Equal => {
                self.db
                    .set_prediction_state(prediction, MarketState::Refunded(RefundReason::Tie))
                    .await?;
                return Err(MarketError::Tie);
            }
            Ordering::Greater => {
                self.db
                    .set_prediction_state(prediction, MarketState::Resolved(true))
                    .await?
            }
        }
        let cash_out = match self.calculate_cash_out(prediction).await {
            Err(MarketError::Insolvency) => {
                self.db
                    .set_prediction_state(
                        prediction,
                        MarketState::Refunded(RefundReason::Insolvency),
                    )
                    .await?;
                Err(MarketError::Insolvency)
            }
            e => e,
        }?;
        //TODO Cash out all users and judges:
        // Adjust the balances of all users to reflect the cash out
        self.db.set_cash_out(prediction, cash_out).await?;
        Ok(())
    }
    async fn calculate_cash_out(&self, prediction: &String) -> Result<CashOut, MarketError> {
        if let MarketState::Resolved(outcome) = self.db.get_prediction_state(prediction).await? {
            let bets = self.db.get_bets(prediction, outcome).await?;
            let outcome_amount = self.get_prediction_bets(prediction, outcome).await?;
            let non_outcome_amount = self.get_prediction_bets(prediction, !outcome).await?;

            // Calculate users
            let mut user_cash_outs = HashMap::new();
            let mut user_cash_out_amount = 0;
            for (user, bet_amount) in bets {
                let cash_out = Self::calculate_user_cash_out(
                    bet_amount,
                    outcome_amount,
                    non_outcome_amount,
                    self.db.get_judge_share_ppm(prediction).await?,
                );
                user_cash_out_amount += cash_out;
                user_cash_outs.insert(user.clone(), cash_out);
            }

            // Calculate judges
            let mut judge_cash_outs = HashMap::new();
            let mut judge_cash_out_amount = 0;
            let judge_outcome_count = self.get_outcome_judge_count(prediction).await?;
            for (judge, state) in self.db.get_judges(prediction).await? {
                if let JudgeState::Resolved(decision) = state {
                    if decision == outcome {
                        let cash_out = Self::calculate_judge_cash_out(
                            judge_outcome_count,
                            outcome_amount,
                            non_outcome_amount,
                            self.db.get_judge_share_ppm(prediction).await?,
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
        out.to_u32().unwrap()
    }
    pub fn calculate_judge_cash_out(
        outcome_judges: u32,
        outcome_amount: u32,
        non_outcome_amount: u32,
        judge_share_ppm: u32,
    ) -> Sats {
        //! See [`calculate_user_cash_out()`]
        let total_amount = Decimal::from(outcome_amount + non_outcome_amount);
        let outcome_judges = Decimal::from(outcome_judges);
        let judge_share = Decimal::new(judge_share_ppm.into(), 6);

        let mut out = (total_amount * judge_share).trunc();
        out = (out / outcome_judges).trunc();
        out.to_u32().unwrap()
    }
    async fn try_activate_trading(&mut self, prediction: &String) -> Result<(), MarketError> {
        let mut accepted_count = 0;
        for state in self.db.get_judge_states(prediction).await? {
            if state == JudgeState::Accepted {
                accepted_count += 1;
            }
        }
        if accepted_count == self.db.get_judge_count(prediction).await? {
            self.db
                .set_prediction_state(prediction, MarketState::Trading)
                .await?;
        }
        Ok(())
    }
    pub async fn add_bet(
        &mut self,
        prediction: &String,
        user: &UserPubKey,
        bet: bool,
        amount: u32,
    ) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Trading => {
                if self.db.get_trading_end(prediction).await? < Utc::now() {
                    self.db
                        .set_prediction_state(prediction, MarketState::WaitingForDecision)
                        .await?;
                    return Err(MarketError::WrongMarketState);
                }
            }
            _ => return Err(MarketError::WrongMarketState),
        }
        let wallet = self.db.get_prediction_wallet(prediction).await?;
        let invoice = self.funding.create_invoice(&wallet)?;
        //TODO There could be the case where the invoice is payed after the check
        // but the bet was not added.
        if !self.funding.invoice_is_paid(&invoice)? {
            return Err(MarketError::InvoiceWasNotPaid);
        }
        self.db.add_bet_amount(prediction, user, bet, amount).await
    }
    pub async fn cancel_bet(
        &mut self,
        prediction: &String,
        user: &UserPubKey,
        bet: bool,
        wallet: &Wallet,
    ) -> Result<Sats, MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Trading => {
                if self.db.get_trading_end(prediction).await? < Utc::now() {
                    self.db
                        .set_prediction_state(prediction, MarketState::WaitingForDecision)
                        .await?;
                    return Err(MarketError::WrongMarketState);
                }
            }
            _ => return Err(MarketError::WrongMarketState),
        }
        let amount = self.db.remove_bets(prediction, user, bet).await?;
        self.funding.send_to_wallet(
            &self.db.get_prediction_wallet(prediction).await?,
            wallet,
            amount,
        )?;
        Ok(amount)
    }
    pub async fn refund_user(
        &mut self,
        prediction: &String,
        user: &UserPubKey,
        wallet: &Wallet,
    ) -> Result<Sats, MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Refunded(_) => {
                let mut out = 0;
                out += self.db.remove_bets(prediction, user, true).await?;
                out += self.db.remove_bets(prediction, user, false).await?;
                self.funding.send_to_wallet(
                    &self.db.get_prediction_wallet(prediction).await?,
                    wallet,
                    out,
                )?;
                Ok(out)
            }
            _ => Err(MarketError::WrongMarketState),
        }
    }
    pub async fn cash_out_user(
        &mut self,
        prediction: &String,
        user: &UserPubKey,
        wallet: &Wallet,
    ) -> Result<Sats, MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Resolved { .. } => {
                let cash_out = self.db.pop_cash_out_user(prediction, user).await?;
                self.funding.send_to_wallet(
                    &self.db.get_prediction_wallet(prediction).await?,
                    wallet,
                    cash_out,
                )?;
                Ok(cash_out)
            }
            _ => Err(MarketError::WrongMarketState),
        }
    }
    pub async fn cash_out_judge(
        &mut self,
        prediction: &String,
        judge: &UserPubKey,
        wallet: &Wallet,
    ) -> Result<Sats, MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Resolved { .. } => {
                let cash_out = self.db.pop_cash_out_judge(prediction, judge).await?;
                self.funding.send_to_wallet(
                    &self.db.get_prediction_wallet(prediction).await?,
                    wallet,
                    cash_out,
                )?;
                Ok(cash_out)
            }
            _ => Err(MarketError::WrongMarketState),
        }
    }
    async fn get_outcome_judge_count(&self, prediction: &String) -> Result<u32, MarketError> {
        if let MarketState::Resolved(outcome) = self.db.get_prediction_state(prediction).await? {
            let mut count = 0;
            for state in self.db.get_judge_states(prediction).await? {
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
    pub async fn get_prediction_bets(
        &self,
        prediction: &String,
        bet: bool,
    ) -> Result<Sats, MarketError> {
        let bets = self.db.get_bets(prediction, bet).await?;
        Ok(bets.values().sum())
    }
    pub async fn get_bets(
        &self,
        prediction: &String,
        user: &UserPubKey,
    ) -> Result<(Option<Sats>, Option<Sats>), MarketError> {
        self.db.get_user_bets_of_prediction(user, prediction).await
    }
    pub async fn get_prediction(&self, prediction: &String) -> Result<GetPredictionResponse, MarketError> {
        todo!()
    }
    #[cfg(test)]
    async fn force_decision_period(&self, prediction: &String) -> Result<(), MarketError> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Trading => {
                self.db
                    .set_prediction_state(prediction, MarketState::WaitingForDecision)
                    .await
            }
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
    NoCashOutFor(UserPubKey),
    #[error("")]
    MarketCreation(MarketCreationError),
    #[error("")]
    InvoiceWasNotPaid,
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
    use nostr_sdk::prelude::Keys;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn it_works() {
        let (u1, u2, u3, j1, j2, j3) = (
            Keys::generate_from_os_random()
                .key_pair()
                .unwrap()
                .public_key(),
            Keys::generate_from_os_random()
                .key_pair()
                .unwrap()
                .public_key(),
            Keys::generate_from_os_random()
                .key_pair()
                .unwrap()
                .public_key(),
            Keys::generate_from_os_random()
                .key_pair()
                .unwrap()
                .public_key(),
            Keys::generate_from_os_random()
                .key_pair()
                .unwrap()
                .public_key(),
            Keys::generate_from_os_random()
                .key_pair()
                .unwrap()
                .public_key(),
        );
        let mut market = Mercado::new(
            DB::Test(Arc::new(Mutex::new(TestDB::default()))),
            FundingSource::Test,
        );
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
            .await
            .unwrap();
        market.accept_nomination(&prediction, &j1).await.unwrap();
        market.accept_nomination(&prediction, &j2).await.unwrap();
        market.accept_nomination(&prediction, &j3).await.unwrap();
        market.add_bet(&prediction, &u1, true, 100).await.unwrap();
        market.add_bet(&prediction, &u2, true, 100).await.unwrap();
        market.add_bet(&prediction, &u3, true, 100).await.unwrap();
        market.force_decision_period(&prediction).await.unwrap();
        market.make_decision(&prediction, &j1, true).await.unwrap();
        market.make_decision(&prediction, &j2, true).await.unwrap();
        market.make_decision(&prediction, &j3, true).await.unwrap();
        assert_eq!(
            market
                .cash_out_user(&prediction, &u1, &Wallet::default())
                .await,
            Ok(89)
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &u2, &Wallet::default())
                .await,
            Ok(89)
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &u3, &Wallet::default())
                .await,
            Ok(89)
        );
        assert_eq!(
            market
                .cash_out_judge(&prediction, &j1, &Wallet::default())
                .await,
            Ok(10)
        );
        assert_eq!(
            market
                .cash_out_judge(&prediction, &j2, &Wallet::default())
                .await,
            Ok(10)
        );
        assert_eq!(
            market
                .cash_out_judge(&prediction, &j3, &Wallet::default())
                .await,
            Ok(10)
        );
    }
}
