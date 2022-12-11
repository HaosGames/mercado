use crate::market::{Bet, Market, MercadoError, Sats, User};
use chrono::{Utc};
use std::collections::BTreeMap;
use surrealdb::sql::{Array, Datetime, Number, Object};
use surrealdb::{
    sql::{parse, Value},
    Datastore, Response, Session,
};

pub struct Mercado {
    db: Datastore,
    db_session: Session,
}
impl Mercado {
    pub async fn new() -> Self {
        let db = Datastore::new("memory").await.unwrap();
        let session = Session::for_kv();
        Self {
            db,
            db_session: session,
        }
    }
    pub async fn add_user(&self, id: &str) -> Result<(), MercadoError> {
        if let Err(surrealdb::Error::RecordExists { .. }) = self
            .process(format!("CREATE user:{} SET sats = 0;", strip_id(&id)))
            .await
            .result
        {
            return Err(MercadoError::UserAlreadyExists);
        }
        Ok(())
    }
    pub async fn deposit_funds(&self, user: &str, amount: Sats) {
        self.process(format!(
            "UPDATE user:{} SET sats += {};",
            strip_id(&user),
            amount
        ))
        .await;
    }
    pub async fn withdraw_funds(&self, user: &str, amount: Sats) -> Result<(), MercadoError> {
        let funds = self.get_funds(user).await?;
        if funds < amount {
            return Err(MercadoError::NotEnoughFunds);
        }

        self.process(format!(
            "UPDATE user:{} SET sats -= {};",
            strip_id(&user),
            amount
        ))
        .await;
        Ok(())
    }
    pub async fn get_funds(&self, user: &str) -> Result<Sats, MercadoError> {
        let response = self
            .process(format!("SELECT sats FROM user:{};", strip_id(&user),))
            .await;
        let row = if let Some(user) = get_rows(response).unwrap().pop() {
            user
        } else {
            return Err(MercadoError::UserDoesntExist);
        };
        let result = if let Value::Number(Number::Int(sats)) = row.get("sats").unwrap() {
            *sats
        } else {
            return Err(MercadoError::QueryFailed);
        };
        Ok(result)
    }
    pub async fn create_market(
        &self,
        id: &str,
        assumption: &str,
        judge_share: f32,
        decision_period: surrealdb::sql::Duration,
        trading_end: Datetime,
        judges: Vec<String>,
    ) -> Result<(), MercadoError> {
        if judge_share > 0.1 || judge_share < 0.00001 {
            return Err(MercadoError::JudgeShareNotInRange)
        }
        if decision_period < std::time::Duration::from_secs(86400).into() {
            return Err(MercadoError::DecisionPeriodToShort)
        }
        if trading_end < (Utc::now() + chrono::Duration::days(1)).into() {
            return Err(MercadoError::TradingEndToEarly)
        }
        if judges.len() < 3 {
            return Err(MercadoError::NotEnoughJudges)
        }
        if judges.len() % 2 == 0 {
            return Err(MercadoError::EvenJudgeAmount)
        }
        for judge in &judges {
            self.get_user(judge.as_str()).await?;
        }
        if let Err(surrealdb::Error::RecordExists { .. }) = self.process(
            format!(
                "CREATE market:{} SET assumption = '{}', judge_share = {}, decision_period = {}, trading_end = {};",
                strip_id(&id),
                assumption, judge_share, decision_period, trading_end
            )).await.result {
            return Err(MercadoError::MarketAlreadyExists)
        }
        for judge in judges {
            self.process(format!("CREATE judge SET user = {}, market = {}, state = 'Nominated'", judge, strip_id(&id))).await;
        }
        Ok(())
    }
    pub async fn get_market(&self, id: &str) -> Result<Market, MercadoError> {
        if let Some(row) = get_rows(self.process(format!("SELECT * FROM {}", id)).await)
            .unwrap()
            .pop()
        {
            Ok(Market {
                assumption: row.get("assumption").unwrap().clone().as_string(),
                trading_end: row.get("trading_end").unwrap().clone().as_datetime(),
                decision_period: row.get("decision_period").unwrap().clone().as_duration(),
                judge_share: row.get("judge_share").unwrap().clone().as_float(),
            })
        } else {
            return Err(MercadoError::MarketDoesntExist);
        }
    }
    pub async fn make_bet(&self, user: &str, market: &str, option: &str, amount: Sats) {
        self.process(format!(
            "CREATE bet SET user = '{}', market = '{}', option = '{}', amount = {};",
            user, market, option, amount
        ))
        .await;
    }
    pub async fn cancel_bet(&self, id: &str) -> Result<(), MercadoError> {
        let bet = self.get_bet(id).await?;
        let market = self
            .get_market(format!("market:{}", bet.market).as_str())
            .await?;
        if market.trading_end < Datetime::default() {
            return Err(MercadoError::TradingStopped);
        }
        self.process(format!("DELETE {};", id)).await;
        Ok(())
    }
    pub async fn get_bet(&self, id: &str) -> Result<Bet, MercadoError> {
        if let Some(row) = get_rows(self.process(format!("SELECT * FROM {}", id)).await)
            .unwrap()
            .pop()
        {
            Ok(Bet {
                id: row.get("id").unwrap().clone().as_string(),
                user: row.get("user").unwrap().clone().as_string(),
                market: row.get("market").unwrap().clone().as_string(),
                option: row.get("option").unwrap().clone().as_string(),
                amount: row.get("amount").unwrap().clone().as_int(),
            })
        } else {
            return Err(MercadoError::BetDoesntExist);
        }
    }
    pub async fn get_user_bets(&self, user: &str) -> Result<Vec<Bet>, MercadoError> {
        let response = self
            .process(format!("SELECT * FROM bet WHERE user = '{}';", user,))
            .await;
        let rows = get_rows(response).unwrap();
        let mut bets: Vec<Bet> = vec![];
        for row in rows {
            let id = row.get("id").unwrap().clone().as_string();
            let market = row.get("market").unwrap().clone().as_string();
            let option = row.get("option").unwrap().clone().as_string();
            let amount = row.get("amount").unwrap().clone().as_int();
            bets.push(Bet {
                id,
                user: user.to_string(),
                market,
                option,
                amount,
            });
        }
        Ok(bets)
    }
    pub async fn get_market_bets(&self, market: &str) -> Result<Vec<Bet>, MercadoError> {
        let response = self
            .process(format!("SELECT * FROM bet WHERE market = '{}';", market,))
            .await;
        let rows = get_rows(response).unwrap();
        let mut bets: Vec<Bet> = vec![];
        for row in rows {
            let id = row.get("id").unwrap().clone().as_string();
            let user = row.get("user").unwrap().clone().as_string();
            let option = row.get("option").unwrap().clone().as_string();
            let amount = row.get("amount").unwrap().clone().as_int();
            bets.push(Bet {
                id,
                market: market.to_string(),
                user,
                option,
                amount,
            });
        }
        Ok(bets)
    }
    pub async fn get_user(&self, user: &str) -> Result<User, MercadoError> {
        let response = self.process(format!("SELECT * FROM user:{};", user,)).await;
        let rows = get_rows(response).unwrap();
        for row in rows {
            let sats = row.get("sats").unwrap().clone().as_int();
            return Ok(User {
                id: user.to_string(),
                sats,
            });
        }
        Err(MercadoError::UserDoesntExist)
    }
    async fn process(&self, query: String) -> Response {
        let query =
            parse(("USE NS mercado DB mercado; ".to_string() + query.as_str()).as_str()).unwrap();
        let mut responses =
            self.db.process(query, &self.db_session, None, false).await.unwrap();
        assert_eq!(responses.len(), 2);
        responses.pop().unwrap()
    }
}

fn strip_id(id: &str) -> &str {
    id.split(|c: char| !c.is_alphanumeric()).next().unwrap()
}
pub type Rows = Vec<Row>;
pub type Row = BTreeMap<String, Value>;
fn get_rows(response: Response) -> Result<Rows, MercadoError> {
    let result = response.result.unwrap();
    if let Value::Array(Array(result)) = result {
        let mut rows: Vec<Row> = vec![];
        for row in result {
            if let Value::Object(Object(row)) = row {
                rows.push(row);
            }
        }
        Ok(rows)
    } else {
        Err(MercadoError::WrongQueryResponseStructure)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn use_db_funds() {
        let market = Mercado::new().await;
        market.add_user("haos").await.unwrap();
        market.deposit_funds("haos", 100).await;
        assert_eq!(Ok(()), market.withdraw_funds("haos", 50).await);
        let funds = market.get_funds("haos").await.unwrap();
        assert_eq!(funds, 50);
    }
    #[tokio::test]
    async fn use_db_bets() {
        let market = Mercado::new().await;
        market.add_user("haos").await.unwrap();
        market.add_user("judge1").await.unwrap();
        market.add_user("judge2").await.unwrap();
        market.add_user("judge3").await.unwrap();
        assert_eq!(Ok(()), market
            .create_market(
                "hobby",
                "Hello",
                0.01,
                std::time::Duration::from_secs(86400).into(),
                (Utc::now() + chrono::Duration::days(2)).into(),
                vec!["judge1".to_string(), "judge2".to_string(), "judge3".to_string()]
            )
            .await);
        market.make_bet("haos", "hobby", "World", 1).await;
        let bets = market.get_user_bets("haos").await.unwrap();
        assert_eq!(bets.len(), 1);
        let mut bets = market.get_market_bets("hobby").await.unwrap();
        assert_eq!(bets.len(), 1);
        let id = bets.pop().unwrap().id;
        assert_eq!(Ok(()), market.cancel_bet(id.as_str()).await);
        let bets = market.get_user_bets("haos").await.unwrap();
        assert_eq!(bets.len(), 0);
    }
    #[tokio::test]
    async fn try_to_steal() {
        let market = Mercado::new().await;
        market.add_user("haos").await.unwrap();
        market.deposit_funds("haos", 100).await;
        let result = market.withdraw_funds("haos", 110).await;
        assert_eq!(Err(MercadoError::NotEnoughFunds), result);
    }
    #[tokio::test]
    async fn withdraw_from_non_user() {
        let market = Mercado::new().await;
        let result = market.withdraw_funds("haos", 110).await;
        assert_eq!(Err(MercadoError::UserDoesntExist), result);
    }
    #[tokio::test]
    async fn cancel_bet_from_stopped_market() {
        let market = Mercado::new().await;
        market.add_user("haos").await.unwrap();
        market.add_user("judge1").await.unwrap();
        market.add_user("judge2").await.unwrap();
        market.add_user("judge3").await.unwrap();
        assert_eq!(Ok(()), market
            .create_market(
                "hobby",
                "Hello",
                0.01,
                std::time::Duration::from_secs(86400).into(),
                (Utc::now() + chrono::Duration::days(2)).into(),
                vec!["judge1".to_string(), "judge2".to_string(), "judge3".to_string()]
            )
            .await);
        market.make_bet("haos", "hobby", "World", 1).await;
        market.process(format!("UPDATE market:hobby SET trading_end = {};", surrealdb::sql::Datetime::from(Utc::now() - chrono::Duration::days(1)))).await;
        let bet = market.get_user_bets("haos").await.unwrap().pop().unwrap();
        let result = market.cancel_bet(bet.id.as_str()).await;
        assert_eq!(Err(MercadoError::TradingStopped), result);
    }
    #[tokio::test]
    async fn create_user_twice() {
        let market = Mercado::new().await;
        assert!(market.add_user("haos").await.is_ok());
        assert_eq!(
            Err(MercadoError::UserAlreadyExists),
            market.add_user("haos").await
        );
    }
}
