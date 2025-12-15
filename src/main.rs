use anyhow::Result;
use clap::Parser;

mod cli;
mod config;
mod daemon;
mod ipc;
mod keyboard_id;
mod keyboard_state;
mod keyboard_thread;
mod niri;
mod process_event_new;
mod socd;
mod uinput;

use cli::{Cli, Commands};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .init();

    let cli = Cli::parse();

    // Hidden --daemon flag for systemd service
    if cli.daemon {
        return cli::handle_start();
    }

    // Normal subcommand handling
    match cli.command {
        Some(Commands::Start) => cli::handle_start(),
        Some(Commands::Stop) => cli::handle_stop(),
        Some(Commands::Status) => cli::handle_status(),
        Some(Commands::List) => cli::handle_list(),
        Some(Commands::Toggle) => cli::handle_toggle(),
        Some(Commands::SetPassword) => cli::handle_set_password(),
        None => {
            eprintln!("No command specified. Use --help for usage information.");
            std::process::exit(1);
        }
    }
}
