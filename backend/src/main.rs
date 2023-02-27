use crate::api::{NewPredictionResponse, Prediction};
use crate::db::{RowId, SQLite};
use crate::funding_source::TestFundingSource;
use crate::mercado::{Mercado, UserPubKey};
use actix_web::{get, post, web, App, HttpServer, Responder};
use anyhow::Result;
use chrono::{Duration, TimeZone, Utc};
use std::str::FromStr;
use std::sync::Arc;
use env_logger::{Builder, Env};
use log::LevelFilter;
use tokio::sync::RwLock;

mod api;
mod db;
mod funding_source;
mod mercado;

#[get("/hello/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    format!("Hello {}!", name)
}

#[post("/new-prediction")]
async fn new_prediction(
    prediction: web::Json<Prediction>,
    backend: web::Data<Backend>,
) -> web::Json<NewPredictionResponse> {
    let backend = backend.mercado.read().await;
    let id = backend
        .new_prediction(
            prediction.prediction.clone(),
            prediction
                .judges
                .iter()
                .map(|judge| UserPubKey::from_str(judge).unwrap())
                .collect(),
            prediction.judge_count,
            prediction.judge_share_ppm,
            Utc.timestamp_opt(prediction.trading_end, 0).unwrap().into(),
            Duration::seconds(prediction.decision_period_sec.into()),
        )
        .await
        .unwrap();
    web::Json(NewPredictionResponse { id })
}
async fn accept_nomination() {}
async fn refuse_nomination() {}
async fn make_decision() {}
async fn add_bet() {}
async fn check_bet() {}
async fn cancel_bet() {}
async fn cash_out() {}

async fn get_prediction() {}
async fn get_bet() {}
async fn get_user_bets() {}
async fn get_user_prediction_bets() {}

struct Backend {
    mercado: RwLock<Mercado>,
}

#[tokio::main]
async fn main() -> Result<()> {
    Builder::default().filter_level(LevelFilter::Debug).init();
    let market = web::Data::new(Backend {
        mercado: RwLock::new(Mercado::new(
            Box::new(SQLite::new().await),
            Box::new(TestFundingSource::default()),
        )),
    });
    HttpServer::new(move || {
        App::new()
            .service(greet)
            .service(new_prediction)
            .app_data(market.clone())
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await?;
    Ok(())
}
