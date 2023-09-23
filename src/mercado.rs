use crate::api::*;
use crate::{db::DB, funding_source::FundingSource};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use log::{debug, error, trace};
use reqwest::StatusCode;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use secp256k1::ecdsa::Signature;
use secp256k1::hashes::sha256::Hash;
use secp256k1::rand::distributions::Alphanumeric;
use secp256k1::rand::Rng;
use secp256k1::{rand, Message};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

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
impl FromStr for JudgeState {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s {
            "Nominated" => Ok(Self::Nominated),
            "Accepted" => Ok(Self::Accepted),
            "Refused" => Ok(Self::Refused),
            "Resolved(true)" => Ok(Self::Resolved(true)),
            "Resolved(false)" => Ok(Self::Resolved(false)),
            e => {
                error!(
                    "Error trying to serialize \"{}\" from db into JudgeState",
                    e
                );
                unreachable!()
            }
        }
    }
}
impl FromStr for MarketState {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s {
            "WaitingForJudges" => Ok(Self::WaitingForJudges),
            "Trading" => Ok(Self::Trading),
            "Stopped" => Ok(Self::Stopped),
            "WaitingForDecision" => Ok(Self::WaitingForDecision),
            "Resolved(true)" => Ok(Self::Resolved(true)),
            "Resolved(false)" => Ok(Self::Resolved(false)),
            "Refunded(TimeForDecisionRanOut)" => {
                Ok(Self::Refunded(RefundReason::TimeForDecisionRanOut))
            }
            "Refunded(Insolvency)" => Ok(Self::Refunded(RefundReason::Insolvency)),
            "Refunded(Tie)" => Ok(Self::Refunded(RefundReason::Tie)),
            e => {
                error!(
                    "Error trying to serialize \"{}\" from db into MarketState",
                    e
                );
                unreachable!()
            }
        }
    }
}
impl FromStr for RefundReason {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s {
            "Insolvency" => Ok(Self::Insolvency),
            "TimeForDecisionRanOut" => Ok(Self::TimeForDecisionRanOut),
            "Tie" => Ok(Self::Tie),
            e => {
                error!(
                    "Error trying to serialize \"{}\" from db into RefundReason",
                    e
                );
                unreachable!()
            }
        }
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
    db: Arc<Box<dyn DB + Send + Sync>>,
    funding: Arc<Box<dyn FundingSource + Send + Sync>>,
    test: bool,
}

impl FromStr for BetState {
    type Err = anyhow::Error;
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
impl Mercado {
    pub async fn new(
        db: Box<dyn DB + Send + Sync>,
        funding: Box<dyn FundingSource + Send + Sync>,
        admins: Vec<String>,
        test: bool,
    ) -> Result<Self> {
        let me = Self {
            db: Arc::new(db),
            funding: Arc::new(funding),
            test,
        };
        for admin in admins {
            me.db
                .update_user_role(UserPubKey::from_str(admin.as_str())?, UserRole::Root)
                .await?;
        }
        Ok(me)
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
            return Err(anyhow!(
                "There were {} nominated judges but there need to be at least {}",
                judges.len(),
                judge_count
            ));
        }
        if judge_share_ppm > 1000000 {
            return Err(anyhow!(
                "judge_share_ppm was {} but needs to be lower than 100.000",
                judge_share_ppm
            ));
        }
        if trading_end < Utc::now() + Duration::days(2) {
            return Err(anyhow!(
                "Trading end was at {} but needs to be after {}",
                trading_end,
                Utc::now() + Duration::days(2)
            ));
        }
        if decision_period < Duration::days(1) {
            return Err(anyhow!(
                "Decision period was {} but needs to be at least {}",
                decision_period.num_seconds(),
                Duration::days(1).num_seconds()
            ));
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
    pub async fn accept_nomination(
        &mut self,
        prediction: &RowId,
        user: &UserPubKey,
        access: AccessRequest,
    ) -> Result<()> {
        self.check_access_for_user(user.clone(), access).await?;
        match self
            .db
            .get_prediction_state(prediction)
            .await
            .context("failed to get prediction state")?
        {
            MarketState::WaitingForJudges => {}
            _ => bail!("Wrong market state"),
        }
        //TODO Check if judge accepted via Nostr
        match self
            .db
            .set_judge_state(prediction, user, JudgeState::Accepted)
            .await
            .context("failed to set judge state")
        {
            Ok(_) => self.try_activate_trading(prediction).await,
            e => e,
        }
    }
    pub async fn refuse_nomination(
        &mut self,
        prediction: &RowId,
        user: &UserPubKey,
        access: AccessRequest,
    ) -> Result<()> {
        self.check_access_for_user(user.clone(), access).await?;
        match self.db.get_prediction_state(prediction).await? {
            MarketState::WaitingForJudges => {}
            _ => bail!("Wrong market state"),
        }
        //TODO Check if judge refused via Nostr
        self.db
            .set_judge_state(prediction, user, JudgeState::Refused)
            .await
    }
    pub async fn make_decision(
        &mut self,
        prediction: &RowId,
        judge: &UserPubKey,
        decision: bool,
        access: AccessRequest,
    ) -> Result<()> {
        self.check_access_for_user(judge.clone(), access).await?;
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
                    bail!("Wrong market state");
                }
            }
            MarketState::Trading => {
                if self.db.get_trading_end(prediction).await? < Utc::now() {
                    self.db
                        .set_prediction_state(prediction, MarketState::WaitingForDecision)
                        .await?;
                } else {
                    bail!("Can't make decision while market is still trading")
                }
            }
            _ => bail!("Wrong market state"),
        }
        match self.db.get_judge_state(prediction.clone(), &judge).await? {
            JudgeState::Nominated | JudgeState::Refused => {
                bail!("Judge did not accept the nomination")
            }
            JudgeState::Resolved(_) | JudgeState::Accepted => {}
        }
        //TODO Check if judge made decision via Nostr
        match self
            .db
            .set_judge_state(prediction, judge, JudgeState::Resolved(decision))
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
                bail!("There was a decision tie between an even number of judges")
            }
            Ordering::Greater => {
                self.db
                    .set_prediction_state(prediction, MarketState::Resolved(true))
                    .await?
            }
        }
        let cash_out = self.calculate_cash_out(prediction.clone()).await?;
        self.db.set_cash_out(prediction, cash_out).await?;
        Ok(())
    }
    async fn calculate_cash_out(&self, prediction: RowId) -> Result<HashMap<UserPubKey, Sats>> {
        if let MarketState::Resolved(outcome) = self.db.get_prediction_state(&prediction).await? {
            let bets = self
                .db
                .get_prediction_bets_aggregated(prediction, outcome)
                .await?;
            let outcome_amount = self
                .get_prediction_bets_aggregated(prediction, outcome)
                .await?;
            let non_outcome_amount = self
                .get_prediction_bets_aggregated(prediction, !outcome)
                .await?;

            // Calculate users
            let mut user_cash_outs = HashMap::new();
            let mut user_cash_out_amount = 0;
            for (user, bet_amount) in bets {
                let cash_out = Self::calculate_user_cash_out(
                    bet_amount,
                    outcome_amount,
                    non_outcome_amount,
                    self.db.get_judge_share_ppm(&prediction).await?,
                );
                if cash_out == 0 {
                    continue;
                }
                user_cash_out_amount += cash_out;
                user_cash_outs.insert(user.clone(), cash_out);
            }

            // Calculate judges
            let mut judge_cash_out_amount = 0;
            let judge_outcome_count = self.get_outcome_judge_count(&prediction).await?;
            for (judge, state) in self.db.get_prediction_judges_mapped(&prediction).await? {
                if let JudgeState::Resolved(decision) = state {
                    if decision == outcome {
                        let cash_out = Self::calculate_judge_cash_out(
                            judge_outcome_count,
                            outcome_amount,
                            non_outcome_amount,
                            self.db.get_judge_share_ppm(&prediction).await?,
                        );
                        if cash_out == 0 {
                            continue;
                        }
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
                self.db
                    .set_prediction_state(
                        &prediction,
                        MarketState::Refunded(RefundReason::Insolvency),
                    )
                    .await?;
                error!(
                    "For some reason the cash out calculation made the prediction {} \
                   insolvent. Bets are being refunded",
                    prediction
                );
                error!("The following should be true but wasn't:");
                error!("user_cash_out_amount + judge_cash_out_amount > outcome_amount + non_outcome_amount");
                error!(
                    "{} + {} > {} + {}",
                    user_cash_out_amount, judge_cash_out_amount, outcome_amount, non_outcome_amount
                );
                bail!(
                    "For some reason the cash out calculation made the prediction {} \
                  insolvent. Bets are being refunded",
                    prediction
                )
            }
            Ok(user_cash_outs)
        } else {
            bail!("Market not resolved")
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
        for state in self
            .db
            .get_judge_states(prediction)
            .await
            .context("failed to get judge states")?
        {
            if state == JudgeState::Accepted {
                accepted_count += 1;
            }
        }
        if accepted_count
            == self
                .db
                .get_judge_count(prediction)
                .await
                .context("failed to get judge count")?
        {
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
        access: AccessRequest,
    ) -> Result<Payment> {
        self.check_access_for_user(user.clone(), access).await?;
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Trading => {
                if self.db.get_trading_end(prediction).await? < Utc::now() {
                    self.db
                        .set_prediction_state(prediction, MarketState::WaitingForDecision)
                        .await?;
                    debug!("Triggered trading end because someone tried betting after trading end");
                    bail!("Trading ended");
                }
            }
            _ => bail!("Prediction has to be Trading to be able to bet on it"),
        }
        let invoice = self.funding.create_payment().await?;
        self.db
            .create_bet(prediction, user, bet, invoice.clone())
            .await?;
        //TODO Wait until the invoice is payed or check again later
        //self.check_bet(&invoice).await?;
        Ok(invoice)
    }
    pub async fn check_bet(&self, invoice: &String, access: AccessRequest) -> Result<BetState> {
        let bet = self.db.get_bet(invoice).await?;
        self.check_access_for_user(bet.user, access).await?;
        match bet.state {
            BetState::FundInit => {
                let fund_invoice_state = self.funding.check_payment(invoice).await?;
                match fund_invoice_state {
                    PaymentState::Settled(amount) => {
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
                    .check_payment(&bet.refund_invoice.unwrap())
                    .await?;
                match refund_invoice_state {
                    PaymentState::Settled(_refund_amount) => {
                        self.db.settle_bet_refund(invoice).await?;
                        Ok(BetState::Refunded)
                    }
                    PaymentState::Failed => {
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
        invoice: &Payment,
        refund_invoice: &Payment,
        access: AccessRequest,
    ) -> Result<BetState> {
        let bet = self.db.get_bet(invoice).await?;
        self.check_access_for_user(bet.user, access.clone()).await?;
        let state = self.check_bet(invoice, access.clone()).await?;
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
                            bail!("Wrong market state");
                        }
                    }
                    MarketState::Refunded(_) => {}
                    _ => bail!("Wrong market state"),
                }
                self.db
                    .init_bet_refund(invoice, Some(refund_invoice))
                    .await?;
                self.funding
                    .pay(&refund_invoice, bet.amount.unwrap())
                    .await?;
                Ok(BetState::RefundInit)
            }
            BetState::RefundInit => {
                if let None = bet.refund_invoice {
                    self.db
                        .init_bet_refund(invoice, Some(refund_invoice))
                        .await?;
                    self.funding
                        .pay(&refund_invoice, bet.amount.unwrap())
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
        invoice: &Payment,
        access: AccessRequest,
    ) -> Result<Sats> {
        self.check_access_for_user(user.clone(), access).await?;
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Resolved { .. } => {}
            _ => bail!("Wrong market state"),
        }
        let (cash_out_invoice, amount) = self
            .db
            .get_cash_out(prediction, user)
            .await
            .context("no cash out")?;
        if let Some(cash_out_invoice) = cash_out_invoice {
            match self.funding.check_payment(&cash_out_invoice).await? {
                PaymentState::Created | PaymentState::Failed => {
                    if *invoice != cash_out_invoice {
                        self.db
                            .set_cash_out_invoice(prediction, user, invoice.clone())
                            .await
                            .context("couldn't set cash out invoice")?;
                        self.funding.pay(invoice, amount).await?;
                    } else {
                        self.funding.pay(&cash_out_invoice, amount).await?;
                    }
                    Ok(amount)
                }
                PaymentState::PayInit(_) => {
                    if *invoice == cash_out_invoice {
                        bail!("Cash out was already initialised and is still pending")
                    } else {
                        bail!("Cash out was already initialised with another invoice which is still pending")
                    }
                }
                PaymentState::Settled(_) => {
                    if *invoice == cash_out_invoice {
                        bail!("Cash out was already successfully payed out")
                    } else {
                        bail!("Cash out was already successfully payed out with another invoice")
                    }
                }
            }
        } else {
            self.db
                .set_cash_out_invoice(prediction, user, invoice.clone())
                .await?;
            self.funding.pay(invoice, amount).await?;
            Ok(amount)
        }
    }
    pub async fn get_cash_out(
        &self,
        prediction: RowId,
        user: UserPubKey,
        access: AccessRequest,
    ) -> Result<CashOutRespose> {
        self.check_access_for_user(user.clone(), access).await?;
        let (cash_out_invoice, amount) = self
            .db
            .get_cash_out(&prediction, &user)
            .await
            .context("no cash out")?;
        if let Some(invoice) = cash_out_invoice {
            let state = self
                .funding
                .check_payment(&invoice)
                .await
                .context("Error checking invoice")?;
            Ok(CashOutRespose {
                amount,
                invoice: Some((invoice, state)),
            })
        } else {
            Ok(CashOutRespose {
                amount,
                invoice: None,
            })
        }
    }
    pub async fn get_cash_outs(
        &self,
        prediction: Option<RowId>,
        user: Option<UserPubKey>,
        access: AccessRequest,
    ) -> Result<Vec<(RowId, UserPubKey)>> {
        if let Some(user) = user {
            self.check_access_for_user(user, access).await?;
        } else {
            if let UserRole::User = self.check_access(access).await? {
                bail!("Access Denied: Getting cash_outs of users is prohibited");
            }
        }
        let cash_outs = self.db.get_cash_outs(prediction, user).await?;
        Ok(cash_outs)
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
        bail!("Wrong market state")
    }
    pub async fn get_predictions(&self) -> Result<HashMap<RowId, PredictionOverviewResponse>> {
        self.db.get_predictions().await
    }
    pub async fn get_prediction_overview(
        &self,
        prediction: RowId,
    ) -> Result<PredictionOverviewResponse> {
        self.db.get_prediction_overview(prediction).await
    }

    pub async fn get_prediction_bets_aggregated(
        &self,
        prediction: RowId,
        bet: bool,
    ) -> Result<Sats> {
        let bets = self
            .db
            .get_prediction_bets_aggregated(prediction, bet)
            .await?;
        Ok(bets.values().sum())
    }
    pub async fn get_prediction_ratio(&self, prediction: RowId) -> Result<(Sats, Sats)> {
        self.db.get_prediction_ratio(prediction).await
    }
    pub async fn get_prediction_judges(&self, prediction: RowId) -> Result<Vec<Judge>> {
        self.db.get_prediction_judges(prediction).await
    }
    pub async fn force_decision_period(
        &self,
        prediction: &RowId,
        access: AccessRequest,
    ) -> Result<()> {
        if let UserRole::User = self.check_access(access).await? {
            bail!("Access Denied: Admin only API");
        }
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Trading => {
                self.db
                    .set_prediction_state(prediction, MarketState::WaitingForDecision)
                    .await
            }
            _ => bail!("Wrong market state"),
        }
    }
    pub async fn pay_bet(
        &self,
        invoice: &Payment,
        amount: Sats,
        access: AccessRequest,
    ) -> Result<()> {
        if let UserRole::User = self.check_access(access.clone()).await? {
            bail!("Access Denied: Admin only API");
        }
        self.funding.pay(invoice, amount).await?;
        self.check_bet(invoice, access).await?;
        Ok(())
    }
    pub async fn check_access(&self, access: AccessRequest) -> Result<UserRole> {
        if self.test {
            return Ok(UserRole::Root);
        }
        let (db_sig, last_access) = self
            .db
            .get_last_access(access.user, access.challenge)
            .await
            .context("Error getting session from db")?;
        if access.sig != db_sig {
            debug!(
                "User {} tried to access with invalid access token",
                access.user
            );
            bail!("Access token for user {} is invalid", access.user)
        }
        if last_access < Utc::now() - Duration::days(7) {
            debug!(
                "User {} tried to access after more than 7 days",
                access.user
            );
            bail!("Last access was more than 7 days ago")
        }
        let role = self.db.get_user_role(access.user).await?;
        Ok(role)
    }
    pub async fn create_login_challenge(&mut self, user: UserPubKey) -> Result<String> {
        let challenge: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(30)
            .map(char::from)
            .collect();
        trace!("Generated login challenge {}", challenge);
        self.db.create_session(user, challenge.clone()).await?;
        Ok(challenge)
    }
    pub async fn try_login(
        &mut self,
        user: UserPubKey,
        sig: Signature,
        challenge: String,
    ) -> Result<()> {
        sig.verify(
            &Message::from_hashed_data::<Hash>(challenge.as_bytes()),
            &user,
        )?;
        self.db.update_access_token(user, sig, challenge).await?;
        Ok(())
    }
    pub async fn update_user(
        &self,
        user: UserPubKey,
        name: Option<String>,
        access: AccessRequest,
    ) -> Result<()> {
        self.check_access_for_user(user, access).await?;
        if let Some(name) = name {
            self.db.update_username(user, name).await?;
        }
        Ok(())
    }
    pub async fn check_access_for_user(
        &self,
        user: UserPubKey,
        access: AccessRequest,
    ) -> Result<()> {
        if let UserRole::User = self.check_access(access.clone()).await? {
            if user != access.user {
                bail!("Access Denied: Cannot issue request on behalf of other users");
            }
        }
        Ok(())
    }
    pub async fn get_username(&self, user: UserPubKey) -> Result<Option<String>> {
        self.db.get_username(user).await
    }
    pub async fn get_user(&self, user: UserPubKey, access: AccessRequest) -> Result<UserResponse> {
        self.check_access_for_user(user, access).await?;
        self.db.get_user(user).await
    }
    pub async fn get_judges(
        &self,
        prediction: Option<RowId>,
        user: Option<UserPubKey>,
    ) -> Result<Vec<JudgePublic>> {
        self.db.get_judges(prediction, user).await
    }
    pub async fn get_judge(
        &self,
        prediction: RowId,
        user: UserPubKey,
        access: AccessRequest,
    ) -> Result<Judge> {
        self.check_access_for_user(user, access).await?;
        let state = self.db.get_judge_state(prediction, &user).await?;
        Ok(Judge {
            user,
            prediction,
            state,
        })
    }
    pub async fn get_bets(
        &self,
        prediction: Option<RowId>,
        user: Option<UserPubKey>,
        access: AccessRequest,
    ) -> Result<Vec<Bet>> {
        if let Some(user) = user {
            self.check_access_for_user(user, access).await?;
        } else {
            if let UserRole::User = self.check_access(access).await? {
                bail!("Access Denied: Getting bets of users is prohibited");
            }
        }
        let bets = self.db.get_bets(prediction, user).await?;
        Ok(bets)
    }
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

    fn get_test_access() -> AccessRequest {
        AccessRequest {
            user: UserPubKey::from_str("023d51452445aa81ecc3cfcb82dbfe937707db5c89f9f9d21d64835158df405d8c").unwrap(),
            sig: Signature::from_str("30440220208cef162c7081dafc61004daec32f5a3dadb4c6a1b4c0a479056a4962288d47022069022bc92673f73e9843cea14fa0cc46efa1b1e150339b603444c63035de21ee").unwrap(),
            challenge: "iT1HqC3oaoGjbSZEjAwpGZiCbzjtyz".to_string()
        }
    }

    #[tokio::test]
    async fn it_works() {
        let (_, u1) = generate_keypair(&mut rand::thread_rng());
        let (_, u2) = generate_keypair(&mut rand::thread_rng());
        let (_, u3) = generate_keypair(&mut rand::thread_rng());
        let (_, j1) = generate_keypair(&mut rand::thread_rng());
        let (_, j2) = generate_keypair(&mut rand::thread_rng());
        let (_, j3) = generate_keypair(&mut rand::thread_rng());

        let mut market = Mercado::new(
            Box::new(SQLite::new(None).await),
            Box::new(TestFundingSource::default()),
            vec![],
            true,
        )
        .await
        .unwrap();
        let access = get_test_access();
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
        market
            .accept_nomination(&prediction, &j1, access.clone())
            .await
            .unwrap();
        market
            .accept_nomination(&prediction, &j2, access.clone())
            .await
            .unwrap();
        market
            .accept_nomination(&prediction, &j3, access.clone())
            .await
            .unwrap();
        let i1 = market
            .add_bet(&prediction, &u1, true, access.clone())
            .await
            .unwrap();
        let i2 = market
            .add_bet(&prediction, &u2, true, access.clone())
            .await
            .unwrap();
        let i3 = market
            .add_bet(&prediction, &u3, true, access.clone())
            .await
            .unwrap();
        market.pay_bet(&i1, 100, access.clone()).await.unwrap();
        market.pay_bet(&i2, 100, access.clone()).await.unwrap();
        market.pay_bet(&i3, 100, access.clone()).await.unwrap();
        market
            .force_decision_period(&prediction, access.clone())
            .await
            .unwrap();
        market
            .make_decision(&prediction, &j1, true, access.clone())
            .await
            .unwrap();
        market
            .make_decision(&prediction, &j2, true, access.clone())
            .await
            .unwrap();
        market
            .make_decision(&prediction, &j3, true, access.clone())
            .await
            .unwrap();
        assert_eq!(
            market
                .cash_out_user(&prediction, &u1, &"i1".to_string(), access.clone())
                .await
                .unwrap(),
            89
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &u2, &"i2".to_string(), access.clone())
                .await
                .unwrap(),
            89
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &u3, &"i3".to_string(), access.clone())
                .await
                .unwrap(),
            89
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &j1, &"i4".to_string(), access.clone())
                .await
                .unwrap(),
            10
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &j2, &"i5".to_string(), access.clone())
                .await
                .unwrap(),
            10
        );
        assert_eq!(
            market
                .cash_out_user(&prediction, &j3, &"i6".to_string(), access)
                .await
                .unwrap(),
            10
        );
    }
}
