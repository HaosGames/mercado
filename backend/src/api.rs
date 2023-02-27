use crate::hello_world::api_server::Api;
use crate::hello_world::{
    AddBetRequest, CreatePredictionRequest, GenericResponse, GetPredictionRequest,
    GetPredictionResponse, Invoice, NostrEvent,
};
use crate::mercado::Mercado;
use log::{debug, warn};
use tonic::{Request, Response, Status};

pub struct MyApi {
    market: Mercado,
}
impl MyApi {
    pub fn new(market: Mercado) -> Self {
        Self { market }
    }
}

#[tonic::async_trait]
impl Api for MyApi {
    async fn create_prediction(
        &self,
        request: Request<CreatePredictionRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let request = request.into_inner();
        if let Err(e) = self
            .market
            .new_prediction(
                request.prediction,
                request.id.clone(),
                request
                    .judges
                    .iter()
                    .map(|key| key.parse().unwrap())
                    .collect(),
                request.judge_count,
                request.judge_share_ppm,
                request.trading_end.parse().unwrap(),
                chrono::Duration::seconds(request.decision_period_seconds.into()),
            )
            .await
        {
            warn!("{}", e.to_string());
            return Err(Status::unknown(e.to_string()));
        };
        let message = format!("Created market {}", request.id);
        debug!("{}", message);
        Ok(Response::new(GenericResponse { message }))
    }
    async fn get_prediction(
        &self,
        request: Request<GetPredictionRequest>,
    ) -> Result<Response<GetPredictionResponse>, Status> {
        let request = request.into_inner();
        let market = match self.market.get_prediction(&request.id).await {
            Ok(market) => market,
            Err(e) => {
                warn!("{}", e.to_string());
                return Err(Status::unknown(e.to_string()));
            }
        };
        Ok(Response::new(market))
    }

    async fn accept_nomination(
        &self,
        request: Request<NostrEvent>,
    ) -> Result<Response<GenericResponse>, Status> {
        todo!()
    }

    async fn refuse_nomination(
        &self,
        request: Request<NostrEvent>,
    ) -> Result<Response<GenericResponse>, Status> {
        todo!()
    }

    async fn make_decision(
        &self,
        request: Request<NostrEvent>,
    ) -> Result<Response<GenericResponse>, Status> {
        todo!()
    }

    async fn add_bet(&self, request: Request<AddBetRequest>) -> Result<Response<Invoice>, Status> {
        todo!()
    }
}
