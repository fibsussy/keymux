#![allow(clippy::pedantic)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use config::Layer;
use std::path::PathBuf;

/// Get the actual user UID, respecting SUDO context
/// Returns (uid, is_sudo) where is_sudo indicates if running under sudo
pub fn get_actual_user_uid() -> (u32, bool) {
    // Check if running under sudo
    if let Ok(sudo_uid) = std::env::var("SUDO_UID") {
        if let Ok(uid) = sudo_uid.parse::<u32>() {
            return (uid, true);
        }
    }

    // Fall back to current effective UID
    (unsafe { libc::getuid() }, false)
}

/// Get user's home directory from UID using getent
/// Works even when running as root/sudo
pub fn get_user_home_dir(uid: u32) -> anyhow::Result<PathBuf> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("getent passwd {} | cut -d: -f6", uid))
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to get home directory for UID {}",
            uid
        ));
    }

    let home = String::from_utf8(output.stdout)?.trim().to_string();

    if home.is_empty() {
        return Err(anyhow::anyhow!("Empty home directory for UID {}", uid));
    }

    Ok(PathBuf::from(home))
}

pub mod config;
mod config_manager;
mod daemon;
mod debug;
mod doubletap;
mod event_processor;
mod ipc;
mod keyboard_id;
mod keymap;
mod list;
mod modtap;
mod niri;
mod oneshot;
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

    /// Show adaptive timing statistics
    AdaptiveStats {
        /// Path to config file (default: ~/.config/keyboard-middleware/config.ron)
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
        Some(Commands::AdaptiveStats { config }) => {
            show_adaptive_stats(config.as_deref())?;
        }
        Some(Commands::ClearStats) => {
            clear_adaptive_stats()?;
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

fn clear_adaptive_stats() -> Result<()> {
    use colored::Colorize;
    use std::io::{self, Write};

    println!();
    println!(
        "{}",
        "⚠ WARNING: This will delete ALL adaptive timing statistics!"
            .bright_red()
            .bold()
    );
    println!();
    print!("  Are you REALLY sure? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input != "y" && input != "yes" {
        println!();
        println!("  {} Cancelled. No stats were deleted.", "✓".bright_green());
        println!();
        return Ok(());
    }

    let (uid, _) = keyboard_middleware::get_actual_user_uid();
    let home =
        keyboard_middleware::get_user_home_dir(uid).expect("Failed to get user home directory");
    let config_dir = home.join(".config").join("keyboard-middleware");

    let mt_stats = config_dir.join("adaptive_stats.json");
    let all_stats = config_dir.join("all_key_stats.json");

    let mut deleted = 0;
    if mt_stats.exists() {
        std::fs::remove_file(&mt_stats)?;
        deleted += 1;
    }
    if all_stats.exists() {
        std::fs::remove_file(&all_stats)?;
        deleted += 1;
    }

    println!();
    if deleted > 0 {
        println!(
            "  {} Deleted {} stats file(s).",
            "✓".bright_green(),
            deleted
        );
    } else {
        println!("  {} No stats files found.", "ℹ".bright_blue());
    }
    println!();

    Ok(())
}

fn show_adaptive_stats(config_path: Option<&std::path::Path>) -> Result<()> {
    use colored::Colorize;
    use config::{Config, KeyCode};
    use modtap::{MtConfig as ModtapConfig, MtProcessor};

    println!();
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!("  {}", "Adaptive Timing Statistics".bright_cyan().bold());
    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!();

    // Load config
    let config_path = config_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        let (uid, _) = keyboard_middleware::get_actual_user_uid();
        let home =
            keyboard_middleware::get_user_home_dir(uid).expect("Failed to get user home directory");
        home.join(".config")
            .join("keyboard-middleware")
            .join("config.ron")
    });

    print!("  → Loading config... ");
    let config = Config::load(&config_path)?;
    println!("{}", "✓".bright_green());

    // Trigger daemon to save stats first (so we get latest data)
    print!("  → Requesting fresh stats from daemon... ");
    match ipc::send_request(&ipc::IpcRequest::SaveAdaptiveStats) {
        Ok(ipc::IpcResponse::Ok) => {
            println!("{}", "✓".bright_green());
            // Give daemon a moment to save
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(_) => {
            println!("{}", "⚠ unexpected response".bright_yellow());
        }
        Err(_) => {
            println!(
                "{}",
                "⚠ daemon not running (showing cached data)".bright_yellow()
            );
        }
    }

    if !config.mt_config.adaptive_timing {
        println!();
        println!(
            "  {} Adaptive timing is disabled in config",
            "!".bright_yellow()
        );
        println!("  Enable it with: mt_config: ( adaptive_timing: true, ... )");
        println!();
        return Ok(());
    }

    // Load ALL key stats from disk
    let all_stats_path = config_path.parent().unwrap().join("all_key_stats.json");
    let stats = if all_stats_path.exists() {
        let json = std::fs::read_to_string(&all_stats_path).unwrap_or_default();
        let stats_map: std::collections::HashMap<String, modtap::RollingStats> =
            serde_json::from_str(&json).unwrap_or_default();

        let mut result = Vec::new();
        for (key_str, stats) in stats_map {
            let key_json = format!("\"KC_{}\"", key_str);
            if let Ok(keycode) = serde_json::from_str::<KeyCode>(&key_json) {
                result.push((keycode, stats));
            }
        }
        result
    } else {
        Vec::new()
    };

    if stats.is_empty() {
        println!();
        println!(
            "  {} No adaptive statistics collected yet",
            "ℹ".bright_blue()
        );
        println!("  Start typing to build statistics!");
        println!();
        return Ok(());
    }

    println!();
    println!(
        "  Base: {}ms  │  Margin: {}ms",
        config.tapping_term_ms.to_string().bright_yellow(),
        config
            .mt_config
            .adaptive_target_margin_ms
            .to_string()
            .bright_yellow()
    );
    println!();

    // Sort stats by key name
    let mut sorted_stats = stats;
    sorted_stats.sort_by_key(|(k, _)| format!("{:?}", k));

    // Compact table header
    println!("  ┌───────┬────────┬─────────┬──────────┐");
    println!(
        "  │ {:^5} │ {:^6} │ {:^7} │ {:^8} │",
        "Key".bright_white().bold(),
        "Samples".bright_white().bold(),
        "Avg(ms)".bright_white().bold(),
        "Thresh(ms)".bright_white().bold()
    );
    println!("  ├───────┼────────┼─────────┼──────────┤");

    // Table rows
    for (keycode, key_stats) in sorted_stats {
        let key_name = format!("{:?}", keycode).replace("KC_", "");
        let samples = key_stats.tap_sample_count;
        let avg_tap = key_stats.avg_tap_duration;
        let threshold = key_stats.adaptive_threshold;

        println!(
            "  │ {:^5} │ {:^6} │ {:^7} │ {:^8} │",
            key_name.bright_cyan(),
            samples.to_string().bright_green(),
            format!("{:.1}", avg_tap).bright_blue(),
            format!("{:.1}", threshold).bright_yellow()
        );
    }

    println!("  └───────┴────────┴─────────┴──────────┘");

    println!(
        "{}",
        "═══════════════════════════════════════".bright_cyan()
    );
    println!();

    Ok(())
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
            if let Action::SOCD(this_action, opposing_actions) = action {
                // Extract KeyCode from Action (only validate Key actions)
                if let Action::Key(this_key) = this_action.as_ref() {
                    if key != this_key {
                        warnings.push(format!(
                            "⚠️  SOCD key mismatch: {:?} maps to SOCD({:?}, ...)",
                            key, this_key
                        ));
                    }
                    // Store each opposing key as a pair for symmetry check
                    for opposing_action in opposing_actions {
                        if let Action::Key(opposing_key) = opposing_action.as_ref() {
                            pairs.push((*this_key, *opposing_key));
                        }
                    }
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
    // Validate MT config timing
    let window = config.mt_config.double_tap_window_ms;
    if window == 0 || window > 1000 {
        errors.push(format!(
            "mt_config.double_tap_window_ms out of reasonable range (0-1000): {}",
            window
        ));
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
