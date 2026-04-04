mod cli;
mod format;
mod picker;
mod tui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "glimpsectl", about = "Control and debug glimpsed daemon")]
struct Args {
    /// Colorize JSON output (implies --pretty)
    #[arg(long, short)]
    color: bool,

    /// Pretty-print JSON with indentation
    #[arg(long, short)]
    pretty: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// One-shot read of a topic
    Get {
        /// Topic to read (e.g. "battery.status")
        topic: String,
    },
    /// Subscribe to topics and print events
    #[command(name = "sub")]
    Subscribe {
        /// Topic patterns (e.g. "battery.**" "audio.**")
        #[arg(required = true)]
        patterns: Vec<String>,
    },
    /// Call a provider method
    Call {
        /// Method name (e.g. "debug.echo")
        method: String,
        /// JSON params (default: {})
        #[arg(default_value = "{}")]
        params: String,
    },
    /// Interactive TUI mode
    Tui,
    /// List registered providers
    Inspect {
        /// Filter by provider name (can be repeated)
        #[arg(long = "provider", short = 'P')]
        providers: Vec<String>,
        /// Show only topics
        #[arg(long)]
        topics_only: bool,
        /// Show only methods
        #[arg(long)]
        methods_only: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let color = args.color;
    let pretty = args.pretty || args.color;

    match args.command {
        Command::Get { topic } => {
            cli::cmd_get(topic, color, pretty).await?;
        }
        Command::Subscribe { patterns } => {
            cli::cmd_subscribe(patterns, color, pretty).await?;
        }
        Command::Call { method, params } => {
            let params: serde_json::Value = serde_json::from_str(&params)
                .map_err(|e| anyhow::anyhow!("invalid JSON params: {e}\nhint: use single quotes: glimpsectl call {method} '{{\"key\": \"value\"}}'" ))?;
            cli::cmd_call(method, params, color, pretty).await?;
        }
        Command::Tui => {
            tui::run_tui().await?;
        }
        Command::Inspect {
            providers,
            topics_only,
            methods_only,
        } => {
            cli::cmd_inspect(providers, topics_only, methods_only).await?;
        }
    }

    Ok(())
}
