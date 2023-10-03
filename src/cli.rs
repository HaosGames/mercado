#![allow(unused)]
use std::str::FromStr;

use anyhow::Result;
use api::*;
use chrono::Utc;
use clap::{Parser, Subcommand};
use secp256k1::{
    ecdsa::Signature, generate_keypair, hashes::sha256::Hash, rand, Message, SecretKey, SECP256K1,
};
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
    AcceptNomination {
        #[arg(short, long)]
        prediction: RowId,
        #[arg(short, long)]
        judge: UserPubKey,
    },
    RefuseNomination {
        #[arg(short, long)]
        prediction: RowId,
        #[arg(short, long)]
        judge: UserPubKey,
    },
    MakeDecision {
        #[arg(short, long)]
        prediction: RowId,
        #[arg(short, long)]
        judge: UserPubKey,
        #[arg(short, long)]
        decision_true: bool,
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
        bet_true: bool,
        #[arg(short, long)]
        amount: Sats,
        #[arg(short, long)]
        prediction: u32,
        #[arg(short, long)]
        user: UserPubKey,
    },
    CancelBet {
        #[arg(short, long)]
        id: RowId,
    },
    GenerateKeys,
    Login,
    SignEcdsa {
        #[arg(short, long)]
        message: String,
    },
    UpdateUser {
        #[arg(short, long)]
        user: UserPubKey,
        #[arg(long)]
        username: Option<String>,
    },
    AdjustBalance {
        #[arg(short, long)]
        user: UserPubKey,
        #[arg(short, long)]
        amount: Sats,
    },
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
            let rowid = client.new_prediction(request).await?;
            println!("Created new prediction: {}", rowid);
        }
        Commands::AcceptNomination { prediction, judge } => {
            let request = NominationRequest {
                prediction,
                user: judge,
            };
            client
                .accept_nomination(request, get_access().await?)
                .await?;
        }
        Commands::RefuseNomination { prediction, judge } => {
            let request = NominationRequest {
                prediction,
                user: judge,
            };
            client
                .refuse_nomination(request, get_access().await?)
                .await?;
        }
        Commands::MakeDecision {
            prediction,
            judge,
            decision_true,
        } => {
            let request = MakeDecisionRequest {
                prediction,
                judge,
                decision: decision_true,
            };
            client.make_decision(request, get_access().await?).await?;
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
            let request = PredictionUserRequest {
                prediction: Some(request.prediction),
                user: request.user,
            };
            let response = client.get_bets(request, get_access().await?).await?;
            println!("Bets: {:#?}", response);
        }
        Commands::AddBet {
            bet_true,
            amount,
            prediction,
            user,
        } => {
            let request = AddBetRequest {
                bet: bet_true,
                prediction: prediction.into(),
                user,
                amount,
            };
            let access = get_access().await?;
            let payment = client.add_bet(request, access.clone()).await?;
        }
        Commands::CancelBet { id } => {
            client.cancel_bet(id, get_access().await?).await?;
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
            let challenge = client.create_login_challenge(user.clone()).await?;
            let message = Message::from_hashed_data::<Hash>(challenge.as_bytes());
            let signature = secret_key.sign_ecdsa(message);
            let mut file = File::create("access_token").await?;
            file.write_all(signature.to_string().as_bytes()).await?;
            let mut file = File::create("challenge").await?;
            file.write_all(challenge.as_bytes()).await?;
            println!("Signed Challenge \"{}\"", challenge);
            let request = LoginRequest {
                user,
                challenge,
                sig: signature,
            };
            client.try_login(request).await?;
            println!("Logged in as {}", user);
        }
        Commands::SignEcdsa { message } => {
            let message = Message::from_hashed_data::<Hash>(message.as_bytes());
            let secret_key = read_secret().await?;
            let signature = secret_key.sign_ecdsa(message);
            println!("{}", signature);
        }
        Commands::UpdateUser { user, username } => {
            let access = get_access().await?;
            let data = UpdateUserRequest { user, username };
            client.update_user(data, access).await?;
        }
        Commands::AdjustBalance { user, amount } => {
            let access = get_access().await?;
            let data = AdjustBalanceRequest { user, amount };
            client.adjust_balance(data, access).await?;
        }
    }
    Ok(())
}
async fn get_access() -> Result<AccessRequest> {
    let user = read_public().await?;
    let sig = read_token().await?;
    let challenge = read_challenge().await?;
    Ok(AccessRequest {
        user,
        sig,
        challenge,
    })
}
async fn read_secret() -> Result<SecretKey> {
    let mut file = File::open("ecdsa.key").await?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).await?;
    Ok(SecretKey::from_str(String::from_utf8(contents)?.as_str())?)
}
async fn read_public() -> Result<UserPubKey> {
    let mut file = File::open("ecdsa.pub").await?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).await?;
    Ok(UserPubKey::from_str(String::from_utf8(contents)?.as_str())?)
}
async fn read_token() -> Result<Signature> {
    let mut file = File::open("access_token").await?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).await?;
    Ok(Signature::from_str(String::from_utf8(contents)?.as_str())?)
}
async fn read_challenge() -> Result<String> {
    let mut file = File::open("challenge").await?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).await?;
    Ok(String::from_utf8(contents)?)
}
