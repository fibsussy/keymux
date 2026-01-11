#![allow(clippy::pedantic)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use config::Layer;

pub mod config;
mod config_manager;
mod daemon;
mod debug;
mod event_processor;
mod ipc;
mod keyboard_id;
mod keymap;
mod list;
pub mod niri;
mod session_manager;
mod toggle;

use daemon::AsyncDaemon;

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
    #[command(hide = true)]
    Daemon {
        /// Path to config file (default: ~/.config/keyboard-middleware/config.ron)
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

    /// Reload configuration from disk
    Reload,

    /// Validate configuration file for errors
    Validate {
        /// Path to config file (default: ~/.config/keyboard-middleware/config.ron)
        #[arg(short, long)]
        config: Option<std::path::PathBuf>,
    },

    /// Show debugging information
    Debug,

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

            // Use the new async daemon architecture
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;

            runtime.block_on(async {
                let mut daemon = AsyncDaemon::new(config.clone(), user.clone())?;
                daemon.run().await
            })?;
        }
        Some(Commands::NiriDaemon) => {
            run_niri_daemon()?;
        }
        Some(Commands::List) => {
            list::run_list()?;
        }
        Some(Commands::Toggle) => {
            toggle::run_toggle()?;
        }
        Some(Commands::Reload) => {
            run_reload()?;
        }
        Some(Commands::Validate { config }) => {
            validate_config(config.as_deref())?;
        }
        Some(Commands::Debug) => {
            debug::run_debug()?;
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
    println!(
        "  {} {}",
        "keyboard-middleware".bright_white(),
        "[COMMAND]".dimmed()
    );
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
        "keyboard-middleware daemon".bright_white(),
        "Start the daemon".dimmed()
    );
    println!(
        "  {}    {}",
        "keyboard-middleware list".bright_white(),
        "Show all detected keyboards".dimmed()
    );
    println!(
        "  {}  {}",
        "keyboard-middleware toggle".bright_white(),
        "Select keyboards to enable/disable".dimmed()
    );
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

fn validate_config(config_path: Option<&std::path::Path>) -> Result<()> {
    use config::{Action, Config, KeyCode};
    use std::collections::{HashMap, HashSet};

    println!();
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!("  {}", "Config Validation".bright_cyan().bold());
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!();

    // Determine config path
    let config_path = if let Some(path) = config_path {
        path.to_path_buf()
    } else {
        Config::default_path()?
    };

    println!(
        "  {} {}",
        "Config file:".bright_yellow(),
        config_path.display().to_string().dimmed()
    );
    println!();

    // Try to load the config
    print!("  {} Loading config... ", "→".bright_blue());
    let config = match Config::load(&config_path) {
        Ok(cfg) => {
            println!("{}", "✓".bright_green().bold());
            cfg
        }
        Err(e) => {
            println!("{}", "✗".bright_red().bold());
            println!();
            println!("  {} {}", "Error:".bright_red().bold(), e);
            println!();
            return Err(e);
        }
    };

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Validation 1: Check SOCD pairs are symmetric
    print!("  {} Checking SOCD pairs... ", "→".bright_blue());
    let mut socd_map: HashMap<KeyCode, KeyCode> = HashMap::new();

    let mut extract_socd = |remaps: &HashMap<KeyCode, Action>| {
        let mut pairs = Vec::new();
        for (key, action) in remaps {
            if let Action::SOCD(this_key, opposing_keys) = action {
                if key != this_key {
                    errors.push(format!(
                        "SOCD key mismatch: {:?} maps to SOCD({:?}, ...)",
                        key, this_key
                    ));
                }
                // Store each opposing key as a pair for symmetry check
                for opposing_key in opposing_keys {
                    pairs.push((*this_key, *opposing_key));
                }
            }
        }
        pairs
    };

    for (key1, key2) in extract_socd(&config.remaps) {
        socd_map.insert(key1, key2);
    }
    for layer_config in config.layers.values() {
        for (key1, key2) in extract_socd(&layer_config.remaps) {
            socd_map.insert(key1, key2);
        }
    }
    for (key1, key2) in extract_socd(&config.game_mode.remaps) {
        socd_map.insert(key1, key2);
    }

    // Check symmetry
    let mut socd_checked = HashSet::new();
    for (key1, key2) in &socd_map {
        if socd_checked.contains(key1) {
            continue;
        }
        if let Some(reverse) = socd_map.get(key2) {
            if reverse != key1 {
                errors.push(format!(
                    "SOCD pair asymmetric: {:?} → {:?}, but {:?} → {:?}",
                    key1, key2, key2, reverse
                ));
            }
            socd_checked.insert(*key1);
            socd_checked.insert(*key2);
        } else {
            errors.push(format!(
                "SOCD missing reverse pair: {:?} → {:?}, but {:?} not defined",
                key1, key2, key2
            ));
        }
    }

    if errors.is_empty() {
        println!("{} {} pairs", "✓".bright_green().bold(), socd_map.len() / 2);
    } else {
        println!("{}", "✗".bright_red().bold());
    }

    // Validation 2: Check timing values are reasonable
    print!("  {} Checking timing settings... ", "→".bright_blue());
    if config.tapping_term_ms == 0 || config.tapping_term_ms > 1000 {
        errors.push(format!(
            "tapping_term_ms out of reasonable range (0-1000): {}",
            config.tapping_term_ms
        ));
    }
    if let Some(window) = config.double_tap_window_ms {
        if window == 0 || window > 1000 {
            errors.push(format!(
                "double_tap_window_ms out of reasonable range (0-1000): {}",
                window
            ));
        }
    }
    println!("{}", "✓".bright_green().bold());

    // Validation 4: Check layer references
    print!("  {} Checking layer references... ", "→".bright_blue());
    let mut referenced_layers = HashSet::new();

    let extract_layer_refs = |remaps: &HashMap<KeyCode, Action>| {
        let mut refs = Vec::new();
        for action in remaps.values() {
            if let Action::TO(layer) = action {
                refs.push(layer.0.clone());
            }
        }
        refs
    };

    for layer_name in extract_layer_refs(&config.remaps) {
        referenced_layers.insert(layer_name);
    }
    for layer_config in config.layers.values() {
        for layer_name in extract_layer_refs(&layer_config.remaps) {
            referenced_layers.insert(layer_name);
        }
    }

    let mut missing_layers = Vec::new();
    for layer_name in &referenced_layers {
        if layer_name != "base" && !config.layers.contains_key(&Layer(layer_name.clone())) {
            missing_layers.push(layer_name.clone());
        }
    }

    if missing_layers.is_empty() {
        println!(
            "{} {} layers",
            "✓".bright_green().bold(),
            config.layers.len()
        );
    } else {
        println!("{}", "✗".bright_red().bold());
        for layer_name in missing_layers {
            errors.push(format!("Referenced layer not defined: \"{}\"", layer_name));
        }
    }

    // Print summary
    println!();
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );

    if errors.is_empty() && warnings.is_empty() {
        println!(
            "  {} {}",
            "✓".bright_green().bold(),
            "Config is valid!".bright_green()
        );
    } else {
        if !errors.is_empty() {
            println!(
                "  {} {}",
                "✗".bright_red().bold(),
                format!("{} error(s)", errors.len()).bright_red()
            );
            for error in &errors {
                println!("    {} {}", "•".bright_red(), error);
            }
        }
        if !warnings.is_empty() {
            println!(
                "  {} {}",
                "!".bright_yellow().bold(),
                format!("{} warning(s)", warnings.len()).bright_yellow()
            );
            for warning in &warnings {
                println!("    {} {}", "•".bright_yellow(), warning);
            }
        }
    }

    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!();

    if !errors.is_empty() {
        Err(anyhow::anyhow!(
            "Config validation failed with {} error(s)",
            errors.len()
        ))
    } else {
        Ok(())
    }
}

/// Reload configuration from disk via IPC
fn run_reload() -> Result<()> {
    use crate::ipc::{send_request, IpcRequest, IpcResponse};

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

    match send_request(&IpcRequest::Reload) {
        Ok(IpcResponse::Ok) => {
            println!("{}", "✓".bright_green().bold());
            println!();
            println!(
                "  {} {}",
                "✓".bright_green().bold(),
                "Configuration reloaded successfully!".green()
            );
            println!();
        }
        Ok(IpcResponse::Error(msg)) => {
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

/// Niri window watcher daemon that monitors window focus changes
/// and sends game mode updates to the root keyboard-middleware daemon via IPC
fn run_niri_daemon() -> Result<()> {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use tracing::{error, info, warn};

    // Initialize logging
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    info!("Starting keyboard-middleware-niri watcher");

    // Check if automatic game mode detection is enabled
    if !config::GameMode::auto_detect_enabled() {
        error!("Automatic game mode detection is disabled in config");
        error!("Set game_mode.detection_method = \"Auto\" to enable");
        return Ok(());
    }

    // Check if niri is available
    if !niri::is_niri_available() {
        error!("Niri socket not found - is Niri running?");
        error!("This daemon requires Niri window manager");
        return Ok(());
    }

    info!("Niri detected, starting window focus monitor");

    // Create channel for niri events
    let (niri_tx, niri_rx) = mpsc::channel();

    // Start niri monitor
    niri::start_niri_monitor(niri_tx);

    // Track current game mode state to avoid sending redundant IPC requests
    let mut current_game_mode = false;

    loop {
        match niri_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(niri::NiriEvent::WindowFocusChanged(window_info)) => {
                // Determine if game mode should be active
                let should_enable = niri::should_enable_gamemode(&window_info);

                // Only send IPC if state changed
                if should_enable != current_game_mode {
                    current_game_mode = should_enable;
                    info!(
                        "Game mode state changed: {}",
                        if should_enable { "ENABLED" } else { "DISABLED" }
                    );

                    // Send IPC request to root daemon
                    match ipc::send_request(&ipc::IpcRequest::SetGameMode(should_enable)) {
                        Ok(ipc::IpcResponse::Ok) => {
                            info!("Successfully sent game mode update to daemon");
                        }
                        Ok(other) => {
                            warn!("Unexpected response from daemon: {:?}", other);
                        }
                        Err(e) => {
                            error!("Failed to send game mode update to daemon: {}", e);
                            error!("Is keyboard-middleware daemon running?");
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No event, continue
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                error!("Niri monitor died, exiting");
                break;
            }
        }

        // Brief sleep to avoid busy-waiting
        thread::sleep(Duration::from_millis(50));
    }

    info!("Niri watcher stopped");
    Ok(())
}
