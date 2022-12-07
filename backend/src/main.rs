use log::info;
use tonic::{transport::Server, Request, Response, Status};

use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{FundChange, GenericResponse, HelloReply, HelloRequest};

mod db;
mod market;
pub mod hello_world {
    tonic::include_proto!("helloworld");
}

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);

        let reply = hello_world::HelloReply {
            message: format!("Hello {}!", request.into_inner().name).into(),
        };

        Ok(Response::new(reply))
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
    let greeter = MyGreeter::default();

    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deposit() {}
}
