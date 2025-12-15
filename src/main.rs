use anyhow::Result;
use clap::{Parser, Subcommand};

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

use config::KeyRemapping;

#[cfg(test)]
mod tests;

use socd::SocdCleaner;
use uinput::VirtualKeyboard;

/// What a specific key press is doing (recorded when pressed, replayed on release)
#[derive(Debug, Clone, Copy)]
#[repr(u8)]  // Optimize discriminant to single byte
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
    HomeRowModPending { hrm_key: Key },
}

/// What a physical key is currently doing
/// Fixed-size array for max 2 actions - zero allocations!
#[derive(Debug, Clone, Copy)]
#[repr(C)]  // Predictable memory layout
struct KeyAction {
    // Fixed array: max 2 actions (most keys = 1, some = 2, never more)
    actions: [Option<Action>; 2],
    // Track number of actions for fast iteration
    count: u8,
}

impl Default for KeyAction {
    #[inline(always)]
    fn default() -> Self {
        Self::empty()
    }
}

impl KeyAction {
    #[inline(always)]
    const fn empty() -> Self {
        Self {
            actions: [None, None],
            count: 0,
        }
    }

    #[inline(always)]
    const fn new() -> Self {
        Self {
            actions: [None, None],
            count: 0,
        }
    }

    #[inline(always)]
    const fn add(&mut self, action: Action) {
        self.actions[self.count as usize] = Some(action);
        self.count += 1;
    }

    #[inline(always)]
    const fn is_occupied(&self) -> bool {
        self.count > 0
    }

    #[inline(always)]
    const fn clear(&mut self) {
        self.actions = [None, None];
        self.count = 0;
    }

    #[inline(always)]
    fn iter(&self) -> impl Iterator<Item = &Action> {
        self.actions[0..self.count as usize].iter().filter_map(|a| a.as_ref())
    }
}

/// Reference counting for modifiers (fast array-based lookup)
#[derive(Debug, Clone)]
#[repr(C, align(8))]  // Align to cache line for better performance
struct ModifierState {
    counts: [u8; 8], // Fixed size array for speed
}

impl ModifierState {
    #[inline(always)]
    const fn new() -> Self {
        Self { counts: [0; 8] }
    }

    #[inline(always)]
    const fn modifier_index(key: Key) -> Option<usize> {
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

    #[inline(always)]
    const fn index_to_key(idx: usize) -> Key {
        match idx {
            0 => Key::KEY_LEFTSHIFT,
            1 => Key::KEY_RIGHTSHIFT,
            2 => Key::KEY_LEFTCTRL,
            3 => Key::KEY_RIGHTCTRL,
            4 => Key::KEY_LEFTALT,
            5 => Key::KEY_RIGHTALT,
            6 => Key::KEY_LEFTMETA,
            7 => Key::KEY_RIGHTMETA,
            _ => Key::KEY_RESERVED,
        }
    }

    #[inline]
    fn get_active_modifiers(&self) -> [Option<Key>; 8] {
        let mut active = [None; 8];
        let mut write_idx = 0;
        for (idx, &count) in self.counts.iter().enumerate() {
            if count > 0 {
                active[write_idx] = Some(Self::index_to_key(idx));
                write_idx += 1;
            }
        }
        active
    }

    #[inline(always)]
    const fn increment(&mut self, key: Key) {
        if let Some(idx) = Self::modifier_index(key) {
            self.counts[idx] = self.counts[idx].saturating_add(1);
        }
    }

    #[inline(always)]
    const fn decrement(&mut self, key: Key) -> bool {
        if let Some(idx) = Self::modifier_index(key) {
            if self.counts[idx] > 0 {
                self.counts[idx] -= 1;
                return self.counts[idx] == 0; // Return true if we should release
            }
        }
        true // If not tracked, release immediately
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]  // Pack tightly, no padding
struct HomeRowMod {
    modifier: Key,
    base_key: Key,
}

struct KeyboardState {
    // Memory layout optimized: hot fields first, cold fields last
    // TRUE O(1) array lookup by key code (0-255) - NO HASHING!
    // Box to save stack space (256 * ~40 bytes = 10KB on stack otherwise)
    held_keys: Box<[KeyAction; 256]>,
    // Pending home row mods as bit flags (8 keys = 1 byte!)
    pending_hrm_keys: u8,
    // Reference counting for modifiers
    modifier_state: ModifierState,
    socd_cleaner: SocdCleaner,
    // Key remapping configuration (stack-allocated, ~2 bytes)
    key_remapping: KeyRemapping,
    // Frequently accessed bools (packed together for cache)
    game_mode: bool,
    nav_layer_active: bool,
    password_typed_in_nav: bool,
    caps_lock_on: bool,
    // Cold fields (rarely accessed) - Box to save stack space
    password: Option<Box<str>>,
}

impl KeyboardState {
    fn new(password: Option<String>, key_remapping: KeyRemapping) -> Self {
        // Create array with vec and convert to boxed array
        let mut held_keys_vec = Vec::with_capacity(256);
        for _ in 0..256 {
            held_keys_vec.push(KeyAction::empty());
        }
        let held_keys: Box<[KeyAction; 256]> = held_keys_vec.into_boxed_slice().try_into().unwrap();

        Self {
            held_keys,
            // Bit flags: 0 = no pending keys
            pending_hrm_keys: 0,
            modifier_state: ModifierState::new(),
            socd_cleaner: SocdCleaner::new(),
            key_remapping,
            game_mode: false,
            nav_layer_active: false,
            password_typed_in_nav: false,
            caps_lock_on: false,
            // Box password to save stack space (cold data, rarely accessed)
            password: password.map(|s| s.into_boxed_str()),
        }
    }

    // TRUE O(1) operations - direct array indexing, zero hashing!
    #[inline(always)]
    fn get_held_key(&self, key: Key) -> Option<&KeyAction> {
        let slot = &self.held_keys[key.code() as usize];
        if slot.is_occupied() {
            Some(slot)
        } else {
            None
        }
    }

    #[inline(always)]
    fn get_held_key_mut(&mut self, key: Key) -> Option<&mut KeyAction> {
        let slot = &mut self.held_keys[key.code() as usize];
        if slot.is_occupied() {
            Some(slot)
        } else {
            None
        }
    }

    #[inline(always)]
    fn insert_held_key(&mut self, key: Key, action: KeyAction) {
        self.held_keys[key.code() as usize] = action;
    }

    #[inline(always)]
    fn remove_held_key(&mut self, key: Key) -> Option<KeyAction> {
        let slot = &mut self.held_keys[key.code() as usize];
        if slot.is_occupied() {
            let mut taken = KeyAction::empty();
            std::mem::swap(slot, &mut taken);
            Some(taken)
        } else {
            None
        }
    }

    // Helper: Convert key to bit index for pending_hrm_keys (0-7)
    #[inline(always)]
    #[must_use]
    const fn hrm_key_to_bit(key: Key) -> u8 {
        match key {
            Key::KEY_A => 0,
            Key::KEY_S => 1,
            Key::KEY_D => 2,
            Key::KEY_F => 3,
            Key::KEY_J => 4,
            Key::KEY_K => 5,
            Key::KEY_L => 6,
            Key::KEY_SEMICOLON => 7,
            _ => 255, // Invalid
        }
    }

    // Check if a home row mod key is pending
    #[inline(always)]
    #[must_use]
    const fn is_hrm_pending(&self, key: Key) -> bool {
        let bit = Self::hrm_key_to_bit(key);
        bit < 8 && (self.pending_hrm_keys & (1 << bit)) != 0
    }

    // Set a home row mod key as pending
    #[inline(always)]
    const fn set_hrm_pending(&mut self, key: Key) {
        let bit = Self::hrm_key_to_bit(key);
        if bit < 8 {
            self.pending_hrm_keys |= 1 << bit;
        }
    }

    // Clear a home row mod key from pending
    #[inline(always)]
    const fn clear_hrm_pending(&mut self, key: Key) {
        let bit = Self::hrm_key_to_bit(key);
        if bit < 8 {
            self.pending_hrm_keys &= !(1 << bit);
        }
    }

    // Check if ANY home row mod keys are pending
    #[inline(always)]
    #[must_use]
    const fn has_pending_hrm(&self) -> bool {
        self.pending_hrm_keys != 0
    }

    // Array-based lookup instead of HashMap (compiler optimizes to jump table)
    #[inline(always)]
    #[must_use]
    const fn get_home_row_mod(key: Key) -> Option<HomeRowMod> {
        match key {
            Key::KEY_A => Some(HomeRowMod {
                modifier: Key::KEY_LEFTMETA,
                base_key: Key::KEY_A,
            }),
            Key::KEY_S => Some(HomeRowMod {
                modifier: Key::KEY_LEFTALT,
                base_key: Key::KEY_S,
            }),
            Key::KEY_D => Some(HomeRowMod {
                modifier: Key::KEY_LEFTCTRL,
                base_key: Key::KEY_D,
            }),
            Key::KEY_F => Some(HomeRowMod {
                modifier: Key::KEY_LEFTSHIFT,
                base_key: Key::KEY_F,
            }),
            Key::KEY_J => Some(HomeRowMod {
                modifier: Key::KEY_RIGHTSHIFT,
                base_key: Key::KEY_J,
            }),
            Key::KEY_K => Some(HomeRowMod {
                modifier: Key::KEY_RIGHTCTRL,
                base_key: Key::KEY_K,
            }),
            Key::KEY_L => Some(HomeRowMod {
                modifier: Key::KEY_RIGHTALT,
                base_key: Key::KEY_L,
            }),
            Key::KEY_SEMICOLON => Some(HomeRowMod {
                modifier: Key::KEY_RIGHTMETA,
                base_key: Key::KEY_SEMICOLON,
            }),
            _ => None,
        }
    }

    #[inline(always)]
    #[must_use]
    const fn is_home_row_mod(key: Key) -> bool {
        matches!(
            key,
            Key::KEY_A | Key::KEY_S | Key::KEY_D | Key::KEY_F |
            Key::KEY_J | Key::KEY_K | Key::KEY_L | Key::KEY_SEMICOLON
        )
    }

    #[inline(always)]
    #[must_use]
    const fn is_wasd_key(key: Key) -> bool {
        matches!(key, Key::KEY_W | Key::KEY_A | Key::KEY_S | Key::KEY_D)
    }
}

// Compile-time assertions to ensure optimal memory layout
const _: () = {
    // Ensure ModifierState is small (8 bytes)
    assert!(std::mem::size_of::<ModifierState>() == 8);
    // Ensure HomeRowMod fits in 8 bytes (2 Keys = 2*u16 = 4 bytes, padded to 8)
    assert!(std::mem::size_of::<HomeRowMod>() <= 8);
};

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
    dirs::config_dir().map_or_else(|| std::path::PathBuf::from("config.toml"), |p| p.join("keyboard-middleware").join("config.toml"))
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
        ipc::IpcResponse::Error(e) => anyhow::bail!("Error: {e}"),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

fn handle_toggle_keyboards() -> Result<()> {
    // Get current keyboard list
    let response = ipc::send_request(&ipc::IpcRequest::ToggleKeyboards)?;
    let keyboards = match response {
        ipc::IpcResponse::KeyboardList(kb) => kb,
        ipc::IpcResponse::Error(e) => anyhow::bail!("Error: {e}"),
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
                println!("\x1b[31m✗ Failed: {e}\x1b[0m");
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
                println!("\x1b[31m✗ Failed: {e}\x1b[0m");
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
        ipc::IpcResponse::Error(e) => anyhow::bail!("Error: {e}"),
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
