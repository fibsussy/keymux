// New process_event implementation with action recording

#![allow(clippy::inline_always)]

use anyhow::Result;
use evdev::{InputEvent, EventType, Key};
use tracing::info;

use crate::keyboard_state::{Action, KeyAction, KeyboardState};
use crate::uinput::VirtualKeyboard;

// Branch prediction hints for hot path optimization
#[inline(always)]
#[cold]
const fn cold() {}

#[inline(always)]
const fn likely(b: bool) -> bool {
    if !b {
        cold();
    }
    b
}

#[inline(always)]
const fn unlikely(b: bool) -> bool {
    if b {
        cold();
    }
    b
}

#[allow(clippy::too_many_lines)]
#[inline]
pub fn process_event(
    event: InputEvent,
    state: &mut KeyboardState,
    vkbd: &mut VirtualKeyboard,
) -> Result<()> {
    // Fast path: only handle KEY events
    if unlikely(event.event_type() != EventType::KEY) {
        return Ok(());
    }

    let key = Key::new(event.code());
    let pressed = event.value() == 1;
    let released = event.value() == 0;
    let repeat = event.value() == 2;

    // Fast path: ignore repeats immediately
    if unlikely(repeat) {
        return Ok(());
    }

    // ============================================================================
    // KEY PRESS: Record what this key will do
    // ============================================================================
    if likely(pressed) {
        let mut key_action = KeyAction::new();

        // Track Caps Lock toggle
        // Only toggle if ESC is mapped to CAPSLOCK (default behavior when esc_to_grave is false)
        if unlikely(key == Key::KEY_ESC && state.key_remapping.caps_to_esc && !state.key_remapping.esc_to_grave) {
            state.caps_lock_on = !state.caps_lock_on;
        }

        // Detect both shifts held (no other keys) to toggle game mode override
        if unlikely(matches!(key, Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT)) {
            // Record this shift key first
            let final_key = apply_key_remapping(key, &state.key_remapping);
            vkbd.press_key(final_key)?;
            key_action.add(Action::RegularKey(final_key));
            state.insert_held_key(key, key_action);

            // Check if only both shifts are now held
            if state.only_both_shifts_held() {
                state.toggle_game_mode_override();
                info!("Game mode override toggled for window: {:?}", state.current_window_app_id);
            }
            return Ok(());
        }

        // Track Left Alt for nav layer (disabled in game mode, passes through as regular key)
        if unlikely(key == Key::KEY_LEFTALT)
            && likely(!state.effective_game_mode()) {
                state.nav_layer_active = true;
                key_action.add(Action::NavLayerActivation);
                state.insert_held_key(key, key_action);
                return Ok(());
            }
            // In game mode, treat as regular key (will be handled below)

        // === GAME MODE HANDLING ===
        // Game mode is controlled ONLY by niri monitor
        if unlikely(state.effective_game_mode()) {
            // SOCD cleaning for WASD (ULTRA HOT PATH for gamers)
            if likely(KeyboardState::is_wasd_key(key)) {
                let output_keys = state.socd_cleaner.handle_press(key);
                vkbd.update_socd_keys(output_keys)?;
                key_action.add(Action::SocdManaged);
                state.insert_held_key(key, key_action);
                return Ok(());
            }
        }

        // === NAV LAYER (Left Alt held) ===
        if unlikely(state.nav_layer_active) {
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
                Key::KEY_UP | Key::KEY_DOWN => {
                    vkbd.press_key(Key::BTN_MIDDLE)?;
                    key_action.add(Action::RegularKey(Key::BTN_MIDDLE));
                }
                Key::KEY_LEFT => {
                    vkbd.press_key(Key::BTN_LEFT)?;
                    key_action.add(Action::RegularKey(Key::BTN_LEFT));
                }
                Key::KEY_RIGHT => {
                    vkbd.press_key(Key::BTN_RIGHT)?;
                    key_action.add(Action::RegularKey(Key::BTN_RIGHT));
                }
                // Backspace -> Password typer (cold path - rarely used)
                Key::KEY_BACKSPACE => {
                    if unlikely(!state.password_typed_in_nav) {
                        // First press: type password with modifier stow/unstow
                        if let Some(ref password) = state.password {
                            cold();  // Mark as cold path
                            info!("Typing password");
                            // Get active modifiers and caps lock state, use stow/unstow pattern
                            let active_mods = state.modifier_state.get_active_modifiers();
                            vkbd.type_string_with_mods(password, &active_mods, state.caps_lock_on)?;
                            state.password_typed_in_nav = true;
                        }
                    } else {
                        // Subsequent presses: press Enter
                        cold();  // Mark as cold path
                        info!("Pressing Enter (password already typed)");
                        vkbd.tap_key(Key::KEY_ENTER)?;
                    }
                    // Don't record this key action since it's a special command
                    state.insert_held_key(key, key_action);
                    return Ok(());
                }
                _ => {
                    // Other keys pass through normally in nav layer
                    let final_key = apply_key_remapping(key, &state.key_remapping);
                    vkbd.press_key(final_key)?;
                    key_action.add(Action::RegularKey(final_key));
                }
            }
            state.insert_held_key(key, key_action);
            return Ok(());
        }

        // === HOME ROW MODS ===
        // In game mode: disable left hand (ASDF), keep right hand (JKL;)
        let is_left_hand_hrm = matches!(key, Key::KEY_A | Key::KEY_S | Key::KEY_D | Key::KEY_F);
        let skip_hrm = state.effective_game_mode() && is_left_hand_hrm;

        if likely(!skip_hrm) {
            if likely(KeyboardState::is_home_row_mod(key)) {
                // Check for double-tap: if this key was tapped recently, hold the base key
                if state.is_double_tap(key) {
                    if let Some(hrm) = KeyboardState::get_home_row_mod(key) {
                        // Double-tap detected: press and hold the base key
                        vkbd.press_key(hrm.base_key)?;
                        key_action.add(Action::HomeRowModHoldingBase { base_key: hrm.base_key });
                        state.insert_held_key(key, key_action);
                        return Ok(());
                    }
                }

                // Check if ANY other home row mod is held and pending (bit flag check)
                // Iterate through all 8 home row mod keys
                const HRM_KEYS: [Key; 8] = [
                    Key::KEY_A, Key::KEY_S, Key::KEY_D, Key::KEY_F,
                    Key::KEY_J, Key::KEY_K, Key::KEY_L, Key::KEY_SEMICOLON,
                ];

                for &hrm_key in &HRM_KEYS {
                    if state.is_hrm_pending(hrm_key) {
                        // In game mode, skip left hand home row mods
                        let is_left_hand = matches!(hrm_key, Key::KEY_A | Key::KEY_S | Key::KEY_D | Key::KEY_F);
                        if state.effective_game_mode() && is_left_hand {
                            continue;
                        }

                        if let Some(other_hrm) = KeyboardState::get_home_row_mod(hrm_key) {
                            vkbd.press_key(other_hrm.modifier)?;
                            state.modifier_state.increment(other_hrm.modifier);

                            // Update the action and remove from pending set
                            if let Some(other_action) = state.get_held_key_mut(hrm_key) {
                                other_action.clear();
                                other_action.add(Action::Modifier(other_hrm.modifier));
                            }
                            state.clear_hrm_pending(hrm_key);
                        }
                    }
                }

                // Record this key as pending
                key_action.add(Action::HomeRowModPending { hrm_key: key });
                state.set_hrm_pending(key);
                state.insert_held_key(key, key_action);
                return Ok(());
            }

            // Non-home-row key pressed: activate any pending home row mods (permissive hold)
            if likely(!KeyboardState::is_wasd_key(key)) && unlikely(state.has_pending_hrm()) {
                const HRM_KEYS: [Key; 8] = [
                    Key::KEY_A, Key::KEY_S, Key::KEY_D, Key::KEY_F,
                    Key::KEY_J, Key::KEY_K, Key::KEY_L, Key::KEY_SEMICOLON,
                ];

                for &hrm_key in &HRM_KEYS {
                    if state.is_hrm_pending(hrm_key) {
                        // In game mode, skip left hand home row mods
                        let is_left_hand = matches!(hrm_key, Key::KEY_A | Key::KEY_S | Key::KEY_D | Key::KEY_F);
                        if state.effective_game_mode() && is_left_hand {
                            continue;
                        }

                        if let Some(hrm) = KeyboardState::get_home_row_mod(hrm_key) {
                            vkbd.press_key(hrm.modifier)?;
                            state.modifier_state.increment(hrm.modifier);

                            // Update the action and remove from pending set
                            if let Some(held_action) = state.get_held_key_mut(hrm_key) {
                                held_action.clear();
                                held_action.add(Action::Modifier(hrm.modifier));
                            }
                            state.clear_hrm_pending(hrm_key);
                        }
                    }
                }
            }
        }

        // === REGULAR KEY ===
        let final_key = apply_key_remapping(key, &state.key_remapping);
        vkbd.press_key(final_key)?;
        key_action.add(Action::RegularKey(final_key));
        state.insert_held_key(key, key_action);
        return Ok(());
    }

    // ============================================================================
    // KEY RELEASE: Undo what this key was doing
    // ============================================================================
    if likely(released) {
        if let Some(key_action) = state.remove_held_key(key) {
            for action in key_action.iter() {
                match action {
                    Action::Modifier(modifier_key) => {
                        // Decrement ref count, only release if count hits 0
                        if state.modifier_state.decrement(*modifier_key) {
                            vkbd.release_key(*modifier_key)?;
                        }
                    }
                    Action::RegularKey(emitted_key) => {
                        vkbd.release_key(*emitted_key)?;
                    }
                    Action::SocdManaged => {
                        // SOCD cleaner handles this (ULTRA HOT PATH for gamers)
                        let output_keys = state.socd_cleaner.handle_release(key);
                        vkbd.update_socd_keys(output_keys)?;
                    }
                    Action::NavLayerActivation => {
                        state.nav_layer_active = false;
                        state.password_typed_in_nav = false;
                    }
                    Action::HomeRowModPending { hrm_key } => {
                        // Key was released while pending - tap it (no logging for latency)
                        state.clear_hrm_pending(*hrm_key);
                        if let Some(hrm) = KeyboardState::get_home_row_mod(*hrm_key) {
                            // Note: tapping term check could be added here if needed
                            // For now we always tap regardless of hold duration
                            vkbd.tap_key(hrm.base_key)?;
                            // Record tap time for double-tap detection
                            state.set_hrm_last_tap(*hrm_key);
                        }
                    }
                    Action::HomeRowModHoldingBase { base_key } => {
                        // Release the base key that was being held (double-tap-and-hold)
                        vkbd.release_key(*base_key)?;
                    }
                }
            }
        }
        return Ok(());
    }

    Ok(())
}

/// Apply key remapping based on configuration
#[inline(always)]
const fn apply_key_remapping(key: Key, remapping: &crate::config::KeyRemapping) -> Key {
    match key {
        Key::KEY_CAPSLOCK => {
            if remapping.caps_to_esc {
                Key::KEY_ESC
            } else {
                key
            }
        }
        Key::KEY_ESC => {
            if remapping.esc_to_grave {
                Key::KEY_GRAVE
            } else if remapping.caps_to_esc {
                // Default: swap ESC to CAPSLOCK when caps_to_esc is enabled
                Key::KEY_CAPSLOCK
            } else {
                key
            }
        }
        _ => key,
    }
}
