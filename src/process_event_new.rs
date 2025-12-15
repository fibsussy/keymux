// New process_event implementation with action recording

use anyhow::Result;
use evdev::{InputEvent, EventType, Key};
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::{KeyboardState, Action, KeyAction, VirtualKeyboard, TAPPING_TERM_MS};

#[allow(clippy::too_many_lines)]
pub async fn process_event(
    event: InputEvent,
    state: &mut KeyboardState,
    vkbd: &mut VirtualKeyboard,
) -> Result<()> {
    if event.event_type() != EventType::KEY {
        return Ok(());
    }

    let key = Key::new(event.code());
    let pressed = event.value() == 1;
    let released = event.value() == 0;
    let repeat = event.value() == 2;

    if repeat {
        return Ok(());
    }

    debug!("Event: {:?} pressed={} released={}", key, pressed, released);

    // ============================================================================
    // KEY PRESS: Record what this key will do
    // ============================================================================
    if pressed {
        let mut key_action = KeyAction::new();

        // Track Left Alt for nav layer (disabled in game mode, passes through as regular key)
        if key == Key::KEY_LEFTALT {
            if !state.game_mode {
                state.nav_layer_active = true;
                key_action.add(Action::NavLayerActivation);
                state.held_keys.insert(key, key_action);
                return Ok(());
            }
            // In game mode, treat as regular key (will be handled below)
        }

        // === GAME MODE HANDLING ===
        // Game mode is controlled ONLY by niri monitor
        if state.game_mode {
            // SOCD cleaning for WASD
            if KeyboardState::is_wasd_key(key) {
                let output_keys = state.socd_cleaner.handle_press(key);
                vkbd.update_socd_keys(output_keys)?;
                key_action.add(Action::SocdManaged);
                state.held_keys.insert(key, key_action);
                return Ok(());
            }
        }

        // === NAV LAYER (Left Alt held) ===
        if state.nav_layer_active {
            match key {
                // Home row -> modifiers
                Key::KEY_A => {
                    vkbd.press_key(Key::KEY_LEFTMETA)?;
                    state.modifier_state.increment(Key::KEY_LEFTMETA);
                    key_action.add(Action::Modifier(Key::KEY_LEFTMETA));
                }
                Key::KEY_S => {
                    vkbd.press_key(Key::KEY_LEFTALT)?;
                    state.modifier_state.increment(Key::KEY_LEFTALT);
                    key_action.add(Action::Modifier(Key::KEY_LEFTALT));
                }
                Key::KEY_D => {
                    vkbd.press_key(Key::KEY_LEFTCTRL)?;
                    state.modifier_state.increment(Key::KEY_LEFTCTRL);
                    key_action.add(Action::Modifier(Key::KEY_LEFTCTRL));
                }
                Key::KEY_F => {
                    vkbd.press_key(Key::KEY_LEFTSHIFT)?;
                    state.modifier_state.increment(Key::KEY_LEFTSHIFT);
                    key_action.add(Action::Modifier(Key::KEY_LEFTSHIFT));
                }
                // HJKL -> arrow keys
                Key::KEY_H => {
                    vkbd.press_key(Key::KEY_LEFT)?;
                    key_action.add(Action::RegularKey(Key::KEY_LEFT));
                }
                Key::KEY_J => {
                    vkbd.press_key(Key::KEY_DOWN)?;
                    key_action.add(Action::RegularKey(Key::KEY_DOWN));
                }
                Key::KEY_K => {
                    vkbd.press_key(Key::KEY_UP)?;
                    key_action.add(Action::RegularKey(Key::KEY_UP));
                }
                Key::KEY_L => {
                    vkbd.press_key(Key::KEY_RIGHT)?;
                    key_action.add(Action::RegularKey(Key::KEY_RIGHT));
                }
                // Mouse buttons
                Key::KEY_UP => {
                    vkbd.press_key(Key::BTN_MIDDLE)?;
                    key_action.add(Action::RegularKey(Key::BTN_MIDDLE));
                }
                Key::KEY_LEFT => {
                    vkbd.press_key(Key::BTN_LEFT)?;
                    key_action.add(Action::RegularKey(Key::BTN_LEFT));
                }
                Key::KEY_DOWN => {
                    vkbd.press_key(Key::BTN_MIDDLE)?;
                    key_action.add(Action::RegularKey(Key::BTN_MIDDLE));
                }
                Key::KEY_RIGHT => {
                    vkbd.press_key(Key::BTN_RIGHT)?;
                    key_action.add(Action::RegularKey(Key::BTN_RIGHT));
                }
                // Backspace -> Password typer
                Key::KEY_BACKSPACE => {
                    if !state.password_typed_in_nav {
                        // First press: type password
                        if let Some(ref password) = state.password {
                            info!("Typing password");
                            vkbd.type_string(password)?;
                            state.password_typed_in_nav = true;
                        }
                    } else {
                        // Subsequent presses: press Enter
                        info!("Pressing Enter (password already typed)");
                        vkbd.tap_key(Key::KEY_ENTER)?;
                    }
                    // Don't record this key action since it's a special command
                    state.held_keys.insert(key, key_action);
                    return Ok(());
                }
                _ => {
                    // Other keys pass through normally in nav layer
                    let final_key = map_caps_esc(key);
                    vkbd.press_key(final_key)?;
                    key_action.add(Action::RegularKey(final_key));
                }
            }
            state.held_keys.insert(key, key_action);
            return Ok(());
        }

        // === HOME ROW MODS ===
        // In game mode: disable left hand (ASDF), keep right hand (JKL;)
        let is_left_hand_hrm = matches!(key, Key::KEY_A | Key::KEY_S | Key::KEY_D | Key::KEY_F);
        let skip_hrm = state.game_mode && is_left_hand_hrm;

        if !skip_hrm {
            if let Some(_hrm) = state.home_row_mods.get(&key) {
                // Check if ANY other home row mod is held and pending
                // Collect keys to activate first (to avoid borrow issues)
                let mut keys_to_activate = Vec::new();
                for (other_key, other_action) in &state.held_keys {
                    if *other_key == key {
                        continue;
                    }
                    for action in &other_action.actions {
                        if let Action::HomeRowModPending { hrm_key, .. } = action {
                            // In game mode, skip left hand home row mods
                            let is_left_hand = matches!(*hrm_key, Key::KEY_A | Key::KEY_S | Key::KEY_D | Key::KEY_F);
                            if state.game_mode && is_left_hand {
                                continue;
                            }
                            keys_to_activate.push(*hrm_key);
                        }
                    }
                }

                // Now activate them
                for hrm_key in keys_to_activate {
                    if let Some(other_hrm) = state.home_row_mods.get(&hrm_key) {
                        vkbd.press_key(other_hrm.modifier)?;
                        state.modifier_state.increment(other_hrm.modifier);
                        info!("Home row mod activated by another home row key: {:?}", other_hrm.modifier);

                        // Update the action
                        if let Some(other_action) = state.held_keys.get_mut(&hrm_key) {
                            other_action.actions.clear();
                            other_action.add(Action::Modifier(other_hrm.modifier));
                        }
                    }
                }

                // Record this key as pending
                key_action.add(Action::HomeRowModPending {
                    hrm_key: key,
                    press_time: Instant::now(),
                });
                debug!("Home row key pressed, waiting to determine tap or hold: {:?}", key);
                state.held_keys.insert(key, key_action);
                return Ok(());
            }

            // Non-home-row key pressed: activate any pending home row mods (permissive hold)
            if !KeyboardState::is_wasd_key(key) {
                let mut keys_to_activate = Vec::new();
                for (_held_key, held_action) in &state.held_keys {
                    for action in &held_action.actions {
                        if let Action::HomeRowModPending { hrm_key, .. } = action {
                            // In game mode, skip left hand home row mods
                            let is_left_hand = matches!(*hrm_key, Key::KEY_A | Key::KEY_S | Key::KEY_D | Key::KEY_F);
                            if state.game_mode && is_left_hand {
                                continue;
                            }
                            keys_to_activate.push(*hrm_key);
                        }
                    }
                }

                for hrm_key in keys_to_activate {
                    if let Some(hrm) = state.home_row_mods.get(&hrm_key) {
                        vkbd.press_key(hrm.modifier)?;
                        state.modifier_state.increment(hrm.modifier);
                        info!("Home row mod activated by other key press: {:?}", hrm.modifier);

                        // Update the action
                        if let Some(held_action) = state.held_keys.get_mut(&hrm_key) {
                            held_action.actions.clear();
                            held_action.add(Action::Modifier(hrm.modifier));
                        }
                    }
                }
            }
        }

        // === REGULAR KEY ===
        let final_key = map_caps_esc(key);
        vkbd.press_key(final_key)?;
        key_action.add(Action::RegularKey(final_key));
        state.held_keys.insert(key, key_action);
        return Ok(());
    }

    // ============================================================================
    // KEY RELEASE: Undo what this key was doing
    // ============================================================================
    if released {
        if let Some(key_action) = state.held_keys.remove(&key) {
            for action in &key_action.actions {
                match action {
                    Action::Modifier(modifier_key) => {
                        // Decrement ref count, only release if count hits 0
                        if state.modifier_state.decrement(*modifier_key) {
                            vkbd.release_key(*modifier_key)?;
                            info!("Modifier released: {:?}", modifier_key);
                        } else {
                            debug!("Modifier still held by other keys: {:?}", modifier_key);
                        }
                    }
                    Action::RegularKey(emitted_key) => {
                        vkbd.release_key(*emitted_key)?;
                    }
                    Action::SocdManaged => {
                        // SOCD cleaner handles this
                        let output_keys = state.socd_cleaner.handle_release(key);
                        vkbd.update_socd_keys(output_keys)?;
                    }
                    Action::NavLayerActivation => {
                        state.nav_layer_active = false;
                        state.password_typed_in_nav = false;
                        debug!("Nav layer deactivated, password state reset");
                    }
                    Action::HomeRowModPending { hrm_key, press_time } => {
                        // Key was released while pending - determine tap or hold
                        let hold_duration = Instant::now().duration_since(*press_time);
                        if let Some(hrm) = state.home_row_mods.get(hrm_key) {
                            if hold_duration < Duration::from_millis(TAPPING_TERM_MS) {
                                info!("Home row QUICK TAP: {:?} ({}ms)", hrm.base_key, hold_duration.as_millis());
                                vkbd.tap_key(hrm.base_key)?;
                            } else {
                                info!("Home row LONG HOLD TAP: {:?} ({}ms)", hrm.base_key, hold_duration.as_millis());
                                vkbd.tap_key(hrm.base_key)?;
                            }
                        }
                    }
                }
            }
        } else {
            debug!("Key released but not found in held_keys: {:?}", key);
        }
        return Ok(());
    }

    Ok(())
}

fn map_caps_esc(key: Key) -> Key {
    match key {
        Key::KEY_CAPSLOCK => Key::KEY_ESC,
        Key::KEY_ESC => Key::KEY_CAPSLOCK,
        _ => key,
    }
}
