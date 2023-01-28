use crate::funding_source::{Wallet, WalletAccess};
use crate::mercado::{CashOut, JudgeState, MarketCreationError, MarketError, MarketState, MResult, Prediction, Sats, UserPubKey};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

pub enum DB {
    Test(Arc<Mutex<TestDB>>),
}
#[derive(Debug, Default)]
pub struct TestDB {
    predictions: HashMap<String, Prediction>,
}
impl DB {
    pub async fn add_prediction(
        &self,
        id: String,
        prediction: Prediction,
    ) -> MResult<()> {
        match self {
            DB::Test(db) => {
                let mut db = db.lock().await;
                if !db.predictions.contains_key(&id) {
                    db.predictions.insert(id, prediction);
                    Ok(())
                } else {
                    Err(MarketCreationError::MarketAlreadyExists.into())
                }
            }
        }
    }
    fn test_get_mut_prediction<'a>(
        db: &'a mut MutexGuard<TestDB>,
        prediction: &'_ String,
    ) -> MResult<&'a mut Prediction> {
        if let Some(market) = db.predictions.get_mut(prediction) {
            Ok(market)
        } else {
            Err(MarketError::MarketDoesntExist)
        }
    }
    pub async fn get_prediction_state(
        &self,
        prediction: &String,
    ) -> MResult<MarketState> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.state.clone())
            }
        }
    }
    pub async fn set_prediction_state(
        &self,
        prediction: &String,
        state: MarketState,
    ) -> MResult<()> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                prediction.state = state;
                Ok(())
            }
        }
    }
    pub async fn set_judge_accepted_if_nominated(
        &self,
        prediction: &String,
        judge: &UserPubKey,
    ) -> MResult<()> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
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
    pub async fn set_judge_refused_if_nominated(
        &self,
        prediction: &String,
        judge: &UserPubKey,
    ) -> MResult<()> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
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
    pub async fn set_judge_resolved_if_accepted(
        &self,
        prediction: &String,
        judge: &UserPubKey,
        decision: bool,
    ) -> MResult<()> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
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
    pub async fn get_trading_end(&self, prediction: &String) -> MResult<DateTime<Utc>> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.trading_end.clone())
            }
        }
    }
    pub async fn get_decision_period(&self, prediction: &String) -> MResult<Duration> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.decision_period.clone())
            }
        }
    }
    pub async fn get_judges(
        &self,
        prediction: &String,
    ) -> MResult<HashMap<UserPubKey, JudgeState>> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.judges.clone())
            }
        }
    }
    pub async fn get_judge_states(
        &self,
        prediction: &String,
    ) -> MResult<Vec<JudgeState>> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.judges.values().cloned().collect())
            }
        }
    }
    pub async fn set_cash_out(
        &self,
        prediction: &String,
        cash_out: CashOut,
    ) -> MResult<()> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                prediction.cash_out = Some(cash_out);
                Ok(())
            }
        }
    }
    pub async fn get_judge_share_ppm(&self, prediction: &String) -> MResult<u32> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.judge_share_ppm)
            }
        }
    }
    pub async fn get_judge_count(&self, prediction: &String) -> MResult<u32> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.judge_count)
            }
        }
    }
    /// Add amount to new or existing bet if enough unlocked funds are available
    pub async fn add_bet_amount(
        &self,
        prediction: &String,
        user: &UserPubKey,
        bet: bool,
        amount: Sats,
    ) -> MResult<()> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
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
    pub async fn remove_bets(
        &self,
        prediction: &String,
        user: &UserPubKey,
        bet: bool,
    ) -> MResult<Sats> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
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
    pub async fn get_user_bets_of_prediction(
        &self,
        user: &UserPubKey,
        prediction: &String,
    ) -> MResult<(Option<Sats>, Option<Sats>)> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                let bet_true = prediction.bets_true.get(user);
                let bet_false = prediction.bets_false.get(user);
                Ok((bet_true.cloned(), bet_false.cloned()))
            }
        }
    }
    pub async fn pop_cash_out_user(
        &self,
        prediction: &String,
        user: &UserPubKey,
    ) -> MResult<Sats> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
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
    pub async fn pop_cash_out_judge(
        &self,
        prediction: &String,
        user: &UserPubKey,
    ) -> MResult<Sats> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
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
    pub async fn get_bets(
        &self,
        prediction: &String,
        outcome: bool,
    ) -> MResult<HashMap<UserPubKey, u32>> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                if outcome {
                    Ok(prediction.bets_true.clone())
                } else {
                    Ok(prediction.bets_false.clone())
                }
            }
        }
    }
    pub async fn get_prediction_wallet(
        &self,
        prediction: &String,
    ) -> MResult<WalletAccess> {
        match self {
            Self::Test(db) => {
                let mut db = db.lock().await;
                let prediction = Self::test_get_mut_prediction(&mut db, prediction)?;
                Ok(prediction.wallet.clone())
            }
        }
    }
}
