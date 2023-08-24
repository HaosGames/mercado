use anyhow::{bail, Ok, Result};
use reqwest::{Response, StatusCode};

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
    pub async fn new_prediction(&self, request: NewPredictionRequest) -> Response {
        self.client
            .post(self.url.clone() + "/new_prediction")
            .json(&request)
            .send()
            .await
            .unwrap()
    }
    pub async fn accept_nomination(&self, request: AcceptNominationRequest) -> Result<()> {
        let response = self
            .client
            .post(self.url.clone() + "/accept_nomination")
            .json(&request)
            .send()
            .await?;
        bail_if_err(response).await?;
        Ok(())
    }
    pub async fn refuse_nomination(&self, request: AcceptNominationRequest) -> Result<()> {
        let response = self
            .client
            .post(self.url.clone() + "/refuse_nomination")
            .json(&request)
            .send()
            .await?;
        bail_if_err(response).await?;
        Ok(())
    }
    pub async fn make_decision(&self, request: MakeDecisionRequest) -> Response {
        self.client
            .post(self.url.clone() + "/make_decision")
            .json(&request)
            .send()
            .await
            .unwrap()
    }
    pub async fn add_bet(&self, request: AddBetRequest) -> Result<Invoice> {
        let response = self
            .client
            .post(self.url.clone() + "/add_bet")
            .json(&request)
            .send()
            .await?;
        if response.status() != StatusCode::CREATED {
            bail!("{}: {}", response.status(), response.text().await?)
        }
        Ok(response.text().await?)
    }
    #[cfg(test)]
    pub async fn pay_bet(&self, request: PayBetRequest) -> Response {
        self.client
            .post(self.url.clone() + "/pay_bet")
            .json(&request)
            .send()
            .await
            .unwrap()
    }
    pub async fn check_bet(&self) {}
    pub async fn cancel_bet(&self) {}
    pub async fn cash_out_user(&self, request: CashOutUserRequest) -> Response {
        self.client
            .post(self.url.clone() + "/cash_out_user")
            .json(&request)
            .send()
            .await
            .unwrap()
    }
    #[cfg(test)]
    pub async fn force_decision_period(&self, prediction: RowId) -> Response {
        self.client
            .post(self.url.clone() + "/force_decision_period")
            .json(&prediction)
            .send()
            .await
            .unwrap()
    }
    pub async fn get_predictions(&self) -> Result<Vec<PredictionOverviewResponse>> {
        let response = self
            .client
            .get(self.url.clone() + "/get_predictions")
            .send()
            .await?;
        let response = bail_if_err(response).await?;
        Ok(response.json::<Vec<PredictionOverviewResponse>>().await?)
    }
    pub async fn get_prediction_ratio(&self, request: PredictionRequest) -> Result<(Sats, Sats)> {
        let response = self
            .client
            .post(self.url.clone() + "/get_prediction_ratio")
            .json(&request)
            .send()
            .await?;
        let response = bail_if_err(response).await?;
        let ratio = response.json::<(Sats, Sats)>().await?;
        Ok(ratio)
    }
    pub async fn get_prediction_judges(&self, request: PredictionRequest) -> Result<Vec<Judge>> {
        let response = self
            .client
            .post(self.url.clone() + "/get_prediction_judges")
            .json(&request)
            .send()
            .await?;
        let response = bail_if_err(response).await?;
        Ok(response.json::<Vec<Judge>>().await?)
    }
    pub async fn get_prediction_overview(
        &self,
        request: PredictionRequest,
    ) -> Result<PredictionOverviewResponse> {
        let response = self
            .client
            .post(self.url.clone() + "/get_prediction_overview")
            .json(&request)
            .send()
            .await?;
        let response = bail_if_err(response).await?;
        Ok(response.json::<PredictionOverviewResponse>().await?)
    }
    pub async fn get_prediction_bets(&self, request: PredictionRequest) -> Result<Vec<Bet>> {
        let response = self
            .client
            .post(self.url.clone() + "/get_prediction_bets")
            .json(&request)
            .send()
            .await?;
        let response = bail_if_err(response).await?;
        Ok(response.json::<Vec<Bet>>().await?)
    }
}
async fn bail_if_err(response: Response) -> Result<Response> {
    if response.status() != StatusCode::OK {
        bail!("{}: {}", response.status(), response.text().await?)
    } else {
        Ok(response)
    }
}
