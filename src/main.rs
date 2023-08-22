#![allow(unused)]
use crate::api::*;
use crate::db::SQLite;
use crate::funding_source::TestFundingSource;
use crate::mercado::Mercado;
use anyhow::Result;
use axum::extract::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use axum_macros::debug_handler;
use chrono::{Duration, TimeZone, Utc};
use env_logger::{Builder, WriteStyle};
use log::{debug, LevelFilter};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

mod api;
mod client;
mod db;
mod funding_source;
mod mercado;

#[debug_handler]
async fn new_prediction(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(prediction): Json<NewPredictionRequest>,
) -> Result<(StatusCode, Json<RowId>), (StatusCode, String)> {
    let backend = state.read().await;
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
        .map_err(|e| {
            debug!("Error when creating prediction: {:#}", e);
            (StatusCode::BAD_REQUEST, format!("{:?}", e))
        })?;
    debug!("Created Prediction {}: {}", id, prediction.prediction);
    Ok((StatusCode::CREATED, id.into()))
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
async fn refuse_nomination(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<AcceptNominationRequest>,
) {
    let mut backend = state.write().await;
    debug!(
        "Refusing nomination on prediction {} for user {}",
        request.prediction, request.user
    );
    backend
        .refuse_nomination(&request.prediction, &request.user)
        .await
        .unwrap();
}
async fn make_decision(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<MakeDecisionRequest>,
) {
    let mut backend = state.write().await;
    debug!(
        "Voting for {} on prediction {} for judge {}",
        request.decision, request.prediction, request.judge
    );
    backend
        .make_decision(&request.prediction, &request.judge, request.decision)
        .await
        .unwrap();
}
async fn add_bet(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<AddBetRequest>,
) -> (StatusCode, Invoice) {
    let mut backend = state.write().await;
    debug!(
        "Adding bet on {} and prediction {} for user {}",
        request.bet, request.prediction, request.user
    );
    let invoice = backend
        .add_bet(&request.prediction, &request.user, request.bet)
        .await
        .unwrap();
    (StatusCode::CREATED, invoice)
}
#[cfg(test)]
async fn pay_bet(State(state): State<Arc<RwLock<Mercado>>>, Json(request): Json<PayBetRequest>) {
    use api::PayBetRequest;

    let mut backend = state.write().await;
    debug!("Paying bet invoice with {} sats", request.amount);
    let invoice = backend
        .pay_bet(&request.invoice, request.amount)
        .await
        .unwrap();
}
async fn check_bet() {}
async fn cancel_bet() {}
async fn cash_out_user(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<CashOutUserRequest>,
) -> Json<Sats> {
    let mut backend = state.write().await;
    let sats = backend
        .cash_out_user(&request.prediction, &request.user, &request.invoice)
        .await
        .unwrap();
    debug!(
        "Cashed out {} sats for user {} on prediction {}",
        sats, request.user, request.prediction
    );
    Json(sats)
}

async fn get_predictions(
    State(state): State<Arc<RwLock<Mercado>>>,
) -> Json<Vec<PredictionListItemResponse>> {
    let mut backend = state.write().await;
    let predictions = backend.get_predictions().await.unwrap();
    Json(predictions.into_values().collect())
}
async fn get_prediction() {}
async fn get_bet() {}
async fn get_user_bets() {}
async fn get_user_prediction_bets() {}

#[cfg(test)]
async fn force_decision_period(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<RowId>,
) {
    let mut backend = state.write().await;
    debug!(
        "Forcing the end of the decision period for prediction {}",
        request
    );
    backend.force_decision_period(&request).await.unwrap();
}

#[tokio::main]
async fn main() -> Result<()> {
    Builder::default()
        .filter_level(LevelFilter::Debug)
        .write_style(WriteStyle::Always)
        .init();
    let (_port, handle) = run_test_server(Some(8081)).await;
    handle.await;
    Ok(())
}

async fn run_test_server(port: Option<u16>) -> (u16, JoinHandle<()>) {
    let state = Arc::new(RwLock::new(Mercado::new(
        Box::new(SQLite::new().await),
        Box::new(TestFundingSource::default()),
    )));
    let app = Router::new()
        .route("/new_prediction", post(new_prediction))
        .route("/accept_nomination", post(accept_nomination))
        .route("/refuse_nomination", post(refuse_nomination))
        .route("/add_bet", post(add_bet))
        .route("/make_decision", post(make_decision))
        .route("/cash_out_user", post(cash_out_user))
        .route("/get_predictions", get(get_predictions));
    #[cfg(test)]
    let app = app.route("/pay_bet", post(pay_bet));
    #[cfg(test)]
    let app = app.route("/force_decision_period", post(force_decision_period));
    let app = app.with_state(state);

    let addr = "127.0.0.1:".to_string() + port.unwrap_or(0).to_string().as_str();
    let server = axum::Server::bind(&addr.parse().unwrap()).serve(app.into_make_service());
    let port = server.local_addr().port();
    debug!("Listening on {}", server.local_addr());
    let handle = tokio::spawn(async move {
        server.await.unwrap();
    });
    (port, handle)
}

#[cfg(test)]
mod test {
    use secp256k1::{generate_keypair, rand};

    use crate::client::Client;

    use super::*;

    #[tokio::test]
    async fn main() {
        // Builder::default()
        //     .filter_level(LevelFilter::Debug)
        //     .write_style(WriteStyle::Always)
        //     .init();
        let (port, _) = run_test_server(None).await;
        let client = Client::new("http://127.0.0.1:".to_string() + port.to_string().as_str());

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
            judge_count: 2,
        };
        let response = client.new_prediction(prediction).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let prediction_id = response.json::<RowId>().await.unwrap();

        // Refuse Nomination for 1 judge
        let request = AcceptNominationRequest {
            prediction: prediction_id,
            user: j3,
        };
        let response = client.refuse_nomination(request).await;
        assert_eq!(response.status(), StatusCode::OK);

        // Accept Nomination for 2 judges
        for judge in [j1, j2] {
            let request = AcceptNominationRequest {
                prediction: prediction_id,
                user: judge,
            };
            let response = client.accept_nomination(request).await;
            assert_eq!(response.status(), StatusCode::OK);
        }

        // Add bet for 3 users
        for user in [u1, u2, u3] {
            let request = AddBetRequest {
                prediction: prediction_id,
                user,
                bet: true,
            };
            let response = client.add_bet(request).await;
            assert_eq!(response.status(), StatusCode::CREATED);
            let invoice = response.text().await.unwrap();
            let request = PayBetRequest {
                invoice,
                amount: 100,
            };
            let response = client.pay_bet(request).await;
            assert_eq!(response.status(), StatusCode::OK)
        }

        // Forcing the end of the decision period
        let response = client.force_decision_period(prediction_id).await;
        assert_eq!(response.status(), StatusCode::OK);

        // Voting for outcomes for 2 judges
        for judge in [j1, j2] {
            let request = MakeDecisionRequest {
                prediction: prediction_id,
                judge,
                decision: true,
            };
            let response = client.make_decision(request).await;
            assert_eq!(response.status(), StatusCode::OK);
        }

        // Cash out users
        for user in [u1, u2, u3] {
            let request = CashOutUserRequest {
                prediction: prediction_id,
                user,
                invoice: user.to_string(),
            };
            let response = client.cash_out_user(request).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.json::<Sats>().await.unwrap(), 89);
        }

        // Cash out judges
        for judge in [j1, j2] {
            let request = CashOutUserRequest {
                prediction: prediction_id,
                user: judge,
                invoice: judge.to_string(),
            };
            let response = client.cash_out_user(request).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.json::<Sats>().await.unwrap(), 15);
        }

        let predictions = client.get_predictions().await.unwrap();
        let prediction = predictions.first().unwrap();
        assert_eq!(prediction.bets_true, 300);
        assert_eq!(prediction.name, "Test prediction".to_string());
    }
}
