use anyhow::{bail, Ok, Result};
use reqwest::{blocking::Response, StatusCode};
use serde::Serialize;

use crate::api::*;

#[derive(Debug)]
pub struct Client {
    url: String,
    client: reqwest::blocking::Client,
}
impl Client {
    pub fn new(url: String) -> Self {
        let client = reqwest::blocking::Client::new();
        Self { url, client }
    }
    fn post(
        &self,
        path: &'static str,
        request: impl Serialize,
        expexted_code: StatusCode,
    ) -> Result<Response> {
        let response = self
            .client
            .post(self.url.clone() + path)
            .json(&request)
            .send()?;
        bail_if_err(response, expexted_code)
    }
    fn get(&self, path: &'static str, expexted_code: StatusCode) -> Result<Response> {
        let response = self.client.get(self.url.clone() + path).send()?;
        bail_if_err(response, expexted_code)
    }
    pub fn new_prediction(&self, request: NewPredictionRequest) -> Response {
        self.client
            .post(self.url.clone() + "/new_prediction")
            .json(&request)
            .send()
            .unwrap()
    }
    pub fn accept_nomination(
        &self,
        request: NominationRequest,
        access: AccessRequest,
    ) -> Result<()> {
        self.post(
            "/accept_nomination",
            PostRequest {
                data: request,
                access,
            },
            StatusCode::OK,
        )?;
        Ok(())
    }
    pub fn refuse_nomination(
        &self,
        request: NominationRequest,
        access: AccessRequest,
    ) -> Result<()> {
        self.post(
            "/refuse_nomination",
            PostRequest {
                data: request,
                access,
            },
            StatusCode::OK,
        )?;
        Ok(())
    }
    pub fn make_decision(&self, request: MakeDecisionRequest, access: AccessRequest) -> Result<()> {
        self.post(
            "/make_decision",
            PostRequest {
                data: request,
                access,
            },
            StatusCode::OK,
        )?;
        Ok(())
    }
    pub fn add_bet(&self, request: AddBetRequest, access: AccessRequest) -> Result<Invoice> {
        let response = self.post(
            "/add_bet",
            PostRequest {
                data: request,
                access,
            },
            StatusCode::CREATED,
        )?;
        Ok(response.text()?)
    }
    pub fn pay_bet(&self, request: PayBetRequest, access: AccessRequest) -> Result<()> {
        self.post(
            "/pay_bet",
            PostRequest {
                data: request,
                access,
            },
            StatusCode::OK,
        )?;
        Ok(())
    }
    pub fn check_bet(&self) {}
    pub fn cancel_bet(&self, request: CancelBetRequest, access: AccessRequest) -> Result<()> {
        self.post(
            "/cancel_bet",
            PostRequest {
                data: request,
                access,
            },
            StatusCode::OK,
        )?;
        Ok(())
    }
    pub fn cash_out_user(
        &self,
        request: CashOutUserRequest,
        access: AccessRequest,
    ) -> Result<Sats> {
        let response = self.post(
            "/cash_out_user",
            PostRequest {
                data: request,
                access,
            },
            StatusCode::OK,
        )?;
        Ok(response.json::<Sats>()?)
    }
    pub fn force_decision_period(&self, prediction: RowId, access: AccessRequest) -> Result<()> {
        self.post(
            "/force_decision_period",
            PostRequest {
                data: prediction,
                access,
            },
            StatusCode::OK,
        )?;
        Ok(())
    }
    pub fn get_predictions(&self) -> Result<Vec<PredictionOverviewResponse>> {
        let response = self.get("/get_predictions", StatusCode::OK)?;
        Ok(response.json::<Vec<PredictionOverviewResponse>>()?)
    }
    pub fn get_prediction_ratio(&self, request: PredictionRequest) -> Result<(Sats, Sats)> {
        let response = self.post("/get_prediction_ratio", request, StatusCode::OK)?;
        Ok(response.json::<(Sats, Sats)>()?)
    }
    pub fn get_prediction_judges(&self, request: PredictionRequest) -> Result<Vec<Judge>> {
        let response = self.post("/get_prediction_judges", request, StatusCode::OK)?;
        Ok(response.json::<Vec<Judge>>()?)
    }
    pub fn get_prediction_overview(
        &self,
        request: PredictionRequest,
    ) -> Result<PredictionOverviewResponse> {
        let response = self.post("/get_prediction_overview", request, StatusCode::OK)?;
        Ok(response.json::<PredictionOverviewResponse>()?)
    }
    pub fn get_prediction_bets(&self, request: PredictionRequest) -> Result<Vec<Bet>> {
        let response = self.post("/get_prediction_bets", request, StatusCode::OK)?;
        Ok(response.json::<Vec<Bet>>()?)
    }
    pub fn get_login_challenge(&self, user: UserPubKey) -> Result<String> {
        let response = self.post("/get_login_challenge", user, StatusCode::OK)?;
        Ok(response.text()?)
    }
    pub fn try_login(&self, request: LoginRequest) -> Result<()> {
        let _response = self.post("/try_login", request, StatusCode::OK)?;
        Ok(())
    }
    pub fn check_login(&self, access: AccessRequest) -> Result<()> {
        self.post("/check_login", access, StatusCode::OK)?;
        Ok(())
    }
    pub fn update_user(&self, request: UpdateUserRequest, access: AccessRequest) -> Result<()> {
        self.post(
            "/update_user",
            PostRequest {
                data: request,
                access,
            },
            StatusCode::OK,
        )?;
        Ok(())
    }
    pub fn get_username(&self, user: UserPubKey) -> Result<String> {
        let response = self.post("/get_username", user, StatusCode::OK)?;
        Ok(response.text()?)
    }
}

fn bail_if_err(response: Response, expexted_code: StatusCode) -> Result<Response> {
    if response.status() != expexted_code {
        bail!("{}: {}", response.status(), response.text()?)
    } else {
        Ok(response)
    }
}
