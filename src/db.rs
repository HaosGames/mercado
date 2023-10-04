use crate::api::*;
use crate::mercado::Prediction;
use anyhow::{bail, Context, Ok, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use secp256k1::ecdsa::Signature;
use serde_json::json;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::types::Json;
use sqlx::{query, Executor, Pool, Row, SqlitePool};
use std::collections::HashMap;
use std::str::FromStr;

pub struct DB {
    connection: SqlitePool,
}
impl DB {
    pub async fn new(db_conn: String) -> Self {
        let options = SqliteConnectOptions::from_str(db_conn.as_str())
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(false);
        let connection = Pool::connect_with(options).await.unwrap();
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS predictions (\
                id PRIMARY KEY,\
                prediction,\
                judge_share_ppm,\
                state,\
                trading_end,\
                decision_period,\
                judge_count\
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
                amount\
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
                "CREATE TABLE IF NOT EXISTS users (\
                pubkey,\
                role DEFAULT User,\
                username UNIQUE,\
                balance DEFAULT 0,\
                PRIMARY KEY (pubkey)\
                )",
            )
            .await
            .unwrap();
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS sessions (\
                pubkey,\
                last_access,\
                challenge,\
                access_token,\
                PRIMARY KEY (challenge)\
                )",
            )
            .await
            .unwrap();
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS payments (\
                user NOT NULL,\
                initiated NOT NULL, \
                direction NOT NULL,\
                type NOT NULL,\
                bolt11_state,\
                bolt11_details\
                )",
            )
            .await
            .unwrap();
        Self { connection }
    }
    pub async fn add_prediction(&self, prediction: Prediction) -> Result<RowId> {
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
    pub async fn get_prediction_state(&self, prediction: RowId) -> Result<MarketState> {
        let state = MarketState::from_str(
            self.connection
                .fetch_one(query("SELECT state FROM predictions WHERE rowid = ?").bind(prediction))
                .await
                .with_context(|| format!("couldn't get state for prediction {}", prediction))?
                .get("state"),
        )?;
        Ok(state)
    }
    pub async fn set_prediction_state(&self, prediction: RowId, state: MarketState) -> Result<()> {
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
        Ok(())
    }
    pub async fn get_judge_state(&self, prediction: RowId, user: UserPubKey) -> Result<JudgeState> {
        let state = JudgeState::from_str(
            self.connection
                .fetch_one(
                    query(
                        "SELECT state FROM judges WHERE \
                user = ? AND \
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
                    user = ? AND \
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
    pub async fn set_judge_state(
        &self,
        prediction: RowId,
        user: UserPubKey,
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
    pub async fn get_trading_end(&self, prediction: RowId) -> Result<DateTime<Utc>> {
        let trading_end = self
            .connection
            .fetch_one(query("SELECT trading_end FROM predictions WHERE rowid=?").bind(prediction))
            .await?
            .get(0);
        Ok(Utc.timestamp_opt(trading_end, 0).unwrap().into())
    }
    pub async fn get_decision_period(&self, prediction: RowId) -> Result<Duration> {
        let decision_period = self
            .connection
            .fetch_one(
                query("SELECT decision_period FROM predictions WHERE rowid=?").bind(prediction),
            )
            .await?
            .get(0);
        Ok(Duration::seconds(decision_period))
    }
    pub async fn get_prediction_judges_mapped(
        &self,
        prediction: RowId,
    ) -> Result<HashMap<UserPubKey, JudgeState>> {
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
    pub async fn get_judge_states(&self, prediction: RowId) -> Result<Vec<JudgeState>> {
        Ok(self
            .get_prediction_judges_mapped(prediction)
            .await?
            .values()
            .cloned()
            .collect())
    }
    pub async fn get_judge_share_ppm(&self, prediction: RowId) -> Result<u32> {
        let judge_share_ppm = self
            .connection
            .fetch_one(
                query("SELECT judge_share_ppm FROM predictions WHERE rowid=?").bind(prediction),
            )
            .await?
            .get(0);
        Ok(judge_share_ppm)
    }
    pub async fn get_judge_count(&self, prediction: RowId) -> Result<u32> {
        let judge_count = self
            .connection
            .fetch_one(query("SELECT judge_count FROM predictions WHERE rowid=?").bind(prediction))
            .await?
            .get(0);
        Ok(judge_count)
    }
    pub async fn get_bet(&self, bet: RowId) -> Result<Bet> {
        let stmt = query(
            "SELECT user, prediction, bet, amount \
                FROM bets WHERE rowid = ?",
        );
        let row = self.connection.fetch_one(stmt.bind(bet)).await?;
        let user = UserPubKey::from_str(row.get("user")).unwrap();
        let prediction = row.get("prediction");
        let id = bet;
        let bet = row.get("bet");
        let amount = row.get("amount");
        Ok(Bet {
            id,
            user,
            prediction,
            bet,
            amount,
        })
    }
    pub async fn create_bet(
        &self,
        prediction: RowId,
        user: UserPubKey,
        bet: bool,
        amount: Sats,
    ) -> Result<()> {
        let mut tx = self.connection.begin().await?;
        let user_bets: Sats = self.get_user_bets_aggregated(user).await?.values().sum();
        let user_balance = self.get_user_balance(user).await?;
        if user_bets + amount > user_balance {
            tx.rollback().await?;
            bail!("Not enough funds",)
        }
        tx.execute(
            query(
                "INSERT INTO bets ( \
                user, \
                prediction, \
                bet,\
                amount) \
                VALUES (?,?,?,?)",
            )
            .bind(user.to_string())
            .bind(prediction)
            .bind(bet)
            .bind(amount),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn remove_bet(&self, bet: RowId) -> Result<()> {
        let stmt = query("DELETE FROM bets WHERE rowid = ?");
        self.connection.execute(stmt.bind(bet)).await?;
        Ok(())
    }
    pub async fn remove_bets(&self, prediction: RowId, user: UserPubKey) -> Result<Sats> {
        let stmt = query("DELETE FROM bets WHERE prediction = ? AND user = ? RETURNING amount");
        let rows = self
            .connection
            .fetch_all(stmt.bind(prediction).bind(user.to_string()))
            .await?;
        let amount = rows
            .iter()
            .map(|row| {
                let amount: Sats = row.get("amount");
                amount
            })
            .sum();
        Ok(amount)
    }
    pub async fn get_prediction_bets_aggregated(
        &self,
        prediction: RowId,
        outcome: bool,
    ) -> Result<HashMap<UserPubKey, Sats>> {
        let mut aggregated_bets = HashMap::new();
        let bets: Vec<Bet> = self
            .get_bets(Some(prediction), None, vec![])
            .await?
            .into_iter()
            .filter(|p| p.bet == outcome)
            .collect();
        for bet in bets {
            if let Some(amount) = aggregated_bets.get_mut(&bet.user) {
                *amount += bet.amount;
            } else {
                aggregated_bets.insert(bet.user, bet.amount);
            }
        }
        Ok(aggregated_bets)
    }
    pub async fn get_user_bets_aggregated(&self, user: UserPubKey) -> Result<HashMap<RowId, Sats>> {
        let mut aggregated_bets = HashMap::new();
        let exluded_market_states = vec![
            MarketState::Resolved(true),
            MarketState::Resolved(false),
            MarketState::Refunded(RefundReason::Insolvency),
            MarketState::Refunded(RefundReason::Tie),
            MarketState::Refunded(RefundReason::TimeForDecisionRanOut),
        ];
        let bets: Vec<Bet> = self
            .get_bets(None, Some(user), exluded_market_states)
            .await?;
        for bet in bets {
            if let Some(amount) = aggregated_bets.get_mut(&bet.prediction) {
                *amount += bet.amount;
            } else {
                aggregated_bets.insert(bet.prediction, bet.amount);
            }
        }
        Ok(aggregated_bets)
    }
    pub async fn get_bets(
        &self,
        prediction: Option<RowId>,
        user: Option<UserPubKey>,
        exclude_states: Vec<MarketState>,
    ) -> Result<Vec<Bet>> {
        let mut stmt = String::from(
            "SELECT bets.user, bets.prediction, bets.bet, bets.amount, predictions.state, bets.rowid \
                FROM bets LEFT JOIN predictions ON predictions.rowid = bets.prediction ",
        );
        match (prediction, user) {
            (None, None) => {}
            (Some(prediction), None) => stmt = stmt + "WHERE bets.prediction = ?",
            (None, Some(user)) => stmt = stmt + "WHERE bets.user = ?",
            (Some(prediction), Some(user)) => {
                stmt = stmt + "WHERE bets.prediction = ? AND bets.user = ?"
            }
        }
        let rows = match (prediction, user) {
            (None, None) => self.connection.fetch_all(query(stmt.as_str())).await?,
            (Some(prediction), None) => {
                self.connection
                    .fetch_all(query(stmt.as_str()).bind(prediction))
                    .await?
            }
            (None, Some(user)) => {
                self.connection
                    .fetch_all(query(stmt.as_str()).bind(user.to_string()))
                    .await?
            }
            (Some(prediction), Some(user)) => {
                self.connection
                    .fetch_all(query(stmt.as_str()).bind(prediction).bind(user.to_string()))
                    .await?
            }
        };
        let mut bets = Vec::new();
        for row in rows {
            let id = row.get("rowid");
            let user = UserPubKey::from_str(row.get("user")).unwrap();
            let prediction = row.get("prediction");
            let bet = row.get("bet");
            let amount = row.get("amount");
            let state = MarketState::from_str(row.get("state")).unwrap();
            if exclude_states.contains(&state) {
                continue;
            }
            bets.push(Bet {
                id,
                user,
                prediction,
                bet,
                amount,
            });
        }
        Ok(bets)
    }
    pub async fn get_predictions(&self) -> Result<HashMap<RowId, PredictionOverviewResponse>> {
        let stmt = query(
            "SELECT predictions.rowid, predictions.prediction, judge_share_ppm, judge_count, trading_end, \
            decision_period, predictions.state, bet, sum(amount) AS amount \
            FROM predictions \
            LEFT JOIN bets ON predictions.rowid = bets.prediction \
            GROUP BY bet, predictions.rowid",
        );
        let rows = self.connection.fetch_all(stmt).await?;

        let mut predictions: HashMap<RowId, PredictionOverviewResponse> = HashMap::new();
        for row in rows {
            let id = row.get("rowid");
            let name = row.get("prediction");
            let judge_share_ppm = row.get("judge_share_ppm");
            let judge_count = row.get("judge_count");
            let decision_period_sec = row.get("decision_period");
            let trading_end = Utc.timestamp_opt(row.get("trading_end"), 0).unwrap();
            let state = MarketState::from_str(row.get("state")).unwrap();

            predictions.insert(
                id,
                PredictionOverviewResponse {
                    id,
                    name,
                    judge_share_ppm,
                    judge_count,
                    trading_end,
                    decision_period_sec,
                    state,
                },
            );
        }
        Ok(predictions)
    }
    pub async fn get_prediction_overview(
        &self,
        prediction: RowId,
    ) -> Result<PredictionOverviewResponse> {
        let stmt = query(
            "SELECT rowid, prediction, judge_share_ppm, judge_count, trading_end, \
            decision_period, state \
            FROM predictions WHERE rowid = ?",
        );
        let row = self.connection.fetch_one(stmt.bind(prediction)).await?;
        let overview = PredictionOverviewResponse {
            id: row.get("rowid"),
            name: row.get("prediction"),
            judge_share_ppm: row.get("judge_share_ppm"),
            judge_count: row.get("judge_count"),
            trading_end: Utc.timestamp_opt(row.get("trading_end"), 0).unwrap(),
            decision_period_sec: row.get("decision_period"),
            state: MarketState::from_str(row.get("state")).unwrap(),
        };
        Ok(overview)
    }
    pub async fn get_prediction_judges(&self, prediction: RowId) -> Result<Vec<Judge>> {
        let stmt = query(
            "SELECT user, prediction, state, decision \
            FROM judges \
            WHERE prediction = ?",
        );
        let rows = self.connection.fetch_all(stmt.bind(prediction)).await?;
        let judges = rows
            .into_iter()
            .map(|row| Judge {
                user: UserPubKey::from_str(row.get("user")).unwrap(),
                prediction: row.get("prediction"),
                state: JudgeState::from_str(row.get("state")).unwrap(),
            })
            .collect();
        Ok(judges)
    }
    pub async fn get_prediction_ratio(&self, prediction: RowId) -> Result<(Sats, Sats)> {
        let stmt_true = query(
            "SELECT SUM(amount) AS amount \
            FROM bets \
            WHERE prediction = ? AND bet = true",
        );
        let stmt_false = query(
            "SELECT SUM(amount) AS amount \
            FROM bets \
            WHERE prediction = ? AND bet = false",
        );
        let row_true = self
            .connection
            .fetch_one(stmt_true.bind(prediction))
            .await?;
        let row_false = self
            .connection
            .fetch_one(stmt_false.bind(prediction))
            .await?;
        Ok((row_true.get("amount"), row_false.get("amount")))
    }
    pub async fn update_user_role(&self, user: UserPubKey, role: UserRole) -> Result<()> {
        self.create_user(user).await?;
        let stmt = query(
            "UPDATE users SET \
            role = ?\
            WHERE pubkey = ?",
        );
        self.connection
            .execute(stmt.bind(role.to_string()).bind(user.to_string()))
            .await?;
        Ok(())
    }
    pub async fn get_user_role(&self, user: UserPubKey) -> Result<UserRole> {
        let stmt = query("SELECT role FROM users WHERE pubkey = ?");
        let row = self
            .connection
            .fetch_one(stmt.bind(user.to_string()))
            .await?;
        let role: String = row.get("role");
        Ok(UserRole::from_str(role.as_str())?)
    }
    pub async fn get_user_balance(&self, user: UserPubKey) -> Result<Sats> {
        let stmt = query("SELECT balance FROM users WHERE pubkey = ?");
        let row = self
            .connection
            .fetch_one(stmt.bind(user.to_string()))
            .await?;
        let balance: Sats = row.get("balance");
        Ok(balance)
    }
    pub async fn adjust_user_balance(&self, user: UserPubKey, amount: Sats) -> Result<Sats> {
        self.create_user(user).await?;
        let stmt =
            query("UPDATE users SET balance = balance + ? WHERE pubkey = ? RETURNING balance");
        let row = self
            .connection
            .fetch_one(stmt.bind(amount).bind(user.to_string()))
            .await?;
        let balance = row.get("balance");
        Ok(balance)
    }
    pub async fn create_session(&self, user: UserPubKey, challenge: String) -> Result<()> {
        self.create_user(user).await?;
        let stmt = query(
            "INSERT INTO sessions \
            (pubkey, challenge) VALUES \
            (?, ?)",
        );
        self.connection
            .execute(stmt.bind(user.to_string()).bind(challenge))
            .await?;
        Ok(())
    }
    pub async fn update_access_token(
        &self,
        user: UserPubKey,
        sig: Signature,
        challenge: String,
    ) -> Result<()> {
        let stmt = query(
            "UPDATE sessions SET \
            access_token = ?, \
            last_access = ? \
            WHERE pubkey = ? AND challenge = ?",
        );
        self.connection
            .execute(
                stmt.bind(sig.to_string())
                    .bind(Utc::now().timestamp())
                    .bind(user.to_string())
                    .bind(challenge),
            )
            .await?;
        Ok(())
    }
    pub async fn update_access(&self, user: UserPubKey, challenge: String) -> Result<()> {
        let stmt = query(
            "UPDATE sessions SET \
            last_access = ? \
            WHERE pubkey = ? AND challenge = ?",
        );
        self.connection
            .execute(
                stmt.bind(Utc::now().timestamp())
                    .bind(user.to_string())
                    .bind(challenge),
            )
            .await?;
        Ok(())
    }
    pub async fn get_last_access(
        &self,
        user: UserPubKey,
        challenge: String,
    ) -> Result<(Signature, DateTime<Utc>)> {
        let stmt = query(
            "SELECT access_token, last_access \
            FROM sessions \
            WHERE pubkey = ? AND challenge = ?",
        );
        let row = self
            .connection
            .fetch_one(stmt.bind(user.to_string()).bind(challenge))
            .await?;
        let token: String = row.get("access_token");
        let last_access = row.get("last_access");
        Ok((
            Signature::from_str(token.as_str())?,
            Utc.timestamp_opt(last_access, 0).unwrap(),
        ))
    }
    pub async fn update_username(&self, user: UserPubKey, name: String) -> Result<()> {
        let stmt = query(
            "UPDATE users SET \
            username = ? \
            WHERE pubkey = ?",
        );
        self.connection
            .execute(stmt.bind(name).bind(user.to_string()))
            .await?;
        Ok(())
    }
    pub async fn create_user(&self, user: UserPubKey) -> Result<()> {
        let stmt = query(
            "INSERT OR IGNORE INTO users \
            (pubkey) VALUES (?)",
        );
        self.connection.execute(stmt.bind(user.to_string())).await?;
        Ok(())
    }
    pub async fn get_username(&self, user: UserPubKey) -> Result<Option<String>> {
        let stmt = query("SELECT username FROM users WHERE pubkey = ?");
        let row = self
            .connection
            .fetch_optional(stmt.bind(user.to_string()))
            .await?;
        if let Some(row) = row {
            Ok(row.get("username"))
        } else {
            Ok(None)
        }
    }
    pub async fn get_user(&self, user: UserPubKey) -> Result<UserResponse> {
        let stmt = query("SELECT username, role FROM users WHERE pubkey = ?");
        let row = self
            .connection
            .fetch_one(stmt.bind(user.to_string()))
            .await?;
        Ok(UserResponse {
            user,
            username: row.try_get("username").ok(),
            role: UserRole::from_str(row.get("role"))?,
        })
    }
    pub async fn get_judges(
        &self,
        prediction: Option<RowId>,
        user: Option<UserPubKey>,
    ) -> Result<Vec<JudgePublic>> {
        let mut stmt = String::from("SELECT user, prediction FROM judges ");
        match (prediction, user) {
            (None, None) => {}
            (Some(prediction), None) => stmt = stmt + "WHERE prediction = ?",
            (None, Some(user)) => stmt = stmt + "WHERE user = ?",
            (Some(prediction), Some(user)) => stmt = stmt + "WHERE prediction = ? AND user = ?",
        }
        let rows = match (prediction, user) {
            (None, None) => self.connection.fetch_all(query(stmt.as_str())).await?,
            (Some(prediction), None) => {
                self.connection
                    .fetch_all(query(stmt.as_str()).bind(prediction))
                    .await?
            }
            (None, Some(user)) => {
                self.connection
                    .fetch_all(query(stmt.as_str()).bind(user.to_string()))
                    .await?
            }
            (Some(prediction), Some(user)) => {
                self.connection
                    .fetch_all(query(stmt.as_str()).bind(prediction).bind(user.to_string()))
                    .await?
            }
        };
        let judges = rows
            .into_iter()
            .map(|row| JudgePublic {
                user: UserPubKey::from_str(row.get("user")).unwrap(),
                prediction: row.get("prediction"),
            })
            .collect();
        Ok(judges)
    }
    pub async fn create_tx(
        &self,
        user: UserPubKey,
        direction: TxDirection,
        tx: TxType,
    ) -> Result<RowId> {
        match tx {
            TxType::Bolt11 { details, state } => {
                let stmt = query(
                    "INSERT INTO payments (\
                    user, \
                    initiated, \
                    direction, \
                    type, \
                    bolt11_state, \
                    bolt11_details\
                    ) VALUES (?,?,?,?,?,?) RETURNING rowid",
                )
                .bind(json!(user))
                .bind(Utc::now().timestamp())
                .bind(json!(direction))
                .bind(json!(TxTypes::Bolt11))
                .bind(json!(state))
                .bind(json!(details));
                let row = self.connection.fetch_one(stmt).await?;
                let id = row.get("rowid");
                Ok(id)
            }
        }
    }
    pub async fn update_tx_state_bolt11(&self, id: RowId, state: TxStateBolt11) -> Result<()> {
        let stmt = query("UPDATE payments SET bolt11_state = ? WHERE rowid = ?")
            .bind(json!(state))
            .bind(id);
        self.connection.execute(stmt).await?;
        Ok(())
    }
    pub async fn get_tx(&self, id: RowId) -> Result<Tx> {
        let stmt = query(
            "SELECT user, initiated, direction, type, bolt11_state, bolt11_details \
            FROM payments WHERE rowid = ?",
        )
        .bind(id);
        let row = self.connection.fetch_optional(stmt).await?;
        let row = row.ok_or(anyhow::anyhow!("Transaction {} doesn't exist", id))?;
        let user: Json<UserPubKey> = row.get("user");
        let initiated = DateTime::from_timestamp(row.get("initiated"), 0).unwrap();
        let direction: Json<TxDirection> = row.get("direction");
        let tx_type: Json<TxTypes> = row.get("type");
        match tx_type.0 {
            TxTypes::Bolt11 => {
                let state: Json<_> = row.get("bolt11_state");
                let details: Json<_> = row.get("bolt11_details");
                Ok(Tx {
                    user: user.0,
                    initiated,
                    direction: direction.0,
                    tx_type: TxType::Bolt11 {
                        details: details.0,
                        state: state.0,
                    },
                })
            }
        }
    }
    pub async fn get_txs(
        &self,
        user: Option<UserPubKey>,
        direction: Option<TxDirection>,
    ) -> Result<Vec<RowId>> {
        let mut stmt = String::from(
            "SELECT rowid \
                FROM payments ",
        );
        match (user, direction.clone()) {
            (None, None) => {}
            (Some(user), None) => stmt = stmt + "WHERE user = ?",
            (None, Some(direction)) => stmt = stmt + "WHERE direction = ?",
            (Some(user), Some(direction)) => stmt = stmt + "WHERE user = ? AND direction = ?",
        }
        let rows = match (user, direction) {
            (None, None) => self.connection.fetch_all(query(stmt.as_str())).await?,
            (Some(user), None) => {
                self.connection
                    .fetch_all(query(stmt.as_str()).bind(json!(user)))
                    .await?
            }
            (None, Some(direction)) => {
                self.connection
                    .fetch_all(query(stmt.as_str()).bind(json!(direction)))
                    .await?
            }
            (Some(user), Some(direction)) => {
                self.connection
                    .fetch_all(
                        query(stmt.as_str())
                            .bind(json!(user))
                            .bind(json!(direction)),
                    )
                    .await?
            }
        };
        let txs = rows.into_iter().map(|row| row.get("rowid")).collect();
        Ok(txs)
    }
}
