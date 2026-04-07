mod format;
mod probe;

use clap::Parser;

#[derive(Parser)]
#[command(name = "glimpsectl", about = "Probe glimpse providers")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Run a provider and stream its events to stdout
    Probe {
        /// Provider to probe (e.g. battery)
        provider: String,

        /// Output compact JSONL instead of colored pretty JSON
        #[arg(long, short)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Probe { provider, json } => {
            probe::run(&provider, !json, !json).await
        }
    }
}
