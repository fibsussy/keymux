use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dialoguer::Select;
use evdev::{Device, Key};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;
use tracing::{debug, error, info};

mod config;
mod niri;
mod process_event_new;
mod socd;
mod uinput;

#[cfg(test)]
mod tests;

use niri::NiriEvent;
use process_event_new::process_event;
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

async fn find_keyboard_device() -> Result<Device> {
    let devices = evdev::enumerate();

    info!("Scanning for keyboard devices...");

    let mut keyboards = Vec::new();
    let mut device_count = 0;

    // List all devices and find keyboards
    for (path, device) in devices {
        device_count += 1;
        let name = device.name().unwrap_or("unknown").to_string();
        let has_keys = device.supported_keys().is_some();

        info!("Device #{}: {} - {} (has_keys: {})", device_count, path.display(), name, has_keys);

        if let Some(keys) = device.supported_keys() {
            // Check if it has typical keyboard keys
            let has_letter_keys = keys.contains(Key::KEY_A)
                && keys.contains(Key::KEY_Z)
                && keys.contains(Key::KEY_SPACE);

            debug!("  KEY_A: {}, KEY_Z: {}, KEY_SPACE: {}",
                   keys.contains(Key::KEY_A),
                   keys.contains(Key::KEY_Z),
                   keys.contains(Key::KEY_SPACE));

            if has_letter_keys {
                info!("  -> This is a keyboard!");
                keyboards.push((path, device, name));
            }
        }
    }

    if keyboards.is_empty() {
        anyhow::bail!(
            "No keyboard device found.\n\
             Possible solutions:\n\
             1. Run with sudo: sudo cargo run\n\
             2. Add your user to the 'input' group: sudo usermod -a -G input $USER (then log out/in)\n\
             3. Run with RUST_LOG=debug to see all accessible devices\n\
             \n\
             Found {device_count} device(s) but none were keyboards with A-Z and SPACE keys."
        );
    }

    // If only one keyboard, use it automatically
    if keyboards.len() == 1 {
        let (path, device, name) = keyboards.into_iter().next().unwrap();
        info!("Using keyboard: {} ({})", name, path.display());
        return Ok(device);
    }

    // Multiple keyboards - let user choose
    info!("Found {} keyboards", keyboards.len());
    println!("\nFound multiple keyboards:");

    let items: Vec<String> = keyboards
        .iter()
        .enumerate()
        .map(|(i, (path, _, name))| format!("{}. {} ({})", i + 1, name, path.display()))
        .collect();

    let selection = Select::new()
        .with_prompt("Select keyboard to use")
        .items(&items)
        .default(0)
        .interact()
        .context("Failed to get keyboard selection")?;

    let (path, device, name) = keyboards.into_iter().nth(selection).unwrap();
    info!("Using keyboard: {} ({})", name, path.display());

    Ok(device)
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::SetPassword) => {
            return handle_set_password();
        }
        Some(Commands::Daemon) | None => {
            // Continue to daemon mode below
        }
    }

    tracing_subscriber::fmt::init();

    info!("Starting keyboard middleware");

    // Load config
    let config_path = get_config_path();
    let config = config::Config::load_or_default(&config_path);
    if config.password.is_some() {
        info!("Password configured for nav+backspace");
    }

    // Find keyboard device
    let mut device = find_keyboard_device().await?;
    info!("Using device: {}", device.name().unwrap_or("unknown"));

    // Grab the device to intercept all events
    device
        .grab()
        .context("Failed to grab keyboard device. Are you running as root?")?;

    // Create virtual keyboard
    let mut vkbd = VirtualKeyboard::new().context("Failed to create virtual keyboard")?;

    // Initialize state
    let mut state = KeyboardState::new(config.password);

    // Start niri monitor for gamescope detection
    let (niri_tx, niri_rx): (_, Receiver<NiriEvent>) = mpsc::channel();
    niri::start_niri_monitor(niri_tx);
    info!("Niri monitor started for gamescope detection");

    info!("Keyboard middleware ready");

    // Main event loop
    loop {
        // Check for niri events (non-blocking)
        match niri_rx.try_recv() {
            Ok(NiriEvent::WindowFocusChanged(app_id)) => {
                let should_enable = niri::should_enable_gamemode(app_id.as_deref());
                if should_enable && !state.game_mode {
                    info!("🎮 Entering game mode (gamescope detected)");
                    state.game_mode = true;
                    state.layers = vec![Layer::Base, Layer::Game];
                } else if !should_enable && state.game_mode {
                    info!("💻 Exiting game mode (left gamescope)");
                    state.game_mode = false;
                    state.layers = vec![Layer::Base, Layer::HomeRowMod];
                    state.socd_cleaner.reset();
                }
            }
            Err(TryRecvError::Empty) => {
                // No niri events, continue
            }
            Err(TryRecvError::Disconnected) => {
                error!("Niri monitor disconnected");
            }
        }

        let events = device.fetch_events().context("Failed to fetch events")?;

        for event in events {
            if let Err(e) = process_event(event, &mut state, &mut vkbd).await {
                error!("Error processing event: {}", e);
            }
        }
    }
}
