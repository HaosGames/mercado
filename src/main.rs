use crate::api::{NewPredictionResponse, Prediction};
use crate::db::{RowId, SQLite};
use crate::funding_source::TestFundingSource;
use crate::mercado::{Mercado, UserPubKey};
use anyhow::Result;
use axum::extract::Json;
use axum::extract::State;
use axum::routing::{get, put};
use axum::Router;
use axum_macros::debug_handler;
use chrono::{Duration, TimeZone, Utc};
use env_logger::{Builder, Env, WriteStyle};
use log::{info, LevelFilter};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

mod api;
mod db;
mod funding_source;
mod mercado;

async fn new_prediction(state: State<Arc<RwLock<Mercado>>>, Json(prediction): Json<Prediction>) {
    let backend = state.read().await;
    info!("Creating new prediction");
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
}
async fn accept_nomination() {}
async fn refuse_nomination() {}
async fn make_decision() {}
async fn add_bet() {}
async fn check_bet() {}
async fn cancel_bet() {}
async fn cash_out() {}

async fn predictions() {}
async fn get_prediction() {}
async fn get_bet() {}
async fn get_user_bets() {}
async fn get_user_prediction_bets() {}

#[tokio::main]
async fn main() -> Result<()> {
    Builder::default()
        .filter_level(LevelFilter::Debug)
        .write_style(WriteStyle::Always)
        .init();
    let state = Arc::new(RwLock::new(Mercado::new(
        Box::new(SQLite::new().await),
        Box::new(TestFundingSource::default()),
    )));
    let app = Router::new()
        .route("/new_prediction", put(new_prediction))
        .with_state(state);

    axum::Server::bind(&"127.0.0.1:8081".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}
