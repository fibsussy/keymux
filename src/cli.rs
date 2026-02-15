use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum GamemodeAction {
    /// Control game mode for currently focused window
    Window {
        #[command(subcommand)]
        action: WindowGamemodeAction,
    },
}

#[derive(Subcommand)]
pub enum WindowGamemodeAction {
    /// Invert game mode state for this window
    Invert,
    /// Toggle between invert and normal for this window
    ToggleInvert,
    /// Use normal automatic detection for this window
    Normal,
    /// Always force game mode on for this window
    AlwaysOn,
    /// Always force game mode off for this window
    AlwaysOff,
    /// List all window-specific overrides
    List,
}

#[derive(Parser)]
#[command(name = "keymux")]
#[command(about = "QMK-inspired keyboard middleware for Linux", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the keyboard middleware daemon
    #[command(hide = true)]
    Daemon {
        /// Path to config file (default: ~/.config/keymux/config.ron)
        #[arg(short, long)]
        config: Option<std::path::PathBuf>,

        /// User to run as (for root execution, uses that user's config)
        #[arg(short, long)]
        user: Option<String>,
    },

    /// Run the niri window watcher daemon
    #[command(hide = true)]
    NiriDaemon,

    /// List all detected keyboards
    List,

    /// Toggle keyboard enable/disable state
    Toggle,

    /// Control game mode settings
    Gamemode {
        #[command(subcommand)]
        action: GamemodeAction,
    },

    /// Reload configuration from disk
    Reload,

    /// Validate configuration file for errors
    Validate {
        /// Path to config file (default: ~/.config/keymux/config.ron)
        #[arg(short, long)]
        config: Option<std::path::PathBuf>,
    },

    /// Show debugging information
    Debug,

    /// Show adaptive timing statistics
    AdaptiveStats {
        /// Path to config file (default: ~/.config/keymux/config.ron)
        #[arg(short, long)]
        config: Option<std::path::PathBuf>,
    },

    /// Clear all adaptive timing statistics
    ClearStats,

    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

pub fn print_help() {
    use colored::Colorize;

    println!("{}", "Keyboard Middleware".bright_cyan().bold());
    println!("{}", "QMK-inspired keyboard remapping for Linux".dimmed());
    println!();
    println!("{}", "USAGE:".bright_yellow().bold());
    println!("  {} {}", "keymux".bright_white(), "[COMMAND]".dimmed());
    println!();
    println!("{}", "COMMANDS:".bright_yellow().bold());
    println!(
        "  {}  {}",
        "daemon".bright_green().bold(),
        "Run the keyboard middleware daemon".dimmed()
    );
    println!(
        "  {}    {}",
        "list".bright_green().bold(),
        "List all detected keyboards".dimmed()
    );
    println!(
        "  {}  {}",
        "toggle".bright_green().bold(),
        "Toggle keyboard enable/disable state".dimmed()
    );
    println!(
        "  {}  {}",
        "reload".bright_green().bold(),
        "Reload configuration from disk".dimmed()
    );
    println!(
        "  {}  {}",
        "validate".bright_green().bold(),
        "Validate configuration file".dimmed()
    );
    println!(
        "  {}    {}",
        "help".bright_green().bold(),
        "Print this message".dimmed()
    );
    println!();
    println!("{}", "OPTIONS:".bright_yellow().bold());
    println!(
        "  {}  {}",
        "-h, --help".bright_white(),
        "Print help".dimmed()
    );
    println!(
        "  {}  {}",
        "-V, --version".bright_white(),
        "Print version".dimmed()
    );
    println!();
    println!("{}", "EXAMPLES:".bright_yellow().bold());
    println!(
        "  {}  {}",
        "keymux daemon".bright_white(),
        "Start the daemon".dimmed()
    );
    println!(
        "  {}    {}",
        "keymux list".bright_white(),
        "Show all detected keyboards".dimmed()
    );
    println!(
        "  {}  {}",
        "keymux toggle".bright_white(),
        "Select keyboards to enable/disable".dimmed()
    );
    println!();
}

pub fn generate_completion(shell: clap_complete::Shell) {
    use clap::CommandFactory;
    use clap_complete::generate;
    use std::io;

    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut io::stdout());
}
