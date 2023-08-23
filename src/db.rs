use crate::api::{Bet, BetState, Invoice, PredictionListItemResponse, RowId, Sats, UserPubKey};
use crate::mercado::{JudgeState, MarketState, Prediction, RefundReason};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use sqlx::{query, Executor, Pool, Row, SqlitePool};
use std::collections::HashMap;
use std::str::FromStr;

#[async_trait]
pub trait DB {
    async fn add_prediction(&self, prediction: Prediction) -> Result<RowId>;
    async fn get_prediction_state(&self, prediction: &RowId) -> Result<MarketState>;
    async fn set_prediction_state(&self, prediction: &RowId, state: MarketState) -> Result<()>;
    async fn get_judge_state(&self, prediction: RowId, user: &UserPubKey) -> Result<JudgeState>;
    async fn set_judge_state(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
        state: JudgeState,
    ) -> Result<()>;
    async fn get_trading_end(&self, prediction: &RowId) -> Result<DateTime<Utc>>;
    async fn get_decision_period(&self, prediction: &RowId) -> Result<Duration>;
    async fn get_judges(&self, prediction: &RowId) -> Result<HashMap<UserPubKey, JudgeState>>;
    async fn get_judge_states(&self, prediction: &RowId) -> Result<Vec<JudgeState>>;
    async fn set_cash_out(
        &self,
        prediction: &RowId,
        cash_out: HashMap<UserPubKey, Sats>,
    ) -> Result<()>;
    async fn get_judge_share_ppm(&self, prediction: &RowId) -> Result<u32>;
    async fn get_judge_count(&self, prediction: &RowId) -> Result<u32>;
    async fn get_bet(&self, bet: &Invoice) -> Result<Bet>;
    async fn create_bet(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
        bet: bool,
        invoice: String,
    ) -> Result<()>;
    async fn settle_bet(&self, bet: &Invoice, amount: Sats) -> Result<()>;
    async fn init_bet_refund(&self, bet: &Invoice, refund_invoice: Option<&Invoice>) -> Result<()>;
    async fn settle_bet_refund(&self, bet: &Invoice) -> Result<()>;
    async fn get_user_prediction_bets(
        &self,
        user: &UserPubKey,
        prediction: &RowId,
    ) -> Result<Vec<Bet>>;
    async fn set_cash_out_invoice(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
        cash_out_invoice: Invoice,
    ) -> Result<()>;
    async fn get_cash_out(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
    ) -> Result<(Option<Invoice>, Sats)>;
    async fn get_prediction_bets(
        &self,
        prediction: &RowId,
        outcome: bool,
    ) -> Result<HashMap<UserPubKey, Sats>>;
    async fn get_predictions(&self) -> Result<HashMap<RowId, PredictionListItemResponse>>;
}
pub struct SQLite {
    connection: SqlitePool,
}
impl SQLite {
    pub async fn new() -> Self {
        let connection = Pool::connect("sqlite::memory:").await.unwrap();
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS predictions (\
                id PRIMARY KEY,\
            prediction,\
            judge_share_ppm,\
            state,\
            trading_end,\
            decision_period,\
            judge_count,\
            outcome,\
            refund_reason\
            )",
            )
            .await
            .unwrap();
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS bets (\
            user NOT NULL,\
            prediction NOT NULL,\
            bet NOT NULL,\
            amount,\
            state NOT NULL,\
            fund_invoice,\
            refund_invoice,\
            PRIMARY KEY (fund_invoice)\
            )",
            )
            .await
            .unwrap();
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS judges (\
            user,\
            prediction,\
            state NOT NULL,\
            decision,\
            PRIMARY KEY (user,prediction)\
            )",
            )
            .await
            .unwrap();
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS cash_outs (\
            user,\
            prediction,\
            amount NOT NULL,\
            invoice,\
            PRIMARY KEY (user,prediction)\
            )",
            )
            .await
            .unwrap();
        Self { connection }
    }
}
#[async_trait]
impl DB for SQLite {
    async fn add_prediction(&self, prediction: Prediction) -> Result<RowId> {
        let id = self
            .connection
            .execute(
                query(
                    "INSERT INTO predictions (\
            prediction,\
            judge_share_ppm,\
            state,\
            trading_end,\
            decision_period,\
            judge_count)\
            VALUES (?,?,'WaitingForJudges',?,?,?)",
                )
                .bind(prediction.prediction.clone())
                .bind(prediction.judge_share_ppm)
                .bind(prediction.trading_end.timestamp())
                .bind(prediction.decision_period.num_seconds())
                .bind(prediction.judge_count),
            )
            .await?
            .last_insert_rowid();
        for judge in prediction.judges {
            let stmt = query(
                "INSERT INTO judges (\
            user,\
            prediction,\
            state)\
            VALUES (?,?,'Nominated')",
            );
            self.connection
                .execute(stmt.bind(judge.to_string()).bind(id))
                .await?;
        }
        Ok(id)
    }
    async fn get_prediction_state(&self, prediction: &RowId) -> Result<MarketState> {
        let state = MarketState::from_str(
            self.connection
                .fetch_one(query("SELECT state FROM predictions WHERE rowid = ?").bind(prediction))
                .await
                .with_context(|| format!("couldn't get state for prediction {}", prediction))?
                .get("state"),
        )?;
        match state {
            MarketState::Resolved(_) => {
                let outcome = self
                    .connection
                    .fetch_one(
                        query("SELECT outcome FROM predictions WHERE rowid = ?").bind(prediction),
                    )
                    .await
                    .with_context(|| format!("couldn't get outcome for prediction {}", prediction))?
                    .get("outcome");
                Ok(MarketState::Resolved(outcome))
            }
            MarketState::Refunded(_) => {
                let reason = RefundReason::from_str(
                    self.connection
                        .fetch_one(
                            query("SELECT refund_reason FROM predictions WHERE rowid = ?")
                                .bind(prediction),
                        )
                        .await?
                        .get("refund_reason"),
                )?;
                Ok(MarketState::Refunded(reason))
            }
            state => Ok(state),
        }
    }
    async fn set_prediction_state(&self, prediction: &RowId, state: MarketState) -> Result<()> {
        self.connection
            .execute(
                query(
                    "UPDATE predictions \
                SET state = ? \
                WHERE rowid = ?",
                )
                .bind(state.to_string())
                .bind(prediction),
            )
            .await?;
        match state {
            MarketState::Resolved(outcome) => {
                self.connection
                    .execute(
                        query(
                            "UPDATE predictions \
                SET outcome = ? \
                WHERE rowid = ?",
                        )
                        .bind(outcome)
                        .bind(prediction),
                    )
                    .await?;
            }
            MarketState::Refunded(reason) => {
                self.connection
                    .execute(
                        query(
                            "UPDATE predictions \
                SET refund_reason = ? \
                WHERE rowid = ?",
                        )
                        .bind(reason.to_string())
                        .bind(prediction),
                    )
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }
    async fn get_judge_state(&self, prediction: RowId, user: &UserPubKey) -> Result<JudgeState> {
        let state = JudgeState::from_str(
            self.connection
                .fetch_one(
                    query(
                        "SELECT state FROM judges WHERE \
                user = ?, \
                prediction = ?",
                    )
                    .bind(user.to_string())
                    .bind(prediction),
                )
                .await?
                .get(0),
        )?;
        if let JudgeState::Resolved(_) = state {
            let decision = self
                .connection
                .fetch_one(
                    query(
                        "SELECT decision FROM judges WHERE \
                    user = ?, \
                    prediction = ?",
                    )
                    .bind(user.to_string())
                    .bind(prediction),
                )
                .await?
                .get(0);
            return Ok(JudgeState::Resolved(decision));
        }
        Ok(state)
    }
    async fn set_judge_state(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
        state: JudgeState,
    ) -> Result<()> {
        self.connection
            .execute(
                query(
                    "UPDATE judges SET state = ? \
                WHERE user = ? AND prediction = ?",
                )
                .bind(state.to_string())
                .bind(user.to_string())
                .bind(prediction),
            )
            .await?;
        if let JudgeState::Resolved(decision) = state {
            self.connection
                .execute(
                    query(
                        "UPDATE judges SET \
                    decision = ? \
                    WHERE user = ? AND prediction = ?",
                    )
                    .bind(decision)
                    .bind(user.to_string())
                    .bind(prediction),
                )
                .await?;
        }
        Ok(())
    }
    async fn get_trading_end(&self, prediction: &RowId) -> Result<DateTime<Utc>> {
        let trading_end = self
            .connection
            .fetch_one(query("SELECT trading_end FROM predictions WHERE rowid=?").bind(prediction))
            .await?
            .get(0);
        Ok(Utc.timestamp_opt(trading_end, 0).unwrap().into())
    }
    async fn get_decision_period(&self, prediction: &RowId) -> Result<Duration> {
        let decision_period = self
            .connection
            .fetch_one(
                query("SELECT decision_period FROM predictions WHERE rowid=?").bind(prediction),
            )
            .await?
            .get(0);
        Ok(Duration::seconds(decision_period))
    }
    async fn get_judges(&self, prediction: &RowId) -> Result<HashMap<UserPubKey, JudgeState>> {
        let mut judges = HashMap::default();
        let statement = query(
            "SELECT user, state, decision FROM judges \
                WHERE prediction = ?",
        );
        let rows = self
            .connection
            .fetch_all(statement.bind(prediction))
            .await?;
        for row in rows {
            let user = UserPubKey::from_str(row.get("user")).unwrap();
            let state = match JudgeState::from_str(row.get("state"))? {
                JudgeState::Resolved(_) => {
                    let decision = row.get("decision");
                    JudgeState::Resolved(decision)
                }
                state => state,
            };
            judges.insert(user, state);
        }
        Ok(judges)
    }
    async fn get_judge_states(&self, prediction: &RowId) -> Result<Vec<JudgeState>> {
        Ok(self
            .get_judges(prediction)
            .await?
            .values()
            .cloned()
            .collect())
    }
    async fn set_cash_out(
        &self,
        prediction: &RowId,
        cash_out: HashMap<UserPubKey, Sats>,
    ) -> Result<()> {
        for (user, amount) in cash_out {
            let stmt = query(
                "INSERT INTO cash_outs (\
            user,\
            prediction,\
            amount) \
            VALUES (?,?,?)",
            );
            self.connection
                .execute(stmt.bind(user.to_string()).bind(prediction).bind(amount))
                .await?;
        }
        Ok(())
    }
    async fn get_judge_share_ppm(&self, prediction: &RowId) -> Result<u32> {
        let judge_share_ppm = self
            .connection
            .fetch_one(
                query("SELECT judge_share_ppm FROM predictions WHERE rowid=?").bind(prediction),
            )
            .await?
            .get(0);
        Ok(judge_share_ppm)
    }
    async fn get_judge_count(&self, prediction: &RowId) -> Result<u32> {
        let judge_count = self
            .connection
            .fetch_one(query("SELECT judge_count FROM predictions WHERE rowid=?").bind(prediction))
            .await?
            .get(0);
        Ok(judge_count)
    }
    async fn get_bet(&self, invoice: &Invoice) -> Result<Bet> {
        let stmt = query(
            "SELECT user, prediction, bet, amount, state, refund_invoice \
                FROM bets WHERE fund_invoice = ?",
        );
        let row = self.connection.fetch_one(stmt.bind(invoice)).await?;
        let user = UserPubKey::from_str(row.get("user")).unwrap();
        let prediction = row.get("prediction");
        let bet = row.get("bet");
        let state: BetState = BetState::from_str(row.get("state"))?;
        let amount = match state {
            BetState::FundInit => None,
            _ => row.get("amount"),
        };
        let refund_invoice = match state {
            BetState::FundInit => None,
            BetState::Funded => None,
            BetState::RefundInit => row.get("refund_invoice"),
            BetState::Refunded => row.get("refund_invoice"),
        };
        Ok(Bet {
            user,
            prediction,
            bet,
            amount,
            state,
            fund_invoice: invoice.clone(),
            refund_invoice,
        })
    }
    async fn create_bet(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
        bet: bool,
        invoice: String,
    ) -> Result<()> {
        self.connection
            .execute(
                query(
                    "INSERT INTO bets ( \
                user, \
                prediction, \
                bet, \
                fund_invoice, \
                state) \
                VALUES (?,?,?,?,?)",
                )
                .bind(user.to_string())
                .bind(prediction)
                .bind(bet)
                .bind(invoice)
                .bind(BetState::FundInit.to_string()),
            )
            .await?;
        Ok(())
    }
    async fn settle_bet(&self, bet: &Invoice, amount: Sats) -> Result<()> {
        self.connection
            .execute(
                query(
                    "UPDATE bets SET \
                state = ?, \
                amount = ? \
                WHERE fund_invoice=?",
                )
                .bind(BetState::Funded.to_string())
                .bind(amount)
                .bind(bet),
            )
            .await?;
        Ok(())
    }
    async fn init_bet_refund(&self, bet: &Invoice, refund_invoice: Option<&Invoice>) -> Result<()> {
        self.connection
            .execute(
                query(
                    "UPDATE bets SET \
                state = ?, \
                refund_invoice = ? \
                WHERE fund_invoice = ?",
                )
                .bind(BetState::RefundInit.to_string())
                .bind(refund_invoice)
                .bind(bet),
            )
            .await?;
        Ok(())
    }
    async fn settle_bet_refund(&self, bet: &Invoice) -> Result<()> {
        self.connection
            .execute(
                query(
                    "UPDATE bets SET \
                state = ?, \
                WHERE fund_invoice = ?",
                )
                .bind(BetState::Refunded.to_string())
                .bind(bet),
            )
            .await?;
        Ok(())
    }
    async fn get_user_prediction_bets(
        &self,
        user: &UserPubKey,
        prediction: &RowId,
    ) -> Result<Vec<Bet>> {
        let stmt = query(
            "SELECT user, prediction, bet, amount, state, refund_invoice, fund_invoice \
                FROM bets WHERE user = ? AND prediction = ?",
        );
        let mut bets = Vec::new();
        let rows = self
            .connection
            .fetch_all(stmt.bind(user.to_string()).bind(prediction))
            .await?;
        for row in rows {
            let user = UserPubKey::from_str(row.get("user")).unwrap();
            let prediction = row.get("prediction");
            let bet = row.get("bet");
            let fund_invoice = row.get("fund_invoice");
            let state = BetState::from_str(row.get("state"))?;
            let amount = match state {
                BetState::FundInit => None,
                _ => row.get("amount"),
            };
            let refund_invoice = match state {
                BetState::FundInit => None,
                BetState::Funded => None,
                BetState::RefundInit => row.get("refund_invoice"),
                BetState::Refunded => row.get("refund_invoice"),
            };
            bets.push(Bet {
                user,
                prediction,
                bet,
                amount,
                state,
                fund_invoice,
                refund_invoice,
            });
        }
        Ok(bets)
    }

    async fn set_cash_out_invoice(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
        cash_out_invoice: Invoice,
    ) -> Result<()> {
        let stmt = query(
            "UPDATE cash_outs \
                SET invoice = ? \
                WHERE user = ? AND prediction = ?",
        );
        self.connection
            .execute(
                stmt.bind(cash_out_invoice.clone())
                    .bind(user.to_string())
                    .bind(prediction),
            )
            .await
            .with_context(|| {
                format!(
                    "couldn't set cash out invoice {}, for user {} and prediction {}",
                    cash_out_invoice, user, prediction
                )
            })?;
        Ok(())
    }

    async fn get_cash_out(
        &self,
        prediction: &RowId,
        user: &UserPubKey,
    ) -> Result<(Option<Invoice>, Sats)> {
        let row = self
            .connection
            .fetch_one(
                query("SELECT invoice, amount FROM cash_outs WHERE user = ? AND prediction = ?")
                    .bind(user.to_string())
                    .bind(prediction),
            )
            .await
            .with_context(|| {
                format!(
                    "no cash out for user {} and prediction {}",
                    user, prediction
                )
            })?;
        let amount = row.get("amount");
        let invoice = match row.get("invoice") {
            "" => None,
            v => Some(v.to_string()),
        };
        Ok((invoice, amount))
    }

    async fn get_prediction_bets(
        &self,
        prediction: &RowId,
        outcome: bool,
    ) -> Result<HashMap<UserPubKey, Sats>> {
        let stmt = query(
            "SELECT user, prediction, bet, amount, state, refund_invoice, fund_invoice \
                FROM bets WHERE bet = ? AND prediction = ?",
        );
        let mut bets = Vec::new();
        let rows = self
            .connection
            .fetch_all(stmt.bind(outcome).bind(prediction))
            .await?;
        for row in rows {
            let user = UserPubKey::from_str(row.get("user")).unwrap();
            let prediction = row.get("prediction");
            let bet = row.get("bet");
            let fund_invoice = row.get("fund_invoice");
            let state: BetState = FromStr::from_str(row.get("state"))?;
            let amount = match state {
                BetState::FundInit => None,
                _ => row.get("amount"),
            };
            let refund_invoice = match state {
                BetState::FundInit => None,
                BetState::Funded => None,
                BetState::RefundInit => row.get("refund_invoice"),
                BetState::Refunded => row.get("refund_invoice"),
            };
            bets.push(Bet {
                user,
                prediction,
                bet,
                amount,
                state,
                fund_invoice,
                refund_invoice,
            });
        }
        let mut aggregated_bets = HashMap::new();
        for bet in bets {
            if let Some(bet_amount) = bet.amount {
                if let Some(amount) = aggregated_bets.get_mut(&bet.user) {
                    *amount += bet_amount;
                } else {
                    aggregated_bets.insert(bet.user, bet_amount);
                }
            }
        }
        Ok(aggregated_bets)
    }

    async fn get_predictions(&self) -> Result<HashMap<RowId, PredictionListItemResponse>> {
        let stmt = query(
            "SELECT predictions.rowid, predictions.prediction, judge_share_ppm, judge_count, trading_end, \
            decision_period, predictions.state, bet, sum(amount) AS amount \
            FROM predictions \
            LEFT JOIN bets ON predictions.rowid = bets.prediction \
            GROUP BY bet, predictions.prediction",
        );
        let rows = self.connection.fetch_all(stmt).await?;

        let mut predictions: HashMap<RowId, PredictionListItemResponse> = HashMap::new();
        for row in rows {
            let id = row.get("rowid");
            let name = row.get("prediction");
            let judge_share_ppm = row.get("judge_share_ppm");
            let judge_count = row.get("judge_count");
            let decision_period_sec = row.get("decision_period");
            let trading_end = Utc.timestamp_opt(row.get("trading_end"), 0).unwrap();
            let bet = row.get("bet");
            let amount = row.get("amount");

            if let Some(mut prediction) = predictions.get_mut(&id) {
                if bet {
                    prediction.bets_true = amount;
                } else {
                    prediction.bets_false = amount;
                }
            } else {
                predictions.insert(
                    id,
                    PredictionListItemResponse {
                        id,
                        name,
                        judge_share_ppm,
                        judge_count,
                        trading_end,
                        decision_period_sec,
                        bets_true: if bet { amount } else { 0 },
                        bets_false: if bet { 0 } else { amount },
                    },
                );
            }
        }
        Ok(predictions)
    }
}
