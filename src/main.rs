#![allow(unused)]
use crate::api::{NewPredictionRequest, RowId, UserPubKey};
use crate::db::SQLite;
use crate::funding_source::TestFundingSource;
use crate::mercado::Mercado;
use anyhow::{Ok, Result};
use api::AcceptNominationRequest;
use axum::extract::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;
use axum_macros::debug_handler;
use chrono::{Duration, TimeZone, Utc};
use env_logger::{Builder, WriteStyle};
use log::{debug, LevelFilter};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

mod api;
mod db;
mod funding_source;
mod mercado;

#[debug_handler]
async fn new_prediction(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(prediction): Json<NewPredictionRequest>,
) -> (StatusCode, Json<RowId>) {
    let backend = state.read().await;
    debug!("Creating new prediction");
    let id = backend
        .new_prediction(
            prediction.prediction.clone(),
            prediction.judges,
            prediction.judge_count,
            prediction.judge_share_ppm,
            prediction.trading_end,
            Duration::seconds(prediction.decision_period_sec.into()),
        )
        .await
        .unwrap();
    (StatusCode::CREATED, id.into())
}
#[debug_handler]
async fn accept_nomination(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<AcceptNominationRequest>,
) {
    let mut backend = state.write().await;
    debug!(
        "Accepting nomination on prediction {} for user {}",
        request.prediction, request.user
    );
    backend
        .accept_nomination(&request.prediction, &request.user)
        .await
        .unwrap();
}
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
    run_test_server().await;
    Ok(())
}

async fn run_test_server() -> u16 {
    let state = Arc::new(RwLock::new(Mercado::new(
        Box::new(SQLite::new().await),
        Box::new(TestFundingSource::default()),
    )));
    let app = Router::new()
        .route("/new_prediction", post(new_prediction))
        .route("/accept_nomination", post(accept_nomination))
        .with_state(state);

    let server = axum::Server::bind(&"127.0.0.1:0".parse().unwrap()).serve(app.into_make_service());
    let port = server.local_addr().port();
    debug!("Listening on {}", server.local_addr());
    tokio::spawn(async move {
        server.await.unwrap();
    });
    port
}

#[cfg(test)]
mod test {
    use secp256k1::{generate_keypair, rand};

    use super::*;

    #[tokio::test]
    async fn main() {
        Builder::default()
            .filter_level(LevelFilter::Debug)
            .write_style(WriteStyle::Always)
            .init();
        let port = run_test_server().await;
        let client = reqwest::Client::new();

        let (_, u1) = generate_keypair(&mut rand::thread_rng());
        let (_, u2) = generate_keypair(&mut rand::thread_rng());
        let (_, u3) = generate_keypair(&mut rand::thread_rng());
        let (_, j1) = generate_keypair(&mut rand::thread_rng());
        let (_, j2) = generate_keypair(&mut rand::thread_rng());
        let (_, j3) = generate_keypair(&mut rand::thread_rng());

        // Create a new Prediction
        let prediction = NewPredictionRequest {
            prediction: "Test prediction".into(),
            judges: vec![j1, j2, j3],
            judge_share_ppm: 100000,
            trading_end: Utc::now() + Duration::days(3),
            decision_period_sec: Duration::days(1).num_seconds().try_into().unwrap(),
            judge_count: 3,
            bets_true: 0,
            bets_false: 0,
        };
        let response = client
            .post("http://127.0.0.1:".to_string() + port.to_string().as_str() + "/new_prediction")
            .json(&prediction)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let prediction_id = response.json::<RowId>().await.unwrap();

        // Accept Nomination for all 3 judges
        for judge in [j1, j2, j3] {
            let request = AcceptNominationRequest {
                prediction: prediction_id,
                user: judge,
            };
            let response = client
                .post(
                    "http://127.0.0.1:".to_string()
                        + port.to_string().as_str()
                        + "/accept_nomination",
                )
                .json(&request)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }
}
