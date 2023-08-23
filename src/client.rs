use anyhow::{bail, Result};
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
    pub async fn accept_nomination(&self, request: AcceptNominationRequest) -> Response {
        self.client
            .post(self.url.clone() + "/accept_nomination")
            .json(&request)
            .send()
            .await
            .unwrap()
    }
    pub async fn refuse_nomination(&self, request: AcceptNominationRequest) -> Response {
        self.client
            .post(self.url.clone() + "/refuse_nomination")
            .json(&request)
            .send()
            .await
            .unwrap()
    }
    pub async fn make_decision(&self, request: MakeDecisionRequest) -> Response {
        self.client
            .post(self.url.clone() + "/make_decision")
            .json(&request)
            .send()
            .await
            .unwrap()
    }
    pub async fn add_bet(&self, request: AddBetRequest) -> Response {
        self.client
            .post(self.url.clone() + "/add_bet")
            .json(&request)
            .send()
            .await
            .unwrap()
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
    pub async fn get_predictions(&self) -> Result<Vec<PredictionListItemResponse>> {
        let response = self
            .client
            .get(self.url.clone() + "/get_predictions")
            .send()
            .await?;
        Ok(response.json::<Vec<PredictionListItemResponse>>().await?)
    }
    pub async fn get_user_prediction(
        &self,
        prediction: RowId,
        user: UserPubKey,
    ) -> Result<UserPredictionOverviewResponse> {
        let request = UserPredictionOverviewRequest { prediction, user };
        let response = self
            .client
            .post(self.url.clone() + "/get_user_prediction")
            .json(&request)
            .send()
            .await?;
        if response.status() != StatusCode::OK {
            bail!("{}: {}", response.status(), response.text().await?)
        }
        Ok(response.json::<UserPredictionOverviewResponse>().await?)
    }
}
