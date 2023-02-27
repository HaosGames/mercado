use anyhow::Result;
use clap::{Parser, Subcommand};


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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Args::parse();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        Commands::NewPrediction{ prediction, judge, share_ppm, end, decision_period_sec, judge_count } => {
            println!("'myapp add' was used, name")
        }
    }
    Ok(())
}
