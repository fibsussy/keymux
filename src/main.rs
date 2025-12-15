use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::io::Write;
use std::time::Instant;

use evdev::Key;

mod config;
mod daemon;
mod ipc;
mod keyboard_id;
mod keyboard_thread;
mod niri;
mod process_event_new;
mod socd;
mod uinput;

#[cfg(test)]
mod tests;

use socd::SocdCleaner;
use uinput::VirtualKeyboard;

const TAPPING_TERM_MS: u64 = 130;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
enum Layer {
    Base,
    HomeRowMod,
    Nav,
    Game,
    Fn,
}

/// What a specific key press is doing (recorded when pressed, replayed on release)
#[derive(Debug, Clone)]
enum Action {
    /// This key press activated a modifier
    Modifier(Key),
    /// This key press emitted a regular key
    RegularKey(Key),
    /// This key is being handled by SOCD cleaner (don't release manually)
    SocdManaged,
    /// This key activated nav layer
    NavLayerActivation,
    /// Home row mod waiting for decision (tap or hold)
    HomeRowModPending { hrm_key: Key, press_time: Instant },
}

/// What a physical key is currently doing
#[derive(Debug, Clone)]
struct KeyAction {
    actions: Vec<Action>,
}

impl KeyAction {
    fn new() -> Self {
        Self {
            actions: Vec::with_capacity(2), // Most keys do 1-2 things
        }
    }

    fn add(&mut self, action: Action) {
        self.actions.push(action);
    }
}

/// Reference counting for modifiers (fast array-based lookup)
#[derive(Debug, Clone)]
struct ModifierState {
    counts: [u8; 8], // Fixed size array for speed
}

impl ModifierState {
    fn new() -> Self {
        Self { counts: [0; 8] }
    }

    fn modifier_index(key: Key) -> Option<usize> {
        match key {
            Key::KEY_LEFTSHIFT => Some(0),
            Key::KEY_RIGHTSHIFT => Some(1),
            Key::KEY_LEFTCTRL => Some(2),
            Key::KEY_RIGHTCTRL => Some(3),
            Key::KEY_LEFTALT => Some(4),
            Key::KEY_RIGHTALT => Some(5),
            Key::KEY_LEFTMETA => Some(6),
            Key::KEY_RIGHTMETA => Some(7),
            _ => None,
        }
    }

    fn increment(&mut self, key: Key) {
        if let Some(idx) = Self::modifier_index(key) {
            self.counts[idx] = self.counts[idx].saturating_add(1);
        }
    }

    fn decrement(&mut self, key: Key) -> bool {
        if let Some(idx) = Self::modifier_index(key) {
            if self.counts[idx] > 0 {
                self.counts[idx] -= 1;
                return self.counts[idx] == 0; // Return true if we should release
            }
        }
        true // If not tracked, release immediately
    }
}

#[derive(Debug, Clone)]
struct HomeRowMod {
    key: Key,
    modifier: Key,
    base_key: Key,
}

struct KeyboardState {
    layers: Vec<Layer>,
    home_row_mods: HashMap<Key, HomeRowMod>,
    socd_cleaner: SocdCleaner,
    game_mode: bool,

    // Fast lookup for what each physical key is doing
    held_keys: HashMap<Key, KeyAction>,
    // Reference counting for modifiers
    modifier_state: ModifierState,
    // Nav layer tracking
    nav_layer_active: bool,
    // Password typer state (resets when leaving nav layer)
    password_typed_in_nav: bool,
    // Password from config
    password: Option<String>,
}

impl KeyboardState {
    fn new(password: Option<String>) -> Self {
        Self {
            layers: vec![Layer::Base, Layer::HomeRowMod],
            home_row_mods: Self::init_home_row_mods(),
            socd_cleaner: SocdCleaner::new(),
            game_mode: false,
            held_keys: HashMap::new(),
            modifier_state: ModifierState::new(),
            nav_layer_active: false,
            password_typed_in_nav: false,
            password,
        }
    }

    fn init_home_row_mods() -> HashMap<Key, HomeRowMod> {
        let mut mods = HashMap::new();

        // Left hand home row mods
        mods.insert(
            Key::KEY_A,
            HomeRowMod {
                key: Key::KEY_A,
                modifier: Key::KEY_LEFTMETA,
                base_key: Key::KEY_A,
            },
        );
        mods.insert(
            Key::KEY_S,
            HomeRowMod {
                key: Key::KEY_S,
                modifier: Key::KEY_LEFTALT,
                base_key: Key::KEY_S,
            },
        );
        mods.insert(
            Key::KEY_D,
            HomeRowMod {
                key: Key::KEY_D,
                modifier: Key::KEY_LEFTCTRL,
                base_key: Key::KEY_D,
            },
        );
        mods.insert(
            Key::KEY_F,
            HomeRowMod {
                key: Key::KEY_F,
                modifier: Key::KEY_LEFTSHIFT,
                base_key: Key::KEY_F,
            },
        );

        // Right hand home row mods
        mods.insert(
            Key::KEY_J,
            HomeRowMod {
                key: Key::KEY_J,
                modifier: Key::KEY_RIGHTSHIFT,
                base_key: Key::KEY_J,
            },
        );
        mods.insert(
            Key::KEY_K,
            HomeRowMod {
                key: Key::KEY_K,
                modifier: Key::KEY_RIGHTCTRL,
                base_key: Key::KEY_K,
            },
        );
        mods.insert(
            Key::KEY_L,
            HomeRowMod {
                key: Key::KEY_L,
                modifier: Key::KEY_RIGHTALT,
                base_key: Key::KEY_L,
            },
        );
        mods.insert(
            Key::KEY_SEMICOLON,
            HomeRowMod {
                key: Key::KEY_SEMICOLON,
                modifier: Key::KEY_RIGHTMETA,
                base_key: Key::KEY_SEMICOLON,
            },
        );

        mods
    }

    const fn is_wasd_key(key: Key) -> bool {
        matches!(key, Key::KEY_W | Key::KEY_A | Key::KEY_S | Key::KEY_D)
    }
}

#[derive(Parser)]
#[command(name = "keyboard-middleware")]
#[command(about = "Multi-keyboard middleware with home row mods and game mode")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the keyboard middleware daemon (default)
    Daemon,
    /// Set password for nav+backspace password typer
    SetPassword,
    /// Check if daemon is running
    Ping,
    /// List all keyboards with their enabled/disabled status
    ListKeyboards,
    /// Interactively toggle which keyboards are enabled
    ToggleKeyboards,
    /// Shutdown the daemon
    Shutdown,
}

fn get_config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .map(|p| p.join("keyboard-middleware").join("config.toml"))
        .unwrap_or_else(|| std::path::PathBuf::from("config.toml"))
}

fn handle_set_password() -> Result<()> {
    use dialoguer::Password;

    println!("Setting password for nav+backspace password typer\n");

    let password = Password::new()
        .with_prompt("Enter password")
        .with_confirmation("Confirm password", "Passwords don't match")
        .interact()?;

    // Load existing config or create default
    let config_path = get_config_path();
    let mut config = config::Config::load_or_default(&config_path);

    // Update password
    config.password = Some(password);

    // Ensure directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Save config
    config.save(&config_path)?;

    println!("\n✓ Password saved to {}", config_path.display());
    println!("Use nav+backspace to type your password");

    Ok(())
}

fn handle_ping() -> Result<()> {
    let response = ipc::send_request(&ipc::IpcRequest::Ping)?;
    match response {
        ipc::IpcResponse::Pong => {
            println!("✓ Daemon is running");
            Ok(())
        }
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

fn handle_list_keyboards() -> Result<()> {
    let response = ipc::send_request(&ipc::IpcRequest::ListKeyboards)?;
    match response {
        ipc::IpcResponse::KeyboardList(keyboards) => {
            if keyboards.is_empty() {
                println!("No keyboards detected");
                return Ok(());
            }

            println!("\n\x1b[1mDetected keyboards:\x1b[0m\n");
            for (i, kb) in keyboards.iter().enumerate() {
                let (status_icon, status_text, status_color) = if !kb.connected {
                    ("○", "disconnected", "\x1b[90m")
                } else if kb.enabled {
                    ("●", "enabled", "\x1b[32m")
                } else {
                    ("○", "disabled", "\x1b[90m")
                };

                println!("\x1b[1m{}\x1b[0m. {}{}\x1b[0m {} \x1b[90m[{}]\x1b[0m",
                    i + 1,
                    status_color,
                    status_icon,
                    kb.name,
                    status_text
                );
                println!("   \x1b[90mID: {}\x1b[0m", kb.hardware_id);
                if kb.connected {
                    println!("   \x1b[90mPath: {}\x1b[0m", kb.device_path);
                }
                println!();
            }

            Ok(())
        }
        ipc::IpcResponse::Error(e) => anyhow::bail!("Error: {}", e),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

fn handle_toggle_keyboards() -> Result<()> {
    // Get current keyboard list
    let response = ipc::send_request(&ipc::IpcRequest::ToggleKeyboards)?;
    let keyboards = match response {
        ipc::IpcResponse::KeyboardList(kb) => kb,
        ipc::IpcResponse::Error(e) => anyhow::bail!("Error: {}", e),
        _ => anyhow::bail!("Unexpected response from daemon"),
    };

    if keyboards.is_empty() {
        println!("No keyboards detected");
        return Ok(());
    }

    // Show current state
    println!("\n\x1b[1m━━━ Keyboard Configuration ━━━\x1b[0m\n");
    println!("Current state:");
    for kb in &keyboards {
        let status_color = if kb.enabled { "\x1b[32m" } else { "\x1b[90m" };
        let status_icon = if kb.enabled { "●" } else { "○" };
        println!("  {} {} {}\x1b[0m", status_color, status_icon, kb.name);
    }

    // Show interactive multi-select
    println!("\n\x1b[1mSelect keyboards to enable\x1b[0m (Space=toggle, Enter=confirm):\n");

    let items: Vec<String> = keyboards
        .iter()
        .map(|kb| format!("  {}", kb.name))
        .collect();

    let defaults: Vec<bool> = keyboards.iter().map(|kb| kb.enabled).collect();

    let selections = dialoguer::MultiSelect::new()
        .items(&items)
        .defaults(&defaults)
        .interact()?;

    // Apply changes
    println!(); // Blank line before changes
    let mut changes_made = false;

    for (i, kb) in keyboards.iter().enumerate() {
        let should_enable = selections.contains(&i);
        let currently_enabled = kb.enabled;

        if should_enable && !currently_enabled {
            // Enabling a keyboard
            print!("  \x1b[32m●\x1b[0m Enabling {}... ", kb.name);
            std::io::Write::flush(&mut std::io::stdout())?;
            let response = ipc::send_request(&ipc::IpcRequest::EnableKeyboard(kb.hardware_id.clone()))?;
            if let ipc::IpcResponse::Error(e) = response {
                println!("\x1b[31m✗ Failed: {}\x1b[0m", e);
            } else {
                println!("\x1b[32m✓\x1b[0m");
                changes_made = true;
            }
        } else if !should_enable && currently_enabled {
            // Disabling a keyboard
            print!("  \x1b[90m○\x1b[0m Disabling {}... ", kb.name);
            std::io::Write::flush(&mut std::io::stdout())?;
            let response = ipc::send_request(&ipc::IpcRequest::DisableKeyboard(kb.hardware_id.clone()))?;
            if let ipc::IpcResponse::Error(e) = response {
                println!("\x1b[31m✗ Failed: {}\x1b[0m", e);
            } else {
                println!("\x1b[90m✓\x1b[0m");
                changes_made = true;
            }
        }
        // If should_enable == currently_enabled, no change needed (don't print anything)
    }

    if changes_made {
        println!("\n\x1b[32m✓ Configuration updated successfully\x1b[0m\n");
    } else {
        println!("\n\x1b[90mNo changes made\x1b[0m\n");
    }

    Ok(())
}

fn handle_shutdown() -> Result<()> {
    let response = ipc::send_request(&ipc::IpcRequest::Shutdown)?;
    match response {
        ipc::IpcResponse::Ok => {
            println!("✓ Daemon shutdown requested");
            Ok(())
        }
        ipc::IpcResponse::Error(e) => anyhow::bail!("Error: {}", e),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // Client commands
        Some(Commands::SetPassword) => {
            return handle_set_password();
        }
        Some(Commands::Ping) => {
            return handle_ping();
        }
        Some(Commands::ListKeyboards) => {
            return handle_list_keyboards();
        }
        Some(Commands::ToggleKeyboards) => {
            return handle_toggle_keyboards();
        }
        Some(Commands::Shutdown) => {
            return handle_shutdown();
        }
        // Daemon mode
        Some(Commands::Daemon) | None => {
            // Initialize tracing for daemon
            tracing_subscriber::fmt::init();

            // Load config
            let config_path = get_config_path();
            let config = config::Config::load_or_default(&config_path);

            // Create and run daemon
            let mut daemon = daemon::Daemon::new(config, config_path)?;
            daemon.run()?;
        }
    }

    Ok(())
}
