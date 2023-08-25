use anyhow::{bail, Ok, Result};
use reqwest::{Response, StatusCode};
use serde::Serialize;

use crate::api::*;

pub struct Client {
    url: String,
    client: reqwest::Client,
}
impl Client {
    pub fn new(url: String) -> Self {
        let client = reqwest::Client::new();
        Self { url, client }
    }
    async fn post(
        &self,
        path: &'static str,
        request: impl Serialize,
        expexted_code: StatusCode,
    ) -> Result<Response> {
        let response = self
            .client
            .post(self.url.clone() + path)
            .json(&request)
            .send()
            .await?;
        bail_if_err(response, expexted_code).await
    }
    async fn get(&self, path: &'static str, expexted_code: StatusCode) -> Result<Response> {
        let response = self.client.get(self.url.clone() + path).send().await?;
        bail_if_err(response, expexted_code).await
    }
    pub async fn new_prediction(&self, request: NewPredictionRequest) -> Response {
        self.client
            .post(self.url.clone() + "/new_prediction")
            .json(&request)
            .send()
            .await
            .unwrap()
    }
    pub async fn accept_nomination(&self, request: AcceptNominationRequest) -> Result<()> {
        self.post("/accept_nomination", request, StatusCode::OK)
            .await?;
        Ok(())
    }
    pub async fn refuse_nomination(&self, request: AcceptNominationRequest) -> Result<()> {
        self.post("/refuse_nomination", request, StatusCode::OK)
            .await?;
        Ok(())
    }
    pub async fn make_decision(&self, request: MakeDecisionRequest) -> Result<()> {
        self.post("/make_decision", request, StatusCode::OK).await?;
        Ok(())
    }
    pub async fn add_bet(&self, request: AddBetRequest) -> Result<Invoice> {
        let response = self.post("/add_bet", request, StatusCode::CREATED).await?;
        Ok(response.text().await?)
    }
    pub async fn pay_bet(&self, request: PayBetRequest) -> Result<()> {
        self.post("/pay_bet", request, StatusCode::OK).await?;
        Ok(())
    }
    pub async fn check_bet(&self) {}
    pub async fn cancel_bet(&self) {}
    pub async fn cash_out_user(&self, request: CashOutUserRequest) -> Result<Sats> {
        let response = self.post("/cash_out_user", request, StatusCode::OK).await?;
        Ok(response.json::<Sats>().await?)
    }
    pub async fn force_decision_period(&self, prediction: RowId) -> Result<()> {
        self.post("/force_decision_period", prediction, StatusCode::OK)
            .await?;
        Ok(())
    }
    pub async fn get_predictions(&self) -> Result<Vec<PredictionOverviewResponse>> {
        let response = self.get("/get_predictions", StatusCode::OK).await?;
        Ok(response.json::<Vec<PredictionOverviewResponse>>().await?)
    }
    pub async fn get_prediction_ratio(&self, request: PredictionRequest) -> Result<(Sats, Sats)> {
        let response = self
            .post("/get_prediction_ratio", request, StatusCode::OK)
            .await?;
        Ok(response.json::<(Sats, Sats)>().await?)
    }
    pub async fn get_prediction_judges(&self, request: PredictionRequest) -> Result<Vec<Judge>> {
        let response = self
            .post("/get_prediction_judges", request, StatusCode::OK)
            .await?;
        Ok(response.json::<Vec<Judge>>().await?)
    }
    pub async fn get_prediction_overview(
        &self,
        request: PredictionRequest,
    ) -> Result<PredictionOverviewResponse> {
        let response = self
            .post("/get_prediction_overview", request, StatusCode::OK)
            .await?;
        Ok(response.json::<PredictionOverviewResponse>().await?)
    }
    pub async fn get_prediction_bets(&self, request: PredictionRequest) -> Result<Vec<Bet>> {
        let response = self
            .post("/get_prediction_bets", request, StatusCode::OK)
            .await?;
        Ok(response.json::<Vec<Bet>>().await?)
    }
    pub async fn get_login_challenge(&self, user: UserPubKey) -> Result<String> {
        let response = self
            .post("/get_login_challenge", user, StatusCode::OK)
            .await?;
        Ok(response.text().await?)
    }
    pub async fn try_login(&self, request: LoginRequest) -> Result<()> {
        let _response = self.post("/try_login", request, StatusCode::OK).await?;
        Ok(())
    }
    pub async fn update_user(&self, request: PostRequest<UpdateUserRequest>) -> Result<()> {
        self.post("/update_user", request, StatusCode::OK).await?;
        Ok(())
    }
}

async fn bail_if_err(response: Response, expexted_code: StatusCode) -> Result<Response> {
    if response.status() != expexted_code {
        bail!("{}: {}", response.status(), response.text().await?)
    } else {
        Ok(response)
    }
}
