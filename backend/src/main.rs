//use crate::api::MyApi;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Server;

use crate::db::{SQLite, DB};
use crate::funding_source::FundingSource;
use crate::hello_world::api_server::ApiServer;
use crate::mercado::Mercado;

//mod api;
mod db;
mod funding_source;
mod mercado;

mod hello_world {
    tonic::include_proto!("api");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /*let addr = "[::1]:50051".parse()?;
        let market = Mercado::new(
            DB::Test(Arc::new(Mutex::new(SQLite::new("memory")))),
            FundingSource::Test,
        );
        let api = MyApi::new(market);

        Server::builder()
            .add_service(ApiServer::new(api))
            .serve(addr)
            .await?;
    */
    Ok(())
}

#[allow(unused)]
#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn deposit() {}
}
