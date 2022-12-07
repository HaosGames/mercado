use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::sql::Id;

use crate::market::{Funds, Judge, MercadoError, PredictionMarket, Sats};

pub struct API;
impl API {
    fn deposit(user: Id, amount: Sats) {}
    fn withdraw(user: Id, amount: Sats) -> Result<(), MercadoError> {
        todo!()
    }
    fn make_bet(user: Id, market: Id, bet: bool, amount: Sats) -> Result<(), MercadoError> {
        todo!()
    }
    fn cancel_bet(id: Id) -> Result<(), MercadoError> {
        todo!()
    }
    fn make_prediction(
        question: String,
        expiry: DateTime<Utc>,
        judges: Vec<Id>,
        decision_period: Duration,
    ) {
    }
    fn accept_nomination(judge: Id) -> Result<(), MercadoError> {
        todo!()
    }
    fn refuse_nomination(judge: Id) -> Result<(), MercadoError> {
        todo!()
    }
    fn make_decision(judge: Id, decision: bool) -> Result<(), MercadoError> {
        todo!()
    }
    fn get_funds(user: Id) -> Sats {
        todo!()
    }
    fn get_bets(user: Id, market: Id) -> Result<HashMap<Id, Funds>, MercadoError> {
        todo!()
    }
    fn get_prediction(market: Id) -> Result<PredictionMarket, MercadoError> {
        todo!()
    }
    fn query_predictions(query: String) -> HashMap<Id, String> {
        todo!()
    }
    fn get_judges(user: Id) -> Result<HashMap<Id, Judge>, MercadoError> {
        todo!()
    }
    fn get_market_judges(market: Id) -> Result<HashMap<Id, Judge>, MercadoError> {
        todo!()
    }
}
