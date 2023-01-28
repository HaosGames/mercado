use std::sync::Arc;
use log::{debug, warn};
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};

use crate::mercado::Mercado;
use hello_world::api_server::{Api, ApiServer};
use hello_world::{
    CreatePredictionRequest, GenericResponse, GetPredictionRequest, GetPredictionResponse,
};
use crate::db::{DB, TestDB};
use crate::funding_source::FundingSource;

mod api;
mod db;
mod funding_source;
mod mercado;

pub mod hello_world {
    tonic::include_proto!("api");
}

pub struct MyApi {
    market: Mercado,
}
impl MyApi {
    fn new(market: Mercado) -> Self {
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let market = Mercado::new(
        DB::Test(Arc::new(Mutex::new(TestDB::default()))),
        FundingSource::Test,
    );;
    let api = MyApi::new(market);

    Server::builder()
        .add_service(ApiServer::new(api))
        .serve(addr)
        .await?;

    Ok(())
}

#[allow(unused)]
#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn deposit() {}
}
