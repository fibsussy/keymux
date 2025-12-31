#![allow(clippy::pedantic)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;

mod daemon;
mod keyboard_id;
pub mod config;
pub mod niri;
mod toggle;
mod list;
mod event_processor;
mod keymap;
mod ipc;

use daemon::Daemon;

#[derive(Parser)]
#[command(name = "keyboard-middleware")]
#[command(about = "QMK-inspired keyboard middleware for Linux", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the keyboard middleware daemon
    Daemon {
        /// Path to config file (default: ~/.config/keyboard-middleware/config.ron)
        #[arg(short, long)]
        config: Option<std::path::PathBuf>,

        /// User to run as (for root execution, uses that user's config)
        #[arg(short, long)]
        user: Option<String>,
    },

    /// List all detected keyboards
    List,

    /// Toggle keyboard enable/disable state
    Toggle,

    /// Set or update the password for typing
    SetPassword,

    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Daemon { config, user }) => {
            // Initialize tracing for daemon
            tracing_subscriber::fmt()
                .with_target(false)
                .with_thread_ids(false)
                .with_file(false)
                .init();

            let mut daemon = Daemon::new(config.clone(), user.clone())?;
            daemon.run()?;
        }
        Some(Commands::List) => {
            list::run_list()?;
        }
        Some(Commands::Toggle) => {
            toggle::run_toggle()?;
        }
        Some(Commands::SetPassword) => {
            set_password()?;
        }
        Some(Commands::Completion { shell }) => {
            generate_completion(*shell);
        }
        None => {
            // Print help when no command is given
            print_help();
        }
    }

    Ok(())
}

fn print_help() {
    println!("{}", "Keyboard Middleware".bright_cyan().bold());
    println!("{}", "QMK-inspired keyboard remapping for Linux".dimmed());
    println!();
    println!("{}", "USAGE:".bright_yellow().bold());
    println!("  {} {}", "keyboard-middleware".bright_white(), "[COMMAND]".dimmed());
    println!();
    println!("{}", "COMMANDS:".bright_yellow().bold());
    println!("  {}  {}", "daemon".bright_green().bold(), "Run the keyboard middleware daemon".dimmed());
    println!("  {}    {}", "list".bright_green().bold(), "List all detected keyboards".dimmed());
    println!("  {}  {}", "toggle".bright_green().bold(), "Toggle keyboard enable/disable state".dimmed());
    println!("  {}  {}", "set-password".bright_green().bold(), "Set or update the password for typing".dimmed());
    println!("  {}    {}", "help".bright_green().bold(), "Print this message".dimmed());
    println!();
    println!("{}", "OPTIONS:".bright_yellow().bold());
    println!("  {}  {}", "-h, --help".bright_white(), "Print help".dimmed());
    println!("  {}  {}", "-V, --version".bright_white(), "Print version".dimmed());
    println!();
    println!("{}", "EXAMPLES:".bright_yellow().bold());
    println!("  {}  {}", "keyboard-middleware daemon".bright_white(), "Start the daemon".dimmed());
    println!("  {}    {}", "keyboard-middleware list".bright_white(), "Show all detected keyboards".dimmed());
    println!("  {}  {}", "keyboard-middleware toggle".bright_white(), "Select keyboards to enable/disable".dimmed());
    println!();
}

fn generate_completion(shell: clap_complete::Shell) {
    use clap::CommandFactory;
    use clap_complete::generate;
    use std::io;

    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut io::stdout());
}

fn set_password() -> Result<()> {
    use dialoguer::{Password, Confirm};
    use config::Passwords;

    println!();
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!("  {}", "Password Configuration".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!();
    println!("{}", "  Set a password that can be typed with a dedicated key.".dimmed());
    println!("{}", "  Configure the key in your config.ron file using Action::Password".dimmed());
    println!();

    // Get password file path
    let password_path = Passwords::default_path()?;

    // Show current password state
    let current_password = Passwords::load(&password_path)?;
    if current_password.is_some() {
        println!("  {} {}", "Current:".bright_yellow(), "Password is set".green());
        println!();

        let clear = Confirm::new()
            .with_prompt("  Clear existing password?")
            .default(false)
            .interact()?;

        if clear {
            if password_path.exists() {
                std::fs::remove_file(&password_path)?;
            }
            println!();
            println!("  {} {}", "✓".bright_green().bold(), "Password cleared".green());
            println!();
            return Ok(());
        }
    } else {
        println!("  {} {}", "Current:".bright_yellow(), "No password set".dimmed());
        println!();
    }

    // Get password input
    let password = Password::new()
        .with_prompt("  Enter password")
        .with_confirmation("  Confirm password", "  Passwords don't match, try again")
        .interact()?;

    if password.is_empty() {
        println!();
        println!("  {} {}", "✗".bright_red().bold(), "Password cannot be empty".red());
        println!();
        return Ok(());
    }

    // Ensure config directory exists
    if let Some(parent) = password_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Save to password file (plain text)
    std::fs::write(&password_path, password)?;

    println!();
    println!("  {} {}", "✓".bright_green().bold(), "Password saved successfully!".green());
    println!();
    println!("  {} Edit your config.ron to assign a key to Action::Password", "Tip:".bright_yellow().bold());
    println!();

    Ok(())
}
