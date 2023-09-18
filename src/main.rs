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
use clap::Parser;
use env_logger::{Builder, WriteStyle};
use log::trace;
use log::{debug, LevelFilter};
use std::collections::HashMap;
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
    Json(request): Json<NewPredictionRequest>,
) -> Result<(StatusCode, Json<RowId>), (StatusCode, String)> {
    let mut backend = state.write().await;
    let prediction = request;
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
        .map_err(map_any_err_and_code)?;
    debug!("Created Prediction {}: {}", id, prediction.prediction);
    Ok((StatusCode::CREATED, id.into()))
}
#[debug_handler]
async fn accept_nomination(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<NominationRequest>>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    let prediction = request.data;
    backend
        .accept_nomination(&prediction.prediction, &prediction.user, request.access)
        .await
        .map_err(map_any_err_and_code)?;
    debug!(
        "Accepted nomination on prediction {} for user {}",
        prediction.prediction, prediction.user
    );
    Ok(())
}
async fn refuse_nomination(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<NominationRequest>>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    let (request, access) = (request.data, request.access);
    backend
        .refuse_nomination(&request.prediction, &request.user, access)
        .await
        .map_err(map_any_err_and_code)?;
    debug!(
        "Refused nomination on prediction {} for user {}",
        request.prediction, request.user
    );
    Ok(())
}
async fn make_decision(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<MakeDecisionRequest>>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    let (request, access) = (request.data, request.access);
    debug!(
        "Voting for {} on prediction {} for judge {}",
        request.decision, request.prediction, request.judge
    );
    backend
        .make_decision(
            &request.prediction,
            &request.judge,
            request.decision,
            access,
        )
        .await
        .map_err(map_any_err_and_code)?;
    Ok(())
}
async fn add_bet(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<AddBetRequest>>,
) -> Result<(StatusCode, Invoice), (StatusCode, Invoice)> {
    let mut backend = state.write().await;
    let (request, access) = (request.data, request.access);
    debug!(
        "Adding bet on {} and prediction {} for user {}",
        request.bet, request.prediction, request.user
    );
    let invoice = backend
        .add_bet(&request.prediction, &request.user, request.bet, access)
        .await
        .map_err(map_any_err_and_code)?;
    Ok((StatusCode::CREATED, invoice))
}
async fn pay_bet(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<PayBetRequest>>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    let (request, access) = (request.data, request.access);
    debug!("Paying bet invoice with {} sats", request.amount);
    let invoice = backend
        .pay_bet(&request.invoice, request.amount, access)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(())
}
async fn cancel_bet(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<CancelBetRequest>>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    let (request, access) = (request.data, request.access);
    let invoice = backend
        .cancel_bet(&request.invoice, &request.refund_invoice, access)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(())
}

async fn check_bet() {}
async fn cash_out_user(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<CashOutUserRequest>>,
) -> Result<Json<Sats>, (StatusCode, String)> {
    let mut backend = state.write().await;
    let (request, access) = (request.data, request.access);
    let sats = backend
        .cash_out_user(&request.prediction, &request.user, &request.invoice, access)
        .await
        .map_err(map_any_err_and_code)?;
    debug!(
        "Cashed out {} sats for user {} on prediction {}",
        sats, request.user, request.prediction
    );
    Ok(Json(sats))
}

async fn get_predictions(
    State(state): State<Arc<RwLock<Mercado>>>,
) -> Result<Json<Vec<PredictionOverviewResponse>>, (StatusCode, String)> {
    let mut backend = state.write().await;
    let predictions = backend
        .get_predictions()
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(predictions.into_values().collect()))
}
async fn get_prediction_overview(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PredictionRequest>,
) -> Result<Json<PredictionOverviewResponse>, (StatusCode, String)> {
    let mut backend = state.write().await;
    let overview = backend
        .get_prediction_overview(request.prediction)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(overview))
}
async fn get_prediction_ratio(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PredictionRequest>,
) -> Result<Json<(Sats, Sats)>, (StatusCode, String)> {
    let mut backend = state.write().await;
    let ratio = backend
        .get_prediction_ratio(request.prediction)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(ratio))
}
async fn get_prediction_judges(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PredictionRequest>,
) -> Result<Json<Vec<Judge>>, (StatusCode, String)> {
    let mut backend = state.write().await;
    let judges = backend
        .get_prediction_judges(request.prediction)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(judges))
}
async fn get_prediction_bets(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PredictionRequest>,
) -> Result<Json<Vec<Bet>>, (StatusCode, String)> {
    let mut backend = state.write().await;
    let bets = backend
        .get_prediction_bets(request.prediction, request.user)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(bets))
}
async fn get_bet() {}
async fn get_user_bets() {}
async fn get_user_prediction_bets() {}

async fn force_decision_period(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<RowId>>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    debug!(
        "Forcing the end of the decision period for prediction {:?}",
        request
    );
    backend
        .force_decision_period(&request.data, request.access)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(())
}
async fn get_login_challenge(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(user): Json<UserPubKey>,
) -> Result<String, (StatusCode, String)> {
    let mut backend = state.write().await;
    trace!("Getting login challenge for {}", user);
    let challenge = backend
        .create_login_challenge(user)
        .await
        .map_err(map_any_err_and_code)?;
    debug!("Login challenge for user {}: {}", user, challenge);
    Ok(challenge)
}
async fn try_login(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<LoginRequest>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    backend
        .try_login(request.user, request.sig, request.challenge)
        .await
        .map_err(map_any_err_and_code)?;
    debug!("User {} successfully logged in", request.user);
    Ok(())
}
async fn check_login(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<AccessRequest>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    backend
        .check_access(request)
        .await
        .map_err(|e| (StatusCode::UNAUTHORIZED, map_any_err(e)))?;
    Ok(())
}
async fn update_user(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<UpdateUserRequest>>,
) -> Result<(), (StatusCode, String)> {
    let mut backend = state.write().await;
    backend
        .update_user(request.data.user, request.data.username, request.access)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(())
}
async fn get_username(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<UserPubKey>,
) -> Result<String, (StatusCode, String)> {
    let backend = state.read().await;
    let username = backend
        .get_username(request)
        .await
        .map_err(map_any_err_and_code)?;
    if let Some(username) = username {
        Ok(username)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            "No username set for user".to_string(),
        ))
    }
}
async fn get_user(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<UserPubKey>>,
) -> Result<Json<UserResponse>, (StatusCode, String)> {
    let backend = state.read().await;
    let user = backend
        .get_user(request.data, request.access)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(user))
}
async fn get_judges(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PredictionUserRequest>,
) -> Result<Json<Vec<JudgePublic>>, (StatusCode, String)> {
    let backend = state.read().await;
    let judges = backend
        .get_judges(request.prediction, request.user)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(judges))
}
async fn get_judge(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<JudgeRequest>>,
) -> Result<Json<Judge>, (StatusCode, String)> {
    let backend = state.read().await;
    let judge = backend
        .get_judge(request.data.prediction, request.data.user, request.access)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(judge))
}
async fn get_bets(
    State(state): State<Arc<RwLock<Mercado>>>,
    Json(request): Json<PostRequest<PredictionUserRequest>>,
) -> Result<Json<Vec<Bet>>, (StatusCode, String)> {
    let backend = state.read().await;
    let bets = backend
        .get_bets(request.data.prediction, request.data.user, request.access)
        .await
        .map_err(map_any_err_and_code)?;
    Ok(Json(bets))
}

const DB_CONN: &str = "sqlite::memory:";

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    admin: Vec<String>,
    #[arg(short, long, default_value_t = 8081)]
    port: u16,
    #[arg(short, long)]
    test: bool,
    #[arg(short, long)]
    db: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    Builder::default()
        .filter_level(LevelFilter::Debug)
        .write_style(WriteStyle::Always)
        .init();
    let cli = Args::parse();
    let (_port, handle) = run_server(Some(cli.port), cli.admin, cli.test, cli.db).await;
    handle.await;
    Ok(())
}

async fn run_server(
    port: Option<u16>,
    admin: Vec<String>,
    test: bool,
    db_conn: Option<String>,
) -> (u16, JoinHandle<()>) {
    let state = Arc::new(RwLock::new(
        Mercado::new(
            Box::new(SQLite::new(db_conn).await),
            Box::new(TestFundingSource::default()),
            admin,
            test,
        )
        .await
        .unwrap(),
    ));
    let app = Router::new()
        .route("/new_prediction", post(new_prediction))
        .route("/accept_nomination", post(accept_nomination))
        .route("/refuse_nomination", post(refuse_nomination))
        .route("/add_bet", post(add_bet))
        .route("/make_decision", post(make_decision))
        .route("/cash_out_user", post(cash_out_user))
        .route("/get_predictions", get(get_predictions))
        .route("/get_prediction_overview", post(get_prediction_overview))
        .route("/get_prediction_ratio", post(get_prediction_ratio))
        .route("/get_prediction_judges", post(get_prediction_judges))
        .route("/get_prediction_bets", post(get_prediction_bets))
        .route("/try_login", post(try_login))
        .route("/check_login", post(check_login))
        .route("/get_login_challenge", post(get_login_challenge))
        .route("/update_user", post(update_user))
        .route("/pay_bet", post(pay_bet))
        .route("/cancel_bet", post(cancel_bet))
        .route("/force_decision_period", post(force_decision_period))
        .route("/get_username", post(get_username))
        .route("/get_user", post(get_user))
        .route("/get_judges", post(get_judges))
        .route("/get_judge", post(get_judge))
        .route("/get_bets", post(get_bets))
        .with_state(state);

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
    use secp256k1::{ecdsa::Signature, generate_keypair, rand};

    use crate::client::Client;

    use super::*;
    fn get_test_access() -> AccessRequest {
        AccessRequest {
            user: UserPubKey::from_str("023d51452445aa81ecc3cfcb82dbfe937707db5c89f9f9d21d64835158df405d8c").unwrap(),
            sig: Signature::from_str("30440220208cef162c7081dafc61004daec32f5a3dadb4c6a1b4c0a479056a4962288d47022069022bc92673f73e9843cea14fa0cc46efa1b1e150339b603444c63035de21ee").unwrap(),
            challenge: "iT1HqC3oaoGjbSZEjAwpGZiCbzjtyz".to_string()
        }
    }

    #[tokio::test]
    async fn new_prediction() {
        let (port, _) = run_server(None, vec![], true, None).await;
        let client = Client::new("http://127.0.0.1:".to_string() + port.to_string().as_str());

        let (_, j1) = generate_keypair(&mut rand::thread_rng());
        let (_, j2) = generate_keypair(&mut rand::thread_rng());
        let (_, j3) = generate_keypair(&mut rand::thread_rng());

        let mut prediction_request = PredictionRequest {
            prediction: 1,
            user: None,
        };
        client
            .get_prediction_overview(prediction_request.clone())
            .await
            .unwrap_err();

        // Create a new Prediction
        let prediction_http_request = NewPredictionRequest {
            prediction: "Test prediction".into(),
            judges: vec![j1, j2, j3],
            judge_share_ppm: 100000,
            trading_end: Utc::now() + Duration::days(3),
            decision_period_sec: Duration::days(1).num_seconds().try_into().unwrap(),
            judge_count: 2,
        };
        let response = client.new_prediction(prediction_http_request.clone()).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let prediction_id = response.json::<RowId>().await.unwrap();
        prediction_request.prediction = prediction_id;

        let prediction = client
            .get_prediction_overview(prediction_request.clone())
            .await
            .unwrap();
        let ratio = client
            .get_prediction_ratio(prediction_request.clone())
            .await
            .unwrap();
        let judges = client
            .get_prediction_judges(prediction_request)
            .await
            .unwrap();
        assert_eq!(
            prediction,
            PredictionOverviewResponse {
                id: prediction_id,
                name: "Test prediction".into(),
                state: MarketState::WaitingForJudges,
                judge_share_ppm: 100000,
                judge_count: 2,
                trading_end: Utc
                    .timestamp_opt(prediction_http_request.trading_end.timestamp(), 0)
                    .unwrap(),
                decision_period_sec: 86400,
            }
        )
    }

    #[tokio::test]
    async fn all() {
        // Builder::default()
        //     .filter_level(LevelFilter::Debug)
        //     .write_style(WriteStyle::Always)
        //     .init();
        let (port, _) = run_server(None, vec![], true, None).await;
        let client = Client::new("http://127.0.0.1:".to_string() + port.to_string().as_str());
        let access = get_test_access();

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
        let request = NominationRequest {
            prediction: prediction_id,
            user: j3,
        };
        let response = client
            .refuse_nomination(request, access.clone())
            .await
            .unwrap();

        // Accept Nomination for 2 judges
        for judge in [j1, j2] {
            let request = NominationRequest {
                prediction: prediction_id,
                user: judge,
            };
            let response = client
                .accept_nomination(request, access.clone())
                .await
                .unwrap();
        }

        // Add bet for 3 users
        for user in [u1, u2, u3] {
            let request = AddBetRequest {
                prediction: prediction_id,
                user,
                bet: true,
            };
            let invoice = client.add_bet(request, access.clone()).await.unwrap();
            let request = PayBetRequest {
                invoice,
                amount: 100,
            };
            let response = client.pay_bet(request, access.clone()).await.unwrap();
        }

        // Forcing the end of the decision period
        let response = client
            .force_decision_period(prediction_id, access.clone())
            .await
            .unwrap();

        // Voting for outcomes for 2 judges
        for judge in [j1, j2] {
            let request = MakeDecisionRequest {
                prediction: prediction_id,
                judge,
                decision: true,
            };
            let response = client.make_decision(request, access.clone()).await.unwrap();
        }

        // Cash out users
        for user in [u1, u2, u3] {
            let request = CashOutUserRequest {
                prediction: prediction_id,
                user,
                invoice: user.to_string(),
            };
            let sats = client.cash_out_user(request, access.clone()).await.unwrap();
            assert_eq!(sats, 89);
        }

        // Cash out judges
        for judge in [j1, j2] {
            let request = CashOutUserRequest {
                prediction: prediction_id,
                user: judge,
                invoice: judge.to_string(),
            };
            let sats = client.cash_out_user(request, access.clone()).await.unwrap();
            assert_eq!(sats, 15);
        }

        let predictions = client.get_predictions().await.unwrap();
        let prediction = predictions.first().unwrap();
        let ratio = client
            .get_prediction_ratio(PredictionRequest {
                user: None,
                prediction: prediction_id,
            })
            .await
            .unwrap();
        assert_eq!(ratio.0, 300);
        assert_eq!(prediction.name, "Test prediction".to_string());
    }
}
