use crate::db::{RowId, DB};
use crate::funding_source::{FundingSource, Invoice, InvoiceState};
use crate::hello_world::GetPredictionResponse;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

pub type Sats = u32;
pub type UserPubKey = secp256k1::PublicKey;
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub struct Prediction {
    pub prediction: String,
    pub judges: Vec<UserPubKey>,
    pub judge_share_ppm: u32,
    pub state: MarketState,
    pub trading_end: DateTime<Utc>,
    pub decision_period: Duration,
    pub judge_count: u32,
    pub cash_out: Option<CashOut>,
}
#[derive(PartialEq, Debug, Clone)]
pub enum JudgeState {
    Nominated,
    Accepted,
    Refused,
    Resolved(bool),
}
impl FromStr for JudgeState {
    type Err = Error;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s {
            "Nominated" => Ok(Self::Nominated),
            "Accepted" => Ok(Self::Accepted),
            "Refused" => Ok(Self::Refused),
            "Resolved" => Ok(Self::Resolved(true)),
            _ => unreachable!(),
        }
    }
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
#[derive(PartialEq, Debug, Clone)]
pub enum MarketState {
    WaitingForJudges,
    Trading,
    TradingStop,
    WaitingForDecision,
    Resolved(bool),
    Refunded(RefundReason),
}
impl FromStr for MarketState {
    type Err = Error;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s {
            "WaitingForJudges" => Ok(Self::WaitingForJudges),
            "Trading" => Ok(Self::Trading),
            "TradingStop" => Ok(Self::TradingStop),
            "WaitingForDecision" => Ok(Self::WaitingForDecision),
            "Resolved" => Ok(Self::Resolved(true)),
            "Refunded" => Ok(Self::Refunded(RefundReason::TimeForDecisionRanOut)),
            _ => unreachable!(),
        }
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
#[derive(PartialEq, Debug, Clone)]
pub enum RefundReason {
    Insolvency,
    TimeForDecisionRanOut,
    Tie,
}
impl FromStr for RefundReason {
    type Err = Error;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s {
            "Insolvency" => Ok(Self::Insolvency),
            "TimeForDecisionRanOut" => Ok(Self::TimeForDecisionRanOut),
            "Tie" => Ok(Self::Tie),
            _ => unreachable!(),
        }
    }
}
impl Display for RefundReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::Insolvency => "Insolvency",
            Self::TimeForDecisionRanOut => "TimeForDecisionRanOut",
            Self::Tie => "Tie",
        };
        write!(f, "{}", output)
    }
}
#[derive(Debug)]
pub struct CashOut {
    user: UserPubKey,
    prediction: RowId,
    amount: Sats,
    invoice: Option<String>,
}
pub struct Mercado {
    db: Box<dyn DB>,
    funding: Box<dyn FundingSource>,
}

pub struct Judge {
    user: UserPubKey,
    prediction: RowId,
    state: JudgeState,
}
#[derive(Debug, Clone)]
pub struct Bet {
    pub(crate) user: UserPubKey,
    pub(crate) prediction: RowId,
    pub(crate) bet: bool,
    pub(crate) amount: Option<Sats>,
    pub(crate) state: BetState,
    pub(crate) fund_invoice: Invoice,
    pub(crate) refund_invoice: Option<Invoice>,
}
#[derive(Debug, Clone)]
pub enum BetState {
    FundInit,
    Funded,
    RefundInit,
    Refunded,
}
impl FromStr for BetState {
    type Err = Error;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s {
            "FundInit" => Ok(Self::FundInit),
            "Funded" => Ok(Self::Funded),
            "RefundInit" => Ok(Self::RefundInit),
            "Refunded" => Ok(Self::Refunded),
            _ => unreachable!(),
        }
    }
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
impl Mercado {
    pub fn new(db: Box<dyn DB>, funding: Box<dyn FundingSource>) -> Self {
        Self { db, funding }
    }
    pub async fn new_prediction(
        &self,
        prediction: String,
        judges: Vec<UserPubKey>,
        judge_count: u32,
        judge_share_ppm: u32,
        trading_end: DateTime<Utc>,
        decision_period: Duration,
    ) -> Result<RowId> {
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
        let id = self
            .db
            .add_prediction(Prediction {
                prediction,
                judges: judges.iter().map(|user| user.clone()).collect(),
                judge_count,
                judge_share_ppm,
                trading_end,
                decision_period,
                state: MarketState::WaitingForJudges,
                cash_out: None,
            })
            .await?;
        Ok(id)
    }
    pub async fn accept_nomination(&mut self, prediction: &RowId, user: &UserPubKey) -> Result<()> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::WaitingForJudges => {}
            _ => return Err(Error::WrongMarketState),
        }
        //TODO Check if judge accepted via Nostr
        match self
            .db
            .set_judge_state(prediction, user, JudgeState::Accepted)
            .await
        {
            Ok(_) => self.try_activate_trading(prediction).await,
            e => e,
        }
    }
    pub async fn refuse_nomination(&mut self, prediction: &RowId, user: &UserPubKey) -> Result<()> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::WaitingForJudges => {}
            _ => return Err(Error::WrongMarketState),
        }
        //TODO Check if judge refused via Nostr
        self.db
            .set_judge_state(prediction, user, JudgeState::Refused)
            .await
    }
    pub async fn make_decision(
        &mut self,
        prediction: &RowId,
        user: &UserPubKey,
        decision: bool,
    ) -> Result<()> {
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
                    return Err(Error::WrongMarketState);
                }
            }
            _ => return Err(Error::WrongMarketState),
        }
        //TODO Check if judge made decision via Nostr
        match self
            .db
            .set_judge_state(prediction, user, JudgeState::Resolved(decision))
            .await
        {
            Ok(_) => self.try_resolve(prediction).await,
            e => e,
        }
    }
    async fn try_resolve(&mut self, prediction: &RowId) -> Result<()> {
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
                return Err(Error::Tie);
            }
            Ordering::Greater => {
                self.db
                    .set_prediction_state(prediction, MarketState::Resolved(true))
                    .await?
            }
        }
        let cash_out = match self.calculate_cash_out(prediction).await {
            Err(Error::Insolvency) => {
                self.db
                    .set_prediction_state(
                        prediction,
                        MarketState::Refunded(RefundReason::Insolvency),
                    )
                    .await?;
                Err(Error::Insolvency)
            }
            e => e,
        }?;
        self.db.set_cash_out(prediction, cash_out).await?;
        Ok(())
    }
    async fn calculate_cash_out(&self, prediction: &RowId) -> Result<HashMap<UserPubKey, Sats>> {
        if let MarketState::Resolved(outcome) = self.db.get_prediction_state(prediction).await? {
            let bets = self.db.get_prediction_bets(prediction, outcome).await?;
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
                        if let Some(user_cash_out) = user_cash_outs.remove(&judge) {
                            user_cash_outs.insert(judge, user_cash_out + cash_out);
                        } else {
                            user_cash_outs.insert(judge, cash_out);
                        }
                    }
                }
            }

            // Check solvency after calculation
            if user_cash_out_amount + judge_cash_out_amount > outcome_amount + non_outcome_amount {
                return Err(Error::Insolvency);
            }
            Ok(user_cash_outs)
        } else {
            Err(Error::MarketNotResolved)
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
    async fn try_activate_trading(&mut self, prediction: &RowId) -> Result<()> {
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
        prediction: &RowId,
        user: &UserPubKey,
        bet: bool,
    ) -> Result<Invoice> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Trading => {
                if self.db.get_trading_end(prediction).await? < Utc::now() {
                    self.db
                        .set_prediction_state(prediction, MarketState::WaitingForDecision)
                        .await?;
                    return Err(Error::WrongMarketState);
                }
            }
            _ => return Err(Error::WrongMarketState),
        }
        let invoice = self.funding.create_invoice().await?;
        self.db
            .create_bet(prediction, user, bet, invoice.clone())
            .await?;
        //TODO Wait until the invoice is payed or check again later
        self.check_bet(&invoice).await?;
        Ok(invoice)
    }
    pub async fn check_bet(&self, invoice: &String) -> Result<BetState> {
        let bet = self.db.get_bet(invoice).await?;
        match bet.state {
            BetState::FundInit => {
                let fund_invoice_state = self.funding.check_invoice(invoice).await?;
                match fund_invoice_state {
                    InvoiceState::Settled(amount) => {
                        if let MarketState::Trading =
                            self.db.get_prediction_state(&bet.prediction).await?
                        {
                            self.db.settle_bet(invoice, amount).await?;
                            Ok(BetState::Funded)
                        } else {
                            self.db.init_bet_refund(invoice, None).await?;
                            Ok(BetState::RefundInit)
                        }
                    }
                    _ => Ok(BetState::FundInit),
                }
            }
            BetState::RefundInit => {
                let refund_invoice_state = self
                    .funding
                    .check_invoice(&bet.refund_invoice.unwrap())
                    .await?;
                match refund_invoice_state {
                    InvoiceState::Settled(_refund_amount) => {
                        self.db.settle_bet_refund(invoice).await?;
                        Ok(BetState::Refunded)
                    }
                    InvoiceState::Failed => {
                        self.db.init_bet_refund(invoice, None).await?;
                        Ok(BetState::RefundInit)
                    }
                    _ => Ok(BetState::RefundInit),
                }
            }
            state => Ok(state),
        }
    }
    pub async fn cancel_bet(
        &mut self,
        invoice: &Invoice,
        refund_invoice: &Invoice,
    ) -> Result<BetState> {
        let bet = self.db.get_bet(invoice).await?;
        let state = self.check_bet(invoice).await?;
        let market_state = self.db.get_prediction_state(&bet.prediction).await?;
        match state {
            BetState::Funded => {
                match market_state {
                    MarketState::Trading => {
                        if self.db.get_trading_end(&bet.prediction).await? < Utc::now() {
                            self.db
                                .set_prediction_state(
                                    &bet.prediction,
                                    MarketState::WaitingForDecision,
                                )
                                .await?;
                            return Err(Error::WrongMarketState);
                        }
                    }
                    MarketState::Refunded(_) => {}
                    _ => return Err(Error::WrongMarketState),
                }
                self.db
                    .init_bet_refund(invoice, Some(refund_invoice))
                    .await?;
                self.funding
                    .pay_invoice(&refund_invoice, bet.amount.unwrap())
                    .await?;
                Ok(BetState::RefundInit)
            }
            BetState::RefundInit => {
                if let None = bet.refund_invoice {
                    self.db
                        .init_bet_refund(invoice, Some(refund_invoice))
                        .await?;
                    self.funding
                        .pay_invoice(&refund_invoice, bet.amount.unwrap())
                        .await?;
                    Ok(BetState::RefundInit)
                } else {
                    todo!()
                }
            }
            state => Ok(state),
        }
    }
    pub async fn cash_out_user(
        &mut self,
        prediction: &RowId,
        user: &UserPubKey,
        invoice: &Invoice,
    ) -> Result<Sats> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Resolved { .. } => {}
            _ => return Err(Error::WrongMarketState),
        }
        let (cash_out_invoice, amount) = self.db.get_cash_out(prediction, user).await?;
        if let Some(cash_out_invoice) = cash_out_invoice {
            match self.funding.check_invoice(&cash_out_invoice).await? {
                InvoiceState::Created | InvoiceState::Failed => {
                    if *invoice != cash_out_invoice {
                        self.db
                            .set_cash_out_invoice(prediction, user, invoice.clone())
                            .await?;
                        self.funding.pay_invoice(invoice, amount).await?;
                    } else {
                        self.funding.pay_invoice(&cash_out_invoice, amount).await?;
                    }
                    Ok(amount)
                }
                InvoiceState::PayInit(_) => {
                    if *invoice == cash_out_invoice {
                        Err(Error::Other(
                            "Cash out was already initialised and is still pending",
                        ))
                    } else {
                        Err(Error::Other("Cash out was already initialised with another invoice which is still pending"))
                    }
                }
                InvoiceState::Settled(_) => {
                    if *invoice == cash_out_invoice {
                        Err(Error::Other("Cash out was already successfully payed out"))
                    } else {
                        Err(Error::Other(
                            "Cash out was already successfully payed out with another invoice",
                        ))
                    }
                }
            }
        } else {
            self.db
                .set_cash_out_invoice(prediction, user, invoice.clone())
                .await?;
            self.funding.pay_invoice(invoice, amount).await?;
            Ok(amount)
        }
    }
    async fn get_outcome_judge_count(&self, prediction: &RowId) -> Result<u32> {
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
        Err(Error::WrongMarketState)
    }
    pub async fn get_prediction_bets(&self, prediction: &RowId, bet: bool) -> Result<Sats> {
        let bets = self.db.get_prediction_bets(prediction, bet).await?;
        Ok(bets.values().sum())
    }
    pub async fn get_user_prediction_bets(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
    ) -> Result<Vec<Bet>> {
        self.db.get_user_prediction_bets(user, prediction).await
    }
    #[cfg(test)]
    async fn force_decision_period(&self, prediction: &RowId) -> Result<()> {
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Trading => {
                self.db
                    .set_prediction_state(prediction, MarketState::WaitingForDecision)
                    .await
            }
            _ => Err(Error::WrongMarketState),
        }
    }
    #[cfg(test)]
    async fn pay_bet(&self, invoice: &Invoice, amount: Sats) -> Result<()> {
        self.funding.pay_invoice(invoice, amount).await?;
        self.check_bet(invoice).await?;
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum Error {
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
    #[error("The market has the wrong state to execute this operation")]
    WrongMarketState,
    #[error("There is no CashOut for '{0}'")]
    NoCashOutFor(UserPubKey),
    #[error("")]
    MarketCreation(MarketCreationError),
    #[error("")]
    InvoiceWasNotPaid,
    #[error("")]
    BetGotRefunded,
    #[error("")]
    SQLiteError(#[from] sqlx::Error),
    #[error("")]
    InvoiceDoesntExist,
    #[error("")]
    Other(&'static str),
}
impl From<MarketCreationError> for Error {
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
    use crate::db::SQLite;
    use crate::funding_source::TestFundingSource;
    use secp256k1::{generate_keypair, rand};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn it_works() {
        let (_, u1) = generate_keypair(&mut rand::thread_rng());
        let (_, u2) = generate_keypair(&mut rand::thread_rng());
        let (_, u3) = generate_keypair(&mut rand::thread_rng());
        let (_, j1) = generate_keypair(&mut rand::thread_rng());
        let (_, j2) = generate_keypair(&mut rand::thread_rng());
        let (_, j3) = generate_keypair(&mut rand::thread_rng());

        let mut market = Mercado::new(
            Box::new(SQLite::new().await),
            Box::new(TestFundingSource::default()),
        );
        let prediction = "it_works".to_string();
        let prediction = market
            .new_prediction(
                "It works".to_string(),
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
        let i1 = market.add_bet(&prediction, &u1, true).await.unwrap();
        let i2 = market.add_bet(&prediction, &u2, true).await.unwrap();
        let i3 = market.add_bet(&prediction, &u3, true).await.unwrap();
        market.pay_bet(&i1, 100);
        market.pay_bet(&i2, 100);
        market.pay_bet(&i3, 100);
        market.force_decision_period(&prediction).await.unwrap();
        market.make_decision(&prediction, &j1, true).await.unwrap();
        market.make_decision(&prediction, &j2, true).await.unwrap();
        market.make_decision(&prediction, &j3, true).await.unwrap();
        assert_eq!(
            market
                .cash_out_user(&prediction, &u1, &"i1".to_string())
                .await
                .unwrap(),
            89
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &u2, &"i2".to_string())
                .await
                .unwrap(),
            89
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &u3, &"i3".to_string())
                .await
                .unwrap(),
            89
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &j1, &"i4".to_string())
                .await
                .unwrap(),
            10
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &j2, &"i5".to_string())
                .await
                .unwrap(),
            10
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &j3, &"i6".to_string())
                .await
                .unwrap(),
            10
        );
    }
}
