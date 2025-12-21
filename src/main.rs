use anyhow::Result;
use clap::{Parser, Subcommand};

mod daemon;
mod keyboard_id;
pub mod config;
pub mod niri;
mod toggle;
mod event_processor;

use daemon::Daemon;

#[derive(Parser)]
#[command(name = "keyboard-middleware")]
#[command(about = "QMK-inspired keyboard middleware for Linux", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Toggle keyboard enable/disable state
    Toggle,
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Toggle) => {
            toggle::run_toggle()?;
        }
        None => {
            // Default: run daemon
            let mut daemon = Daemon::new()?;
            daemon.run()?;
        }
    }

    Ok(())
}
