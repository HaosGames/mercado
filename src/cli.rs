#![allow(unused)]
use std::str::FromStr;

use anyhow::Result;
use api::*;
use chrono::Utc;
use clap::{Parser, Subcommand};
use secp256k1::{generate_keypair, hashes::sha256::Hash, rand, Message, SecretKey, SECP256K1};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

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
    GetPrediction {
        #[arg(short, long)]
        prediction: RowId,
        #[arg(short, long)]
        user: Option<String>,
    },
    AddBet {
        #[arg(short, long)]
        bet: bool,
        #[arg(short, long)]
        amount: u32,
        #[arg(short, long)]
        prediction: u32,
    },
    GenerateKeys,
    Login,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Args::parse();
    let client = Client::new(cli.url);

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match cli.command {
        Commands::NewPrediction {
            prediction,
            judges,
            share_ppm,
        } => {
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
            let response = client.get_predictions().await?;
            println!("{:#?}", response);
        }
        Commands::GetPrediction { user, prediction } => {
            let request = PredictionRequest {
                user: if let Some(user) = user {
                    Some(UserPubKey::from_str(user.as_str()).unwrap())
                } else {
                    None
                },
                prediction,
            };
            let response = client.get_prediction_overview(request.clone()).await?;
            println!("{:#?}", response);
            let response = client.get_prediction_ratio(request.clone()).await?;
            println!("True: {} sats | False {} sats", response.0, response.1);
            let response = client.get_prediction_judges(request.clone()).await?;
            println!("Judges: {:#?}", response);
        }
        Commands::AddBet {
            bet,
            amount,
            prediction,
        } => {
            let request = AddBetRequest {
                bet,
                prediction: prediction.into(),
                user: generate_keypair(&mut rand::thread_rng()).1,
            };
            let invoice = client.add_bet(request).await?;
            println!("Invoice: {}", invoice);
        }
        Commands::GenerateKeys => {
            let keys = generate_keypair(&mut rand::thread_rng());
            println!("Pubkey: {}", keys.1);
            let mut private = File::create("ecdsa.key").await?;
            let mut public = File::create("ecdsa.pub").await?;
            private
                .write_all(format!("{}", keys.0.display_secret()).as_bytes())
                .await?;
            public.write_all(keys.1.to_string().as_bytes()).await?;
        }
        Commands::Login => {
            let secret_key = read_secret().await?;
            let user = UserPubKey::from_secret_key_global(&secret_key);
            let challenge = client.get_login_challenge(user.clone()).await?;
            let message = Message::from_hashed_data::<Hash>(challenge.as_bytes());
            let signature = secret_key.sign_ecdsa(message);
            println!("Signed Challenge \"{}\"", challenge);
            let request = LoginRequest {
                user,
                sig: signature,
            };
            client.try_login(request).await?;
            println!("Logged in as {}", user);
        }
    }
    Ok(())
}
async fn read_secret() -> Result<SecretKey> {
    let mut file = File::open("ecdsa.key").await?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).await?;
    Ok(SecretKey::from_str(String::from_utf8(contents)?.as_str())?)
}
