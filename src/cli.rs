#![allow(unused)]
use anyhow::Result;
use api::*;
use chrono::Utc;
use clap::{Parser, Subcommand};
use secp256k1::{generate_keypair, rand};

use crate::client::Client;

mod api;
mod client;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long)]
    url: String,
}
#[derive(Subcommand)]
enum Commands {
    NewPrediction {
        #[arg(short, long)]
        prediction: String,
        #[arg(short, long)]
        judges: u32,
        #[arg(short, long)]
        share_ppm: u32,
    },
    GetPredictions,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Args::parse();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match cli.command {
        Commands::NewPrediction {
            prediction,
            judges,
            share_ppm,
        } => {
            let client = Client::new(cli.url);
            let mut new_judges = vec![];
            for i in 0..judges {
                new_judges.push(generate_keypair(&mut rand::thread_rng()).1);
            }
            let request = NewPredictionRequest {
                prediction,
                judges: new_judges,
                judge_share_ppm: share_ppm,
                trading_end: "2023-12-12T12:12:12Z".parse().unwrap(),
                decision_period_sec: 86400,
                judge_count: 3,
            };
            let response = client.new_prediction(request).await;
            println!("{}: {}", response.status(), response.text().await.unwrap());
        }
        Commands::GetPredictions => {
            let client = Client::new(cli.url);
            let response = client.get_predictions().await?;
            println!("{:#?}", response);
        }
    }
    Ok(())
}
