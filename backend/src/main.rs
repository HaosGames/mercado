use log::{info, warn};
use tonic::{transport::Server, Request, Response, Status};

use crate::mercado::Mercado;
use hello_world::api_server::{Api, ApiServer};
use hello_world::{CreateUser, FundChange, GenericResponse};
use crate::market::MercadoError;

mod market;
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
    async fn create_user(
        &self,
        request: Request<CreateUser>,
    ) -> Result<Response<GenericResponse>, Status> {
        let request = request.into_inner();
        if let Err(MercadoError::UserAlreadyExists) = self.market.add_user(request.username.as_str()).await {
            let message = format!("User {} already exists", request.username);
            warn!("{}", message);
            return Err(Status::already_exists(message))
        }
        let message = format!("Created user {}", request.username);
        info!("{}", message);
        Ok(Response::new(GenericResponse { message }))
    }
    async fn deposit(
        &self,
        request: Request<FundChange>,
    ) -> Result<Response<GenericResponse>, Status> {
        let request = request.into_inner();
        let message = format!(
            "Deposited {} Sats for user {}",
            request.amount, request.user
        );
        info!("{}", message);
        Ok(Response::new(GenericResponse { message }))
    }
    async fn withdraw(
        &self,
        request: Request<FundChange>,
    ) -> Result<Response<GenericResponse>, Status> {
        let request = request.into_inner();
        let message = format!(
            "Withdrawn {} Sats for user {}",
            request.amount, request.user
        );
        info!("{}", message);
        Ok(Response::new(GenericResponse { message }))
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

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn deposit() {}
}
