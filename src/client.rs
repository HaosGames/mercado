#![allow(unused)]
use anyhow::Result;
use api::NewPredictionRequest;
use clap::{Parser, Subcommand};
use secp256k1::{generate_keypair, rand};

mod api;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Commands,
    api: String,
}
#[derive(Subcommand)]
enum Commands {
    NewPrediction {
        prediction: String,
        judge: Vec<String>,
        share_ppm: u32,
        end: i64,
        decision_period_sec: u32,
        judge_count: u32,
    },
    Tests,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Args::parse();
    let client = reqwest::Client::new();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        Commands::NewPrediction {
            prediction,
            judge,
            share_ppm,
            end,
            decision_period_sec,
            judge_count,
        } => {
            println!("'myapp add' was used, name")
        }
        Commands::Tests => {
            let (_, u1) = generate_keypair(&mut rand::thread_rng());
            let (_, u2) = generate_keypair(&mut rand::thread_rng());
            let (_, u3) = generate_keypair(&mut rand::thread_rng());
            let (_, j1) = generate_keypair(&mut rand::thread_rng());
            let (_, j2) = generate_keypair(&mut rand::thread_rng());
            let (_, j3) = generate_keypair(&mut rand::thread_rng());

            let prediction = NewPredictionRequest {
                prediction: "Test prediction".into(),
                judges: vec![j1, j2, j3],
                judge_share_ppm: todo!(),
                trading_end: todo!(),
                decision_period_sec: todo!(),
                judge_count: todo!(),
                bets_true: todo!(),
                bets_false: todo!(),
            };
            let response = client
                .post("http://127.0.0.1:8081/new_prediction")
                .json(&prediction)
                .send()
                .await?;
        }
    }
    Ok(())
}
