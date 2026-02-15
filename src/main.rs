#![allow(clippy::pedantic)]

use anyhow::Result;
use clap::Parser;

mod adaptive_stats;
mod cli;
mod gamemode;

mod debug;
mod keyboard_id;
pub mod keycode;
mod list;
mod session_manager;
mod toggle;

pub use keymux::{get_actual_user_uid, get_user_home_dir};

use cli::Cli;
use keymux::daemon::AsyncDaemon;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(cli::Commands::Daemon { config, user }) => {
            tracing_subscriber::fmt()
                .with_target(false)
                .with_thread_ids(false)
                .with_file(false)
                .init();

            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;

            runtime.block_on(async {
                let mut daemon = AsyncDaemon::new(config.clone(), user.clone())?;
                daemon.run().await
            })?;
        }
        Some(cli::Commands::NiriDaemon) => {
            keymux::niri::run_niri_daemon()?;
        }
        Some(cli::Commands::List) => {
            list::run_list()?;
        }
        Some(cli::Commands::Toggle) => {
            toggle::run_toggle()?;
        }
        Some(cli::Commands::Gamemode { action }) => {
            gamemode::handle_gamemode_action(action)?;
        }
        Some(cli::Commands::Reload) => {
            run_reload()?;
        }
        Some(cli::Commands::Validate { config }) => {
            keymux::config::validate_config(config.as_deref())?;
        }
        Some(cli::Commands::Debug) => {
            debug::run_debug()?;
        }
        Some(cli::Commands::AdaptiveStats { config }) => {
            adaptive_stats::show_adaptive_stats(config.as_deref())?;
        }
        Some(cli::Commands::ClearStats) => {
            adaptive_stats::clear_adaptive_stats()?;
        }
        Some(cli::Commands::Completion { shell }) => {
            cli::generate_completion(*shell);
        }
        None => {
            cli::print_help();
        }
    }

    Ok(())
}

fn run_reload() -> Result<()> {
    use colored::Colorize;

    println!();
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!("  {}", "Reload Configuration".bright_cyan().bold());
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!();

    print!("  {} Sending reload request... ", "→".bright_blue());

    match keymux::ipc::send_request(&keymux::ipc::IpcRequest::Reload) {
        Ok(keymux::ipc::IpcResponse::Ok) => {
            println!("{}", "✓".bright_green().bold());
            println!();
            println!(
                "  {} {}",
                "✓".bright_green().bold(),
                "Configuration reloaded successfully!".green()
            );
            println!();
        }
        Ok(keymux::ipc::IpcResponse::Error(msg)) => {
            println!("{}", "✗".bright_red().bold());
            println!();
            println!("  {} {}", "✗".bright_red().bold(), msg.red());
            println!();
            anyhow::bail!("Config reload failed");
        }
        Ok(response) => {
            println!("{}", "✗".bright_red().bold());
            println!();
            println!(
                "  {} Unexpected response: {:?}",
                "✗".bright_red().bold(),
                response
            );
            println!();
            anyhow::bail!("Unexpected response from daemon");
        }
        Err(e) => {
            println!("{}", "✗".bright_red().bold());
            println!();
            println!(
                "  {} {}",
                "✗".bright_red().bold(),
                format!("Failed to connect to daemon: {}", e).red()
            );
            println!();
            println!(
                "  {} {}",
                "Tip:".bright_yellow().bold(),
                "Make sure the daemon is running (usually via systemd)".dimmed()
            );
            println!();
            anyhow::bail!("Failed to reload configuration");
        }
    }

    Ok(())
}
