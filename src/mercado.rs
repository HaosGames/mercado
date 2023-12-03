use crate::api::*;
use crate::db::DB;
use crate::funding_source::FundingSource;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use log::{debug, error, info, trace, warn};
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
    db: Arc<DB>,
    funding: Arc<Box<dyn FundingSource + Send + Sync>>,
    disable_auth: bool,
}

impl Mercado {
    pub async fn new(
        db: Arc<DB>,
        funding: Box<dyn FundingSource + Send + Sync>,
        admins: Vec<String>,
        test: bool,
    ) -> Result<Self> {
        let me = Self {
            db,
            funding: Arc::new(funding),
            disable_auth: test,
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
        if judge_count == 0 {
            bail!("There neeeds to be at least one judge");
        }
        if judges.len() < judge_count as usize {
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
                prediction: prediction.clone(),
                judges: judges.iter().map(|user| user.clone()).collect(),
                judge_count,
                judge_share_ppm,
                trading_end,
                decision_period,
                state: MarketState::WaitingForJudges,
                cash_out: None,
            })
            .await?;
        debug!("Created Prediction {}: {}", id, prediction);
        Ok(id)
    }
    pub async fn accept_nomination(
        &mut self,
        prediction: RowId,
        user: UserPubKey,
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
        debug!(
            "Accepted nomination on prediction {} for user {}",
            prediction, user
        );
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
        prediction: RowId,
        user: UserPubKey,
        access: AccessRequest,
    ) -> Result<()> {
        self.check_access_for_user(user.clone(), access).await?;
        match self.db.get_prediction_state(prediction).await? {
            MarketState::WaitingForJudges => {}
            _ => bail!("Wrong market state"),
        }
        debug!(
            "Refused nomination on prediction {} for user {}",
            prediction, user
        );
        self.db
            .set_judge_state(prediction, user, JudgeState::Refused)
            .await
    }
    pub async fn make_decision(
        &mut self,
        prediction: RowId,
        judge: UserPubKey,
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
                    info!(
                        "Time for decision for prediction {} ran out. Refunding bets",
                        prediction
                    );
                    //Execute refund
                    self.db.remove_bets(Some(prediction), None).await?;
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
        match self.db.get_judge_state(prediction.clone(), judge).await? {
            JudgeState::Nominated | JudgeState::Refused => {
                bail!("Judge did not accept the nomination")
            }
            JudgeState::Resolved(_) | JudgeState::Accepted => {}
        }
        debug!(
            "Voted for {} on prediction {} for judge {}",
            decision, prediction, judge
        );
        match self
            .db
            .set_judge_state(prediction, judge, JudgeState::Resolved(decision))
            .await
        {
            Ok(_) => self.try_resolve(prediction).await,
            e => e,
        }
    }
    async fn try_resolve(&mut self, prediction: RowId) -> Result<()> {
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
                info!("Decision for {} was a tie. Refunding bets", prediction);
                // Refund bets
                self.db.remove_bets(Some(prediction), None).await?;
                bail!("There was a decision tie between an even number of judges")
            }
            Ordering::Greater => {
                self.db
                    .set_prediction_state(prediction, MarketState::Resolved(true))
                    .await?
            }
        }
        let cash_out = self.calculate_cash_out(prediction).await?;
        self.apply_cash_out(cash_out).await?;
        Ok(())
    }
    async fn apply_cash_out(&self, cash_out: HashMap<UserPubKey, (Sats, Sats)>) -> Result<()> {
        for (user, (placed_bets, cash_out)) in cash_out {
            let new_balance = self
                .db
                .adjust_user_balance(user, cash_out - placed_bets)
                .await?;
            if new_balance.is_negative() {
                error!(
                    "User {} has new balance {} after cash_out",
                    user, new_balance
                )
            }
        }
        Ok(())
    }
    async fn calculate_cash_out(
        &self,
        prediction: RowId,
    ) -> Result<HashMap<UserPubKey, (Sats, Sats)>> {
        if let MarketState::Resolved(outcome) = self.db.get_prediction_state(prediction).await? {
            let outcome_bets = self
                .db
                .get_prediction_bets_aggregated(prediction, outcome)
                .await?;
            let non_outcome_bets = self
                .db
                .get_prediction_bets_aggregated(prediction, !outcome)
                .await?;
            let outcome_amount = self
                .get_prediction_bets_aggregated(prediction, outcome)
                .await?;
            let non_outcome_amount = self
                .get_prediction_bets_aggregated(prediction, !outcome)
                .await?;

            // Calculate outcome users
            let mut user_cash_outs = HashMap::new();
            let mut user_cash_out_amount = 0;
            for (user, bet_amount) in outcome_bets {
                let cash_out = calculate_user_cash_out(
                    bet_amount,
                    outcome_amount,
                    non_outcome_amount,
                    self.db.get_judge_share_ppm(prediction).await?,
                );
                if cash_out == 0 {
                    continue;
                }
                user_cash_out_amount += cash_out;
                let non_outcome_bet = non_outcome_bets.get(&user).cloned();
                let placed_bets = bet_amount + non_outcome_bet.unwrap_or_default();
                user_cash_outs.insert(user.clone(), (placed_bets, cash_out));
            }

            //Calculate non-outcome users
            for (user, bet_amount) in non_outcome_bets {
                if let None = user_cash_outs.get(&user) {
                    user_cash_outs.insert(user, (bet_amount, 0));
                }
            }

            // Calculate judges
            let mut judge_cash_out_amount = 0;
            let judge_outcome_count = self.get_outcome_judge_count(prediction).await?;
            for (judge, state) in self.db.get_prediction_judges_mapped(prediction).await? {
                if let JudgeState::Resolved(decision) = state {
                    if decision == outcome {
                        let cash_out = calculate_judge_cash_out(
                            judge_outcome_count,
                            outcome_amount,
                            non_outcome_amount,
                            self.db.get_judge_share_ppm(prediction).await?,
                        );
                        if cash_out == 0 {
                            continue;
                        }
                        judge_cash_out_amount += cash_out;
                        if let Some((placed_bets, user_cash_out)) = user_cash_outs.remove(&judge) {
                            user_cash_outs.insert(judge, (placed_bets, user_cash_out + cash_out));
                        } else {
                            user_cash_outs.insert(judge, (0, cash_out));
                        }
                    }
                }
            }

            // Check solvency after calculation
            if user_cash_out_amount + judge_cash_out_amount > outcome_amount + non_outcome_amount {
                self.db
                    .set_prediction_state(
                        prediction,
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
                //Execute refund
                self.db.remove_bets(Some(prediction), None).await?;
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
    async fn try_activate_trading(&mut self, prediction: RowId) -> Result<()> {
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
        prediction: RowId,
        user: UserPubKey,
        bet: bool,
        amount: Sats,
        access: AccessRequest,
    ) -> Result<()> {
        self.check_access_for_user(user.clone(), access.clone())
            .await?;
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
        if amount <= 0 {
            bail!("Amount has to be positive");
        }
        self.db.create_bet(prediction, user, bet, amount).await?;
        debug!(
            "Added {} sats bet on {} and prediction {} for user {} by {}",
            amount, bet, prediction, user, access.user
        );
        Ok(())
    }
    pub async fn cancel_bet(&mut self, id: RowId, access: AccessRequest) -> Result<()> {
        let bet = self.db.get_bet(id).await?;
        self.check_access_for_user(bet.user, access.clone()).await?;
        let market_state = self.db.get_prediction_state(bet.prediction).await?;
        match market_state {
            MarketState::Trading => {
                if self.db.get_trading_end(bet.prediction).await? < Utc::now() {
                    self.db
                        .set_prediction_state(bet.prediction, MarketState::WaitingForDecision)
                        .await?;
                    bail!("Wrong market state");
                }
            }
            MarketState::Refunded(_) => {
                //TODO what needs to happen here?
            }
            _ => bail!("Wrong market state"),
        }
        self.db.remove_bet(id).await?;
        debug!("Cancelled bet {} by {}", id, access.user);
        Ok(())
    }
    async fn get_outcome_judge_count(&self, prediction: RowId) -> Result<u32> {
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
        prediction: RowId,
        access: AccessRequest,
    ) -> Result<()> {
        if let UserRole::User = self.check_access(access.clone()).await? {
            bail!("Access Denied: Admin only API");
        }
        match self.db.get_prediction_state(prediction).await? {
            MarketState::Trading => {
                warn!(
                    "{} forced the end of the decision period for prediction {}",
                    access.user, prediction
                );
                self.db
                    .set_prediction_state(prediction, MarketState::WaitingForDecision)
                    .await
            }
            _ => bail!("Wrong market state"),
        }
    }
    pub async fn check_access(&self, access: AccessRequest) -> Result<UserRole> {
        if self.disable_auth {
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
        debug!("User {} successfully logged in", user);
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
    /// Action can only be executed from logged in users for themselves
    /// or from logged in admins
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
        let state = self.db.get_judge_state(prediction, user).await?;
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
        let bets = self.db.get_bets(prediction, user, vec![]).await?;
        Ok(bets)
    }
    pub async fn get_balance(&self, user: UserPubKey, access: AccessRequest) -> Result<Sats> {
        self.check_access_for_user(user, access).await?;
        let balance = self.db.get_user_balance(user).await?;
        Ok(balance)
    }
    pub async fn get_available_balance(
        &self,
        user: UserPubKey,
        access: AccessRequest,
    ) -> Result<Sats> {
        self.check_access_for_user(user, access).await?;
        let balance = self.db.get_user_balance(user).await?;
        let user_bets: Sats = self.db.get_user_bets_aggregated(user).await?.values().sum();
        Ok(balance - user_bets)
    }
    pub async fn adjust_balance(
        &self,
        user: UserPubKey,
        amount: Sats,
        access: AccessRequest,
    ) -> Result<Sats> {
        if let UserRole::User = self.check_access(access).await? {
            bail!("Access Denied: Operation only permitted for admins");
        }
        let new_balance = self.db.adjust_user_balance(user, amount).await?;
        warn!("Adjusted balance for {}: {}", user, amount);
        Ok(new_balance)
    }
    pub async fn init_withdrawal_bolt11(
        &self,
        user: UserPubKey,
        invoice: Invoice,
        amount: Sats,
        access: AccessRequest,
    ) -> Result<RowId> {
        self.check_access_for_user(user, access.clone()).await?;
        if amount <= 0 {
            bail!("Amount has to be positive");
        }
        //TODO make balance check atomic with the balance adjustment
        let invoice_amount = self.funding.decode_bolt11(invoice.clone()).await?;
        if invoice_amount != amount {
            bail!("Invoice and form have differing ammounts")
        }
        let balance = self.db.get_user_balance(user).await?;
        if balance - amount < 0 {
            bail!("Not enough funds");
        }
        self.adjust_balance(user, -amount, access).await?;
        let hash = self.funding.pay_bolt11(invoice.clone(), amount).await?;
        let tx = TxType::Bolt11 {
            details: TxDetailsBolt11 {
                payment_hash: hash.clone(),
                payment_request: invoice,
            },
            state: TxStateBolt11::PayInit(amount),
        };
        let id = self.db.create_tx(user, TxDirection::Withdrawal, tx).await?;
        debug!(
            "Initiated Bolt11 Withdrawal: user:{} amount:{}",
            user, amount
        );
        Ok(id)
    }
    pub async fn init_deposit_bolt11(
        &self,
        user: UserPubKey,
        amount: Sats,
        access: AccessRequest,
    ) -> Result<(RowId, Invoice)> {
        self.check_access_for_user(user, access.clone()).await?;
        if amount <= 0 {
            bail!("Amount has to be positive");
        }
        let (hash, invoice) = self.funding.create_bolt11(amount).await?;
        let tx = TxType::Bolt11 {
            details: TxDetailsBolt11 {
                payment_hash: hash.clone(),
                payment_request: invoice.clone(),
            },
            state: TxStateBolt11::PayInit(amount),
        };
        let id = self.db.create_tx(user, TxDirection::Deposit, tx).await?;
        debug!("Initiated Bolt11 Deposit: user:{}, amount:{}", user, amount);
        Ok((id, invoice))
    }
    pub async fn check_tx(&self, id: RowId, access: AccessRequest) -> Result<Tx> {
        let mut tx = self.db.get_tx(id).await?;
        self.check_access_for_user(tx.user, access).await?;
        match tx.direction {
            TxDirection::Withdrawal => self.check_withdrawal(id, tx).await,
            TxDirection::Deposit => self.check_deposit(id, tx).await,
        }
    }
    pub async fn check_withdrawal(&self, id: RowId, tx: Tx) -> Result<Tx> {
        match tx.clone().tx_type {
            TxType::Bolt11 { details, state } => {
                if let TxStateBolt11::Settled(_) = state {
                    return Ok(tx);
                }
                let mut new_state = self
                    .funding
                    .check_bolt11(details.payment_hash.clone())
                    .await?;
                if let TxStateBolt11::PayInit(pending_amount) = new_state {
                    if tx.initiated < Utc::now() - Duration::minutes(10) {
                        new_state = TxStateBolt11::Failed;
                        self.db.adjust_user_balance(tx.user, pending_amount).await?;
                        warn!("Marking withdrawal {} as failed", id);
                    }
                }
                if state == new_state {
                    return Ok(tx);
                }
                self.db
                    .update_tx_state_bolt11(id, new_state.clone())
                    .await?;
                debug!("New state {:?} for withdrawal {}", new_state, id);
                let tx = Tx {
                    user: tx.user,
                    initiated: tx.initiated,
                    direction: tx.direction,
                    tx_type: TxType::Bolt11 {
                        details,
                        state: new_state,
                    },
                };
                Ok(tx)
            }
        }
    }
    pub async fn check_deposit(&self, id: RowId, tx: Tx) -> Result<Tx> {
        match tx.clone().tx_type {
            TxType::Bolt11 { details, state } => {
                if let TxStateBolt11::Settled(_) = state {
                    return Ok(tx);
                }
                let new_state = self
                    .funding
                    .check_bolt11(details.payment_hash.clone())
                    .await?;
                if state == new_state {
                    return Ok(tx);
                }
                self.db
                    .update_tx_state_bolt11(id, new_state.clone())
                    .await?;
                if let TxStateBolt11::Settled(amount) = new_state {
                    self.db.adjust_user_balance(tx.user, amount).await?;
                }
                debug!("New state {:?} for Deposit {}", new_state, id);
                let tx = Tx {
                    user: tx.user,
                    initiated: tx.initiated,
                    direction: tx.direction,
                    tx_type: TxType::Bolt11 {
                        details,
                        state: new_state,
                    },
                };
                Ok(tx)
            }
        }
    }
    pub async fn get_txs(
        &self,
        user: Option<UserPubKey>,
        direction: Option<TxDirection>,
        access: AccessRequest,
    ) -> Result<Vec<RowId>> {
        if let Some(user) = user {
            self.check_access_for_user(user, access).await?;
        } else {
            if let UserRole::User = self.check_access(access).await? {
                bail!("Access Denied: Getting bets of users is prohibited");
            }
        }
        self.db.get_txs(user, direction).await
    }
}

#[allow(unused)]
#[cfg(test)]
mod test {
    use super::*;
    use crate::db::DB;
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

        let db = DB::new("sqlite::memory:".to_string()).await;
        let mut market = Mercado::new(
            Arc::new(db),
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
            .accept_nomination(prediction, j1, access.clone())
            .await
            .unwrap();
        market
            .accept_nomination(prediction, j2, access.clone())
            .await
            .unwrap();
        market
            .accept_nomination(prediction, j3, access.clone())
            .await
            .unwrap();
        let balance = market
            .adjust_balance(u1, 200, access.clone())
            .await
            .unwrap();
        assert_eq!(balance, 200);
        market
            .adjust_balance(u2, 200, access.clone())
            .await
            .unwrap();
        market
            .adjust_balance(u3, 200, access.clone())
            .await
            .unwrap();
        market
            .add_bet(prediction, u1, true, 100, access.clone())
            .await
            .unwrap();
        market
            .add_bet(prediction, u2, true, 100, access.clone())
            .await
            .unwrap();
        market
            .add_bet(prediction, u3, true, 100, access.clone())
            .await
            .unwrap();
        market
            .add_bet(prediction, u1, false, 100, access.clone())
            .await
            .unwrap();
        market
            .add_bet(prediction, u2, false, 100, access.clone())
            .await
            .unwrap();
        market
            .add_bet(prediction, u3, false, 100, access.clone())
            .await
            .unwrap();
        market
            .force_decision_period(prediction, access.clone())
            .await
            .unwrap();
        market
            .make_decision(prediction, j1, true, access.clone())
            .await
            .unwrap();
        market
            .make_decision(prediction, j2, true, access.clone())
            .await
            .unwrap();
        market
            .make_decision(prediction, j3, true, access.clone())
            .await
            .unwrap();
        assert_eq!(market.get_balance(u1, access.clone()).await.unwrap(), 179);
        assert_eq!(market.get_balance(u2, access.clone()).await.unwrap(), 179);
        assert_eq!(market.get_balance(u3, access.clone()).await.unwrap(), 179);
        assert_eq!(market.get_balance(j1, access.clone()).await.unwrap(), 20);
        assert_eq!(market.get_balance(j2, access.clone()).await.unwrap(), 20);
        assert_eq!(market.get_balance(j3, access.clone()).await.unwrap(), 20);
    }
}
