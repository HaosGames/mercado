use log::info;
use tonic::{transport::Server, Request, Response, Status};

use hello_world::api_server::{Api, ApiServer};
use hello_world::{FundChange, GenericResponse};

mod mercado;
mod market;
pub mod hello_world {
    tonic::include_proto!("api");
}

#[derive(Debug, Default)]
pub struct MyApi {}

#[tonic::async_trait]
impl Api for MyApi {
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
    let greeter = MyApi::default();

    Server::builder()
        .add_service(ApiServer::new(greeter))
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
