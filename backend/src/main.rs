use log::{debug, warn};
use tonic::{transport::Server, Request, Response, Status};

/*use crate::mercado::Mercado;
use hello_world::api_server::{Api, ApiServer};
use hello_world::{
    CreateMarketRequest, CreateUser, DepositRequest, GenericResponse, GetFundsRequest,
    GetFundsResponse, GetMarketRequest, GetMarketResponse, WithdrawRequest,
};*/

// mod mercado;
mod api;
mod db;
mod funding_source;
mod platform;
/*
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
    async fn create_user(
        &self,
        request: Request<CreateUser>,
    ) -> Result<Response<GenericResponse>, Status> {
        let request = request.into_inner();
        if let Err(MercadoError::UserAlreadyExists) =
            self.market.add_user(request.username.as_str()).await
        {
            let message = format!("User {} already exists", request.username);
            debug!("{}", message);
            return Err(Status::already_exists(message));
        }
        let message = format!("Created user {}", request.username);
        debug!("{}", message);
        Ok(Response::new(GenericResponse { message }))
    }
    async fn deposit(
        &self,
        request: Request<DepositRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let request = request.into_inner();
        self.market
            .deposit_funds(&request.user, request.amount.into())
            .await;
        let message = format!(
            "Deposited {} Sats for user {}",
            request.amount, request.user
        );
        debug!("{}", message);
        Ok(Response::new(GenericResponse { message }))
    }
    async fn withdraw(
        &self,
        request: Request<WithdrawRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let request = request.into_inner();
        if let Err(e) = self
            .market
            .withdraw_funds(&request.user, request.amount.into())
            .await
        {
            warn!("{}", e.to_string());
            return Err(Status::unknown(e.to_string()));
        }
        let message = format!(
            "Withdrawn {} Sats for user {}",
            request.amount, request.user
        );
        debug!("{}", message);
        Ok(Response::new(GenericResponse { message }))
    }
    async fn get_funds(
        &self,
        request: Request<GetFundsRequest>,
    ) -> Result<Response<GetFundsResponse>, Status> {
        let request = request.into_inner();
        let sats = match self.market.get_funds(&request.user).await {
            Ok(sats) => sats,
            Err(e) => {
                warn!("{}", e.to_string());
                return Err(Status::unknown(e.to_string()));
            }
        };
        Ok(Response::new(GetFundsResponse {
            sats: sats.try_into().unwrap(),
        }))
    }
    async fn create_market(
        &self,
        request: Request<CreateMarketRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let request = request.into_inner();
        if let Err(e) = self
            .market
            .create_market(
                request.id.as_str(),
                request.assumption.as_str(),
                request.judge_share,
                std::time::Duration::from_secs(request.decision_period_seconds.into()).into(),
                request.trading_end.as_str().into(),
                request.judges,
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
    async fn get_market(
        &self,
        request: Request<GetMarketRequest>,
    ) -> Result<Response<GetMarketResponse>, Status> {
        let request = request.into_inner();
        let market = match self.market.get_market(request.id.as_str()).await {
            Ok(market) => market,
            Err(e) => {
                warn!("{}", e.to_string());
                return Err(Status::unknown(e.to_string()));
            }
        };
        Ok(Response::new(GetMarketResponse {
            assumption: market.assumption,
            judge_share: market.judge_share as f32,
            trading_end: market.trading_end.to_string(),
            decision_period_seconds: market.decision_period.as_secs().try_into().unwrap(),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let market = Mercado::new().await;
    let api = MyApi::new(market);

    Server::builder()
        .add_service(ApiServer::new(api))
        .serve(addr)
        .await?;

    Ok(())
}
*/
fn main() {}

#[allow(unused)]
#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn deposit() {}
}
