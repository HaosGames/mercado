use crate::market::{Bet, MercadoError, Sats, User};
use std::collections::BTreeMap;
use surrealdb::sql::{Array, Datetime, Duration, Number, Object, Strand};
use surrealdb::{
    sql::{parse, Value},
    Datastore, Response, Session,
};

pub struct DB {
    db: Datastore,
    session: Session,
}
impl DB {
    pub async fn new() -> Self {
        let db = Datastore::new("memory").await.unwrap();
        let session = Session::for_kv();
        Self { db, session }
    }
    async fn add_user(&self, id: &str) {
        self.process(
            format!(
                "CREATE user:{} SET sats = 0;",
                strip_id(&id)
            )).await;
    }
    async fn deposit_funds(&self, user: &str, amount: Sats) {
        self.process(
            format!(
                "UPDATE user:{} SET sats += {};",
                strip_id(&user),
                amount
            )).await;
    }
    async fn withdraw_funds(&self, user: &str, amount: Sats) -> Result<(), MercadoError> {
        if let Ok(funds) = self.get_funds(user).await {
            if funds < amount {
                return Err(MercadoError::NotEnoughFunds);
            }
        } else {
            return Err(MercadoError::UserDoesntExist);
        }
        self.process(format!(
            "UPDATE user:{} SET sats -= {};",
            strip_id(&user),
            amount
        )).await;
        Ok(())
    }
    async fn get_funds(&self, user: &str) -> Result<Sats, MercadoError> {
        let response = self.process(format!(
            "SELECT sats FROM user:{};",
            strip_id(&user),
        )).await;
        let row = get_row(&response, 0).unwrap();
        let result = if let Value::Number(Number::Int(sats)) = row.get("sats").unwrap() {
            *sats
        } else {
            return Err(MercadoError::QueryFailed);
        };
        Ok(result)
    }
    async fn create_market(
        &self,
        id: &str,
        assumption: &str,
        judge_share: f32,
        decision_period: Duration,
        trading_end: Datetime,
    ) {
        self.process(
            format!(
                "CREATE market:{} SET assumption = '{}', judge_share = {}, decision_period = '{}', trading_end = '{}';",
                strip_id(&id),
                assumption, judge_share, decision_period, trading_end
            )).await;
    }
    async fn make_bet(&self, user: &str, market: &str, option: &str, amount: Sats) {
        self.process(
            format!(
                "CREATE bet SET user = '{}', market = '{}', option = '{}', amount = {};",
                user, market, option, amount
            )).await;
    }
    async fn get_user_bets(&self, user: &str) -> Result<Vec<Bet>, MercadoError> {
        let response = self.process(format!(
            "SELECT * FROM bet WHERE user = '{}';",
            user,
        )).await;
        let rows = unpack_response(&response).unwrap();
        let mut bets: Vec<Bet> = vec![];
        for row in rows {
            if let Value::Object(Object(row)) = row {
                let market = row.get("market").unwrap().clone().as_string();
                let option = row.get("option").unwrap().clone().as_string();
                let amount = row.get("amount").unwrap().clone().as_int();
                bets.push(Bet{user: user.to_string(), market, option, amount});
            }
        }
        Ok(bets)
    }
    async fn get_market_bets(&self, market: &str) -> Result<Vec<Bet>, MercadoError> {
        let response = self
            .process(format!(
                "SELECT * FROM bet WHERE market = '{}';",
                market,
            )).await;
        let rows = unpack_response(&response).unwrap();
        let mut bets: Vec<Bet> = vec![];
        for row in rows {
            if let Value::Object(Object(row)) = row {
                let user = row.get("user").unwrap().clone().as_string();
                let option = row.get("option").unwrap().clone().as_string();
                let amount = row.get("amount").unwrap().clone().as_int();
                bets.push(Bet{market: market.to_string(), user, option, amount});
            }
        }
        Ok(bets)
    }
    async fn get_user(&self, user: &str) -> Result<User, MercadoError> {
        let response = self
            .process(format!(
                "SELECT * FROM user:{};",
                user,
            )).await;
        let rows = unpack_response(&response).unwrap();
        for row in rows {
            if let Value::Object(Object(row)) = row {
                let sats = row.get("sats").unwrap().clone().as_int();
                return Ok(User {
                    id: user.to_string(),
                    sats,
                })
            }
        }
        Err(MercadoError::UserDoesntExist)
    }
    async fn process(&self, query: String) -> Response {
        let query = parse(("USE NS mercado DB mercado; ".to_string() + query.as_str()).as_str()).unwrap();
        let mut responses = self
            .db
            .process(query, &self.session, None, false)
            .await
            .unwrap();
        assert_eq!(responses.len(), 2);
        responses.pop().unwrap()
    }
}

fn strip_id(id: &str) -> &str {
    id.split(|c: char| !c.is_alphanumeric()).next().unwrap()
}
pub type Rows = Vec<Value>;
pub type Row = BTreeMap<String, Value>;
fn unpack_response(response: &Response) -> Result<&Rows, MercadoError> {
    let result = response.result.as_ref().unwrap();
    return if let Value::Array(Array(rows)) = result {
        Ok(rows)
    } else {
        Err(MercadoError::WrongQueryResponseStructure)
    };
}
fn get_row(response: &Response, row: usize) -> Result<&Row, MercadoError> {
    let rows = unpack_response(response).unwrap();
    if let Value::Object(Object(row)) = rows.get(row).unwrap() {
        Ok(row)
    } else {
        Err(MercadoError::WrongQueryResponseStructure)
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn use_db_funds() {
        let db = DB::new().await;
        db.add_user("haos").await;
        db.deposit_funds("haos", 100).await;
        db.withdraw_funds("haos", 50).await;
        let funds = db.get_funds("haos").await.unwrap();
        assert_eq!(funds, 50);
    }
    #[tokio::test]
    async fn use_db_bets() {
        let db = DB::new().await;
        db.add_user("haos").await;
        db.create_market("hobby", "Hello", 0.01, Duration::default(), Datetime::default()).await;
        db.make_bet("haos", "hobby", "World", 1).await;
        let bets = db.get_user_bets("haos").await.unwrap();
        assert_eq!(vec![Bet {
            user: "haos".to_string(),
            market: "hobby".to_string(),
            option: "World".to_string(),
            amount: 1,
        }], bets);
        let bets = db.get_market_bets("hobby").await.unwrap();
        assert_eq!(vec![Bet {
            user: "haos".to_string(),
            market: "hobby".to_string(),
            option: "World".to_string(),
            amount: 1,
        }], bets);
    }
    #[tokio::test]
    async fn try_to_steal() {
        let db = DB::new().await;
        db.add_user("haos").await;
        db.deposit_funds("haos", 100).await;
        let result = db.withdraw_funds("haos", 110).await;
        assert_eq!(Err(MercadoError::NotEnoughFunds), result);
    }
    #[ignore]
    #[tokio::test]
    async fn withdraw_from_non_user() {
        let db = DB::new().await;
        let result = db.withdraw_funds("haos", 110).await;
        assert_eq!(Err(MercadoError::UserDoesntExist), result);
    }
}
