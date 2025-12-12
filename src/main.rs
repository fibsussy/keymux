use anyhow::{Context, Result};
use evdev::{Device, EventType, InputEvent, Key};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};
use tracing::{debug, error, info};

mod config;
mod niri;
mod socd;
mod uinput;

#[cfg(test)]
mod tests;

use niri::NiriEvent;
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

#[derive(Debug, Clone)]
struct HomeRowMod {
    key: Key,
    modifier: Key,
    base_key: Key,
    press_time: Instant,
    is_mod: bool,
    other_key_pressed: bool,
}

struct KeyboardState {
    layers: Vec<Layer>,
    home_row_mods: HashMap<Key, HomeRowMod>,
    socd_cleaner: SocdCleaner,
    game_mode: bool,
    wasd_sequence: Vec<(Key, Instant)>,
    pressed_keys: HashSet<Key>,
}

impl KeyboardState {
    fn new() -> Self {
        Self {
            layers: vec![Layer::Base, Layer::HomeRowMod],
            home_row_mods: Self::init_home_row_mods(),
            socd_cleaner: SocdCleaner::new(),
            game_mode: false,
            wasd_sequence: Vec::new(),
            pressed_keys: HashSet::new(),
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
                press_time: Instant::now(),
                is_mod: false,
                other_key_pressed: false,
            },
        );
        mods.insert(
            Key::KEY_S,
            HomeRowMod {
                key: Key::KEY_S,
                modifier: Key::KEY_LEFTALT,
                base_key: Key::KEY_S,
                press_time: Instant::now(),
                is_mod: false,
                other_key_pressed: false,
            },
        );
        mods.insert(
            Key::KEY_D,
            HomeRowMod {
                key: Key::KEY_D,
                modifier: Key::KEY_LEFTCTRL,
                base_key: Key::KEY_D,
                press_time: Instant::now(),
                is_mod: false,
                other_key_pressed: false,
            },
        );
        mods.insert(
            Key::KEY_F,
            HomeRowMod {
                key: Key::KEY_F,
                modifier: Key::KEY_LEFTSHIFT,
                base_key: Key::KEY_F,
                press_time: Instant::now(),
                is_mod: false,
                other_key_pressed: false,
            },
        );

        // Right hand home row mods
        mods.insert(
            Key::KEY_J,
            HomeRowMod {
                key: Key::KEY_J,
                modifier: Key::KEY_RIGHTSHIFT,
                base_key: Key::KEY_J,
                press_time: Instant::now(),
                is_mod: false,
                other_key_pressed: false,
            },
        );
        mods.insert(
            Key::KEY_K,
            HomeRowMod {
                key: Key::KEY_K,
                modifier: Key::KEY_RIGHTCTRL,
                base_key: Key::KEY_K,
                press_time: Instant::now(),
                is_mod: false,
                other_key_pressed: false,
            },
        );
        mods.insert(
            Key::KEY_L,
            HomeRowMod {
                key: Key::KEY_L,
                modifier: Key::KEY_RIGHTALT,
                base_key: Key::KEY_L,
                press_time: Instant::now(),
                is_mod: false,
                other_key_pressed: false,
            },
        );
        mods.insert(
            Key::KEY_SEMICOLON,
            HomeRowMod {
                key: Key::KEY_SEMICOLON,
                modifier: Key::KEY_RIGHTMETA,
                base_key: Key::KEY_SEMICOLON,
                press_time: Instant::now(),
                is_mod: false,
                other_key_pressed: false,
            },
        );

        mods
    }

    const fn is_wasd_key(key: Key) -> bool {
        matches!(key, Key::KEY_W | Key::KEY_A | Key::KEY_S | Key::KEY_D)
    }

    fn check_game_mode_entry(&mut self, key: Key, pressed: bool) {
        if self.game_mode || !pressed || !Self::is_wasd_key(key) {
            return;
        }

        let now = Instant::now();
        self.wasd_sequence.push((key, now));

        // Keep only recent presses (within 2 seconds)
        self.wasd_sequence
            .retain(|(_, time)| now.duration_since(*time) < Duration::from_secs(2));

        // Check for rapid alternation (5+ different keys pressed)
        let unique_keys: HashSet<_> = self.wasd_sequence.iter().map(|(k, _)| k).collect();

        if unique_keys.len() >= 4 || self.wasd_sequence.len() >= 8 {
            info!("Auto-entering game mode due to WASD activity");
            self.enter_game_mode();
        }
    }

    fn enter_game_mode(&mut self) {
        info!("Entering game mode");
        self.game_mode = true;
        self.layers = vec![Layer::Base, Layer::Game];
        self.wasd_sequence.clear();
    }

    fn exit_game_mode(&mut self) {
        info!("Exiting game mode");
        self.game_mode = false;
        self.layers = vec![Layer::Base, Layer::HomeRowMod];
        self.socd_cleaner.reset();
    }

    const fn should_exit_game_mode(key: Key, pressed: bool) -> bool {
        if !pressed {
            return false;
        }

        // Exit on GUI key press
        matches!(
            key,
            Key::KEY_LEFTMETA | Key::KEY_RIGHTMETA | Key::KEY_LEFTALT | Key::KEY_RIGHTALT
        )
    }
}

#[allow(clippy::too_many_lines, clippy::unused_async)]
async fn process_event(
    event: InputEvent,
    state: &mut KeyboardState,
    vkbd: &mut VirtualKeyboard,
) -> Result<()> {
    if event.event_type() != EventType::KEY {
        // Pass through non-key events
        return Ok(());
    }

    let key = Key::new(event.code());
    let pressed = event.value() == 1;
    let released = event.value() == 0;
    let repeat = event.value() == 2;

    // Ignore repeat events (we handle our own repeats)
    if repeat {
        return Ok(());
    }

    debug!("Event: {:?} pressed={} released={}", key, pressed, released);

    // Track left Alt for nav layer (layer key only, don't emit it)
    if key == Key::KEY_LEFTALT {
        if pressed {
            state.pressed_keys.insert(key);
        } else {
            state.pressed_keys.remove(&key);
        }
        // Don't pass through left Alt - it's just a layer key
        return Ok(());
    }

    // Check for game mode entry
    state.check_game_mode_entry(key, pressed);

    // Handle game mode
    if state.game_mode {
        if KeyboardState::should_exit_game_mode(key, pressed) {
            state.exit_game_mode();
        }

        // SOCD cleaning for WASD in game mode
        if KeyboardState::is_wasd_key(key) {
            if pressed {
                let output_keys = state.socd_cleaner.handle_press(key);
                vkbd.update_socd_keys(output_keys)?;
                return Ok(());
            } else if released {
                let output_keys = state.socd_cleaner.handle_release(key);
                vkbd.update_socd_keys(output_keys)?;
                return Ok(());
            }
        }
    }

    // Check for nav layer activation (left Alt held)
    let nav_layer_active = state.pressed_keys.contains(&Key::KEY_LEFTALT);

    // Handle nav layer (when left Alt is held)
    if nav_layer_active {
        if pressed {
            match key {
                // Home row becomes modifiers (not mod-tap, just modifiers)
                Key::KEY_A => {
                    vkbd.press_key(Key::KEY_LEFTMETA)?;
                    return Ok(());
                }
                Key::KEY_S => {
                    vkbd.press_key(Key::KEY_LEFTALT)?;
                    return Ok(());
                }
                Key::KEY_D => {
                    vkbd.press_key(Key::KEY_LEFTCTRL)?;
                    return Ok(());
                }
                Key::KEY_F => {
                    vkbd.press_key(Key::KEY_LEFTSHIFT)?;
                    return Ok(());
                }
                // HJKL -> arrow keys
                Key::KEY_H => {
                    vkbd.press_key(Key::KEY_LEFT)?;
                    return Ok(());
                }
                Key::KEY_J => {
                    vkbd.press_key(Key::KEY_DOWN)?;
                    return Ok(());
                }
                Key::KEY_K => {
                    vkbd.press_key(Key::KEY_UP)?;
                    return Ok(());
                }
                Key::KEY_L => {
                    vkbd.press_key(Key::KEY_RIGHT)?;
                    return Ok(());
                }
                // Mouse buttons and wheel (on right side cluster)
                Key::KEY_UP => {
                    vkbd.press_key(Key::BTN_MIDDLE)?; // MS_UP position = mouse button 3
                    return Ok(());
                }
                Key::KEY_LEFT => {
                    vkbd.press_key(Key::BTN_LEFT)?; // MS_LEFT position = left click
                    return Ok(());
                }
                Key::KEY_DOWN => {
                    vkbd.press_key(Key::BTN_MIDDLE)?; // MS_DOWN position = middle button
                    return Ok(());
                }
                Key::KEY_RIGHT => {
                    vkbd.press_key(Key::BTN_RIGHT)?; // MS_RIGHT position = right click
                    return Ok(());
                }
                _ => {}
            }
        } else if released {
            match key {
                // Home row modifiers
                Key::KEY_A => {
                    vkbd.release_key(Key::KEY_LEFTMETA)?;
                    return Ok(());
                }
                Key::KEY_S => {
                    vkbd.release_key(Key::KEY_LEFTALT)?;
                    return Ok(());
                }
                Key::KEY_D => {
                    vkbd.release_key(Key::KEY_LEFTCTRL)?;
                    return Ok(());
                }
                Key::KEY_F => {
                    vkbd.release_key(Key::KEY_LEFTSHIFT)?;
                    return Ok(());
                }
                // Arrow keys
                Key::KEY_H => {
                    vkbd.release_key(Key::KEY_LEFT)?;
                    return Ok(());
                }
                Key::KEY_J => {
                    vkbd.release_key(Key::KEY_DOWN)?;
                    return Ok(());
                }
                Key::KEY_K => {
                    vkbd.release_key(Key::KEY_UP)?;
                    return Ok(());
                }
                Key::KEY_L => {
                    vkbd.release_key(Key::KEY_RIGHT)?;
                    return Ok(());
                }
                // Mouse buttons
                Key::KEY_UP | Key::KEY_DOWN => {
                    vkbd.release_key(Key::BTN_MIDDLE)?;
                    return Ok(());
                }
                Key::KEY_LEFT => {
                    vkbd.release_key(Key::BTN_LEFT)?;
                    return Ok(());
                }
                Key::KEY_RIGHT => {
                    vkbd.release_key(Key::BTN_RIGHT)?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    // Handle home row mods
    let is_home_row_key = state.home_row_mods.contains_key(&key);

    if is_home_row_key && pressed {
        // First, activate any OTHER held home row mods as modifiers
        for hrm in state.home_row_mods.values_mut() {
            if state.pressed_keys.contains(&hrm.key) && !hrm.is_mod {
                hrm.is_mod = true;
                hrm.other_key_pressed = true;
                vkbd.press_key(hrm.modifier)?;
                info!("Home row mod activated by another home row key: {:?}", hrm.modifier);
            }
        }

        // Now handle this home row key press
        if let Some(hrm) = state.home_row_mods.get_mut(&key) {
            debug!("Home row key pressed: {:?}, other keys: {}", key, state.pressed_keys.len());
            hrm.press_time = Instant::now();
            hrm.is_mod = false;
            hrm.other_key_pressed = false;

            // Don't auto-activate as mod - let it be decided on release or by another key press
            state.pressed_keys.insert(key);
            debug!("Waiting to determine tap or hold for {:?}", key);
            return Ok(());
        }
    } else if is_home_row_key && released {
        if let Some(hrm) = state.home_row_mods.get_mut(&key) {
            let hold_duration = Instant::now().duration_since(hrm.press_time);
            debug!("Home row key released: {:?}, hold_duration: {:?}ms, is_mod: {}",
                   key, hold_duration.as_millis(), hrm.is_mod);

            state.pressed_keys.remove(&key);

            if hrm.is_mod {
                vkbd.release_key(hrm.modifier)?;
                info!("Home row mod released: {:?}", hrm.modifier);
            } else if hold_duration < Duration::from_millis(TAPPING_TERM_MS) {
                info!("Home row QUICK TAP: {:?} ({}ms)", hrm.base_key, hold_duration.as_millis());
                vkbd.tap_key(hrm.base_key)?;
            } else {
                info!("Home row LONG HOLD TAP: {:?} ({}ms)", hrm.base_key, hold_duration.as_millis());
                vkbd.tap_key(hrm.base_key)?;
            }

            return Ok(());
        }
    }

    // Check if any home row mod should activate due to another key press
    // PERMISSIVE HOLD: if a home row key is held and another key is pressed,
    // activate the modifier regardless of timing
    if pressed && !KeyboardState::is_wasd_key(key) && !is_home_row_key {
        for hrm in state.home_row_mods.values_mut() {
            if state.pressed_keys.contains(&hrm.key) && !hrm.is_mod {
                hrm.is_mod = true;
                hrm.other_key_pressed = true;
                vkbd.press_key(hrm.modifier)?;
                info!("Home row mod activated by other key press: {:?}", hrm.modifier);
            }
        }
    }

    // Swap Caps Lock and Escape
    let final_key = match key {
        Key::KEY_CAPSLOCK => Key::KEY_ESC,
        Key::KEY_ESC => Key::KEY_CAPSLOCK,
        _ => key,
    };

    // Track pressed keys for regular keys (non-home-row-mods)
    if pressed {
        state.pressed_keys.insert(key);
        vkbd.press_key(final_key)?;
    } else if released {
        state.pressed_keys.remove(&key);
        vkbd.release_key(final_key)?;
    }

    Ok(())
}

#[allow(clippy::unused_async)]
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

    // If multiple keyboards, prefer physical keyboards over virtual ones
    keyboards.sort_by_key(|(_, _, name)| {
        let name_lower = name.to_lowercase();
        // Deprioritize virtual keyboards from other tools
        if name_lower.contains("virtual") || name_lower.contains("keyd") {
            return 100;
        }
        // Prioritize: specific keyboard names > usb > any other
        if name_lower.contains("lemokey") || name_lower.contains("keychron") {
            0
        } else if name_lower.contains("keyboard") {
            1
        } else if name_lower.contains("usb") {
            2
        } else {
            3
        }
    });

    let (path, device, name) = keyboards.into_iter().next().unwrap();
    info!("Using keyboard: {} ({})", name, path.display());

    Ok(device)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting keyboard middleware");

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
    let mut state = KeyboardState::new();

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
                    state.enter_game_mode();
                } else if !should_enable && state.game_mode {
                    info!("💻 Exiting game mode (left gamescope)");
                    state.exit_game_mode();
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
