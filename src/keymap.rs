use evdev::Key;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::config::{Action as ConfigAction, Config, KeyCode, Layer};

/// What a key press is doing (recorded on press, replayed on release)
#[derive(Debug, Clone)]
enum KeyAction {
    /// Emitted a regular key
    RegularKey(KeyCode),
    /// Activated a modifier
    Modifier(KeyCode),
    /// Layer activation
    Layer(Layer),
    /// Home row mod pending decision
    HomeRowModPending { tap_key: KeyCode, hold_key: KeyCode },
    /// Home row mod holding base key (double-tap-and-hold)
    HomeRowModHoldingBase { base_key: KeyCode },
    /// Overload pending decision
    OverloadPending { tap_key: KeyCode, hold_key: KeyCode },
    /// Overload holding (double-tap)
    OverloadHolding { hold_key: KeyCode },
    /// SOCD managed key
    SocdManaged,
}

/// SOCD pair tracking
#[derive(Debug, Clone)]
/// SOCD group configuration
struct SocdGroup {
    /// All keys in this SOCD group
    all_keys: Vec<KeyCode>,
    /// Stack of currently held keys (most recent at the end)
    held_stack: Vec<KeyCode>,
    /// Currently active (emitted) key
    active_key: Option<KeyCode>,
}

/// QMK-inspired keymap processor
pub struct KeymapProcessor {
    /// What each physical key is currently doing (indexed by `KeyCode`)
    held_keys: HashMap<KeyCode, Vec<KeyAction>>,

    /// Home row mods - now generic, any key can be an HR mod
    pending_hrm: HashSet<KeyCode>,
    hrm_last_tap: HashMap<KeyCode, Instant>,
    double_tap_window_ms: u32,

    /// Current active layer
    current_layer: Layer,
    /// Base layer remaps from config
    base_remaps: HashMap<KeyCode, ConfigAction>,
    /// All layer remaps from config
    layers: HashMap<Layer, HashMap<KeyCode, ConfigAction>>,

    /// Game mode state
    game_mode_active: bool,
    /// Game mode remaps from config
    game_mode_remaps: HashMap<KeyCode, ConfigAction>,

    /// Tapping term in ms (for OVERLOAD timing)
    tapping_term_ms: u32,
    /// Overload press times for timing calculation
    overload_press_times: HashMap<KeyCode, Instant>,
    /// Which OVERLOAD keys are pending (awaiting tap/hold decision)
    pending_overload: HashSet<KeyCode>,

    /// SOCD state tracking - maps each key to its group ID
    socd_key_to_group: HashMap<KeyCode, usize>,
    /// SOCD groups by ID
    socd_groups: Vec<SocdGroup>,
}

impl KeymapProcessor {
    /// Create a new keymap processor from config
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let mut layers = HashMap::new();
        for (layer, layer_config) in &config.layers {
            layers.insert(layer.clone(), layer_config.remaps.clone());
        }

        // Build SOCD groups from config
        // First, collect all SOCD definitions
        let mut socd_definitions: HashMap<KeyCode, Vec<KeyCode>> = HashMap::new();

        let extract_socd = |remaps: &HashMap<KeyCode, ConfigAction>,
                            defs: &mut HashMap<KeyCode, Vec<KeyCode>>| {
            for (_keycode, action) in remaps {
                if let ConfigAction::SOCD(this_key, opposing_keys) = action {
                    defs.insert(*this_key, opposing_keys.clone());
                }
            }
        };

        extract_socd(&config.remaps, &mut socd_definitions);
        for layer_config in config.layers.values() {
            extract_socd(&layer_config.remaps, &mut socd_definitions);
        }
        extract_socd(&config.game_mode.remaps, &mut socd_definitions);

        // Build groups: each key + its opposing keys form a group
        let mut socd_groups = Vec::new();
        let mut socd_key_to_group = HashMap::new();

        for (this_key, opposing_keys) in socd_definitions {
            // Build the full group: this_key + all opposing_keys
            let mut all_keys = vec![this_key];
            all_keys.extend(opposing_keys);

            let group_id = socd_groups.len();
            socd_groups.push(SocdGroup {
                all_keys: all_keys.clone(),
                held_stack: Vec::new(),
                active_key: None,
            });

            // Map each key in the group to this group ID
            for key in all_keys {
                socd_key_to_group.insert(key, group_id);
            }
        }

        Self {
            held_keys: HashMap::new(),
            pending_hrm: HashSet::new(),
            hrm_last_tap: HashMap::new(),
            double_tap_window_ms: config.double_tap_window_ms.unwrap_or(300),
            current_layer: Layer::base(),
            base_remaps: config.remaps.clone(),
            layers,
            game_mode_active: false,
            game_mode_remaps: config.game_mode.remaps.clone(),
            tapping_term_ms: config.tapping_term_ms,
            overload_press_times: HashMap::new(),
            pending_overload: HashSet::new(),
            socd_key_to_group,
            socd_groups,
        }
    }

    /// Set game mode state
    pub const fn set_game_mode(&mut self, active: bool) {
        self.game_mode_active = active;
    }

    /// Get all currently held keys (for graceful shutdown)
    pub fn get_held_keys(&self) -> Vec<KeyCode> {
        self.held_keys.keys().copied().collect()
    }

    /// Process a key event
    pub fn process_key(&mut self, keycode: KeyCode, pressed: bool) -> ProcessResult {
        if pressed {
            self.process_key_press(keycode)
        } else {
            self.process_key_release(keycode)
        }
    }

    fn process_key_press(&mut self, keycode: KeyCode) -> ProcessResult {
        let mut actions = Vec::new();

        // Look up action for this key
        let action = self.lookup_action(keycode);

        match action {
            Some(ConfigAction::Key(output_key)) => {
                // Simple key remap
                actions.push(KeyAction::RegularKey(output_key));
                self.held_keys.insert(keycode, actions);
                ProcessResult::EmitKey(output_key, true)
            }
            Some(ConfigAction::HR(tap_key, hold_key)) => {
                // Home row mod - check for double-tap first
                if self.is_double_tap(keycode) {
                    // Double-tap: hold the base key
                    actions.push(KeyAction::HomeRowModHoldingBase { base_key: tap_key });
                    self.held_keys.insert(keycode, actions);
                    ProcessResult::EmitKey(tap_key, true)
                } else {
                    // Check if ANY modifiers are pending (HR or OVERLOAD)
                    let has_pending = self.has_pending_hrm() || !self.pending_overload.is_empty();

                    if has_pending {
                        let mut events = Vec::new();

                        // Resolve all pending HRMs to hold
                        if self.has_pending_hrm() {
                            let mod_keys = self.resolve_pending_hrms_to_hold();
                            events.extend(mod_keys.into_iter().map(|k| (k, true)));
                        }

                        // Resolve all pending OVERLOAD keys to hold
                        for &pending_key in &self.pending_overload.clone() {
                            if let Some(held_actions) = self.held_keys.get_mut(&pending_key) {
                                for action in held_actions {
                                    if let KeyAction::OverloadPending {
                                        tap_key: _,
                                        hold_key,
                                    } = *action
                                    {
                                        self.pending_overload.remove(&pending_key);
                                        *action = KeyAction::OverloadHolding { hold_key };
                                        events.push((hold_key, true));
                                    }
                                }
                            }
                        }

                        // Mark this HR key as pending too
                        self.set_hrm_pending(keycode);
                        actions.push(KeyAction::HomeRowModPending { tap_key, hold_key });
                        self.held_keys.insert(keycode, actions);

                        // Emit all the resolved modifiers
                        ProcessResult::MultipleEvents(events)
                    } else {
                        // First modifier - mark as pending
                        self.set_hrm_pending(keycode);
                        actions.push(KeyAction::HomeRowModPending { tap_key, hold_key });
                        self.held_keys.insert(keycode, actions);
                        ProcessResult::None
                    }
                }
            }
            Some(ConfigAction::TO(layer)) => {
                // Layer switch
                self.current_layer = layer.clone();
                actions.push(KeyAction::Layer(layer));
                self.held_keys.insert(keycode, actions);
                ProcessResult::None
            }
            Some(ConfigAction::OVERLOAD(tap_key, hold_key)) => {
                // Overload - tap/hold with permissive hold logic
                // Record press time
                self.overload_press_times.insert(keycode, Instant::now());

                // Check for double-tap
                if self.is_double_tap_overload(keycode) {
                    // Double-tap: hold the base (tap) key
                    actions.push(KeyAction::OverloadHolding { hold_key: tap_key });
                    self.held_keys.insert(keycode, actions);
                    ProcessResult::EmitKey(tap_key, true)
                } else {
                    // Check if ANY modifiers are pending (HR or OVERLOAD)
                    let has_pending = self.has_pending_hrm() || !self.pending_overload.is_empty();

                    if has_pending {
                        let mut events = Vec::new();

                        // Resolve all pending HRMs to hold
                        if self.has_pending_hrm() {
                            let mod_keys = self.resolve_pending_hrms_to_hold();
                            events.extend(mod_keys.into_iter().map(|k| (k, true)));
                        }

                        // Resolve all pending OVERLOAD keys to hold
                        for &pending_key in &self.pending_overload.clone() {
                            if let Some(held_actions) = self.held_keys.get_mut(&pending_key) {
                                for action in held_actions {
                                    if let KeyAction::OverloadPending {
                                        tap_key: _,
                                        hold_key,
                                    } = *action
                                    {
                                        self.pending_overload.remove(&pending_key);
                                        *action = KeyAction::OverloadHolding { hold_key };
                                        events.push((hold_key, true));
                                    }
                                }
                            }
                        }

                        // Mark this OVERLOAD key as pending too
                        actions.push(KeyAction::OverloadPending { tap_key, hold_key });
                        self.held_keys.insert(keycode, actions);
                        self.pending_overload.insert(keycode);

                        // Emit all the resolved modifiers
                        ProcessResult::MultipleEvents(events)
                    } else {
                        // First modifier - mark as pending
                        actions.push(KeyAction::OverloadPending { tap_key, hold_key });
                        self.held_keys.insert(keycode, actions);
                        self.pending_overload.insert(keycode);
                        ProcessResult::None
                    }
                }
            }
            Some(ConfigAction::SOCD(this_key, _opposing_keys)) => {
                // SOCD handling
                actions.push(KeyAction::SocdManaged);
                self.held_keys.insert(keycode, actions);
                self.apply_socd_to_key_press(this_key)
            }
            Some(ConfigAction::CMD(command)) => {
                // Run arbitrary command
                ProcessResult::RunCommand(command.clone())
            }
            None => {
                // No remap - check if modifiers are pending (permissive hold)
                let has_pending_mods = self.has_pending_hrm() || !self.pending_overload.is_empty();

                if has_pending_mods {
                    let mut events: Vec<(KeyCode, bool)> = Vec::new();

                    // Resolve all pending HRMs to hold
                    if self.has_pending_hrm() {
                        let mod_keys = self.resolve_pending_hrms_to_hold();
                        events.extend(mod_keys.into_iter().map(|k| (k, true)));
                    }

                    // Resolve all pending OVERLOAD keys to hold
                    for &pending_key in &self.pending_overload.clone() {
                        if let Some(held_actions) = self.held_keys.get_mut(&pending_key) {
                            for action in held_actions {
                                // Extract hold_key before mutating action
                                if let KeyAction::OverloadPending {
                                    tap_key: _,
                                    hold_key,
                                } = *action
                                {
                                    // Resolve to hold
                                    self.pending_overload.remove(&pending_key);
                                    *action = KeyAction::OverloadHolding { hold_key };
                                    events.push((hold_key, true));
                                }
                            }
                        }
                    }

                    // Then emit this key
                    actions.push(KeyAction::RegularKey(keycode));
                    self.held_keys.insert(keycode, actions);
                    events.push((keycode, true));
                    ProcessResult::MultipleEvents(events)
                } else {
                    // Pass through unchanged
                    actions.push(KeyAction::RegularKey(keycode));
                    self.held_keys.insert(keycode, actions);
                    ProcessResult::EmitKey(keycode, true)
                }
            }
        }
    }

    fn process_key_release(&mut self, keycode: KeyCode) -> ProcessResult {
        if let Some(actions) = self.held_keys.remove(&keycode) {
            let mut events = Vec::new();

            for action in actions {
                match action {
                    KeyAction::RegularKey(key) => {
                        events.push((key, false));
                    }
                    KeyAction::Modifier(key) => {
                        events.push((key, false));
                    }
                    KeyAction::Layer(_prev_layer) => {
                        // Switch back to base layer
                        self.current_layer = Layer::base();
                    }
                    KeyAction::HomeRowModPending {
                        tap_key,
                        hold_key: _,
                    } => {
                        // Released while pending - tap it
                        self.clear_hrm_pending(keycode);
                        self.set_hrm_last_tap(keycode);
                        return ProcessResult::TapKeyPressRelease(tap_key);
                    }
                    KeyAction::HomeRowModHoldingBase { base_key } => {
                        // Release the base key
                        events.push((base_key, false));
                    }
                    KeyAction::OverloadPending { tap_key, hold_key } => {
                        // Remove from pending set
                        self.pending_overload.remove(&keycode);

                        // Check elapsed time to decide tap vs hold
                        if let Some(press_time) = self.overload_press_times.remove(&keycode) {
                            let elapsed =
                                Instant::now().duration_since(press_time).as_millis() as u32;

                            if elapsed < self.tapping_term_ms {
                                // Quick tap: emit tap key press+release
                                return ProcessResult::TapKeyPressRelease(tap_key);
                            }
                            // Held but never resolved by permissive hold - resolve now
                            events.push((hold_key, true));
                            events.push((hold_key, false));
                        }
                    }
                    KeyAction::OverloadHolding { hold_key } => {
                        // Already resolved to hold by permissive hold - just release
                        events.push((hold_key, false));
                    }
                    KeyAction::SocdManaged => {
                        // Apply SOCD release logic
                        return self.apply_socd_to_key_release(keycode);
                    }
                }
            }

            if events.is_empty() {
                ProcessResult::None
            } else if events.len() == 1 {
                ProcessResult::EmitKey(events[0].0, events[0].1)
            } else {
                ProcessResult::MultipleEvents(events)
            }
        } else {
            ProcessResult::None
        }
    }

    /// Look up action for a keycode on current layer
    fn lookup_action(&self, keycode: KeyCode) -> Option<ConfigAction> {
        // Check game mode first if active
        if self.game_mode_active {
            if let Some(action) = self.game_mode_remaps.get(&keycode) {
                return Some(action.clone());
            }
        }

        // Check current layer next (if not base)
        if !self.current_layer.is_base() {
            if let Some(layer_remaps) = self.layers.get(&self.current_layer) {
                if let Some(action) = layer_remaps.get(&keycode) {
                    return Some(action.clone());
                }
            }
        }

        // Fall back to base layer
        self.base_remaps.get(&keycode).cloned()
    }

    /// Resolve all pending HR mods to hold, return the modifier keys
    fn resolve_pending_hrms_to_hold(&mut self) -> Vec<KeyCode> {
        let mut mod_keys = Vec::new();

        // Clone the pending set to avoid borrow issues
        let pending_keys: Vec<KeyCode> = self.pending_hrm.iter().copied().collect();

        for keycode in pending_keys {
            if let Some(actions) = self.held_keys.get_mut(&keycode) {
                // Find the HomeRowModPending action and replace with Modifier
                for action in actions.iter_mut() {
                    if let KeyAction::HomeRowModPending { hold_key, .. } = action {
                        mod_keys.push(*hold_key);
                        *action = KeyAction::Modifier(*hold_key);
                        break;
                    }
                }
            }
            self.clear_hrm_pending(keycode);
        }

        mod_keys
    }

    // === Home Row Mod Helpers ===

    fn has_pending_hrm(&self) -> bool {
        !self.pending_hrm.is_empty()
    }

    fn set_hrm_pending(&mut self, keycode: KeyCode) {
        self.pending_hrm.insert(keycode);
    }

    fn clear_hrm_pending(&mut self, keycode: KeyCode) {
        self.pending_hrm.remove(&keycode);
    }

    fn is_double_tap(&self, keycode: KeyCode) -> bool {
        if let Some(last_tap) = self.hrm_last_tap.get(&keycode) {
            let elapsed = Instant::now().duration_since(*last_tap).as_millis() as u32;
            return elapsed < self.double_tap_window_ms;
        }
        false
    }

    fn set_hrm_last_tap(&mut self, keycode: KeyCode) {
        self.hrm_last_tap.insert(keycode, Instant::now());
    }

    /// Check if this is a double-tap for OVERLOAD (hold base key)
    fn is_double_tap_overload(&self, keycode: KeyCode) -> bool {
        if let Some(press_time) = self.overload_press_times.get(&keycode) {
            let elapsed = Instant::now().duration_since(*press_time).as_millis() as u32;
            return elapsed < self.double_tap_window_ms;
        }
        false
    }

    // === SOCD Helpers (Generic) ===

    /// Apply SOCD key press - uses stack-based last-input-priority
    fn apply_socd_to_key_press(&mut self, keycode: KeyCode) -> ProcessResult {
        // Find which SOCD group this key belongs to
        if let Some(&group_id) = self.socd_key_to_group.get(&keycode) {
            let group = &mut self.socd_groups[group_id];
            let old_active = group.active_key;

            // Add this key to the held stack (most recent at the end)
            if !group.held_stack.contains(&keycode) {
                group.held_stack.push(keycode);
            }

            // The most recent key becomes active
            let new_active = group.held_stack.last().copied();
            group.active_key = new_active;

            // Generate transition events
            self.generate_socd_transition(old_active, new_active)
        } else {
            ProcessResult::None
        }
    }

    /// Apply SOCD key release - activates the previous key in stack
    fn apply_socd_to_key_release(&mut self, keycode: KeyCode) -> ProcessResult {
        // Find which SOCD group this key belongs to
        if let Some(&group_id) = self.socd_key_to_group.get(&keycode) {
            let group = &mut self.socd_groups[group_id];
            let old_active = group.active_key;

            // Remove this key from the held stack
            group.held_stack.retain(|&k| k != keycode);

            // The most recent remaining key becomes active (or None if stack is empty)
            let new_active = group.held_stack.last().copied();
            group.active_key = new_active;

            // Generate transition events
            self.generate_socd_transition(old_active, new_active)
        } else {
            ProcessResult::None
        }
    }

    /// Generate transition events from old to new active key
    fn generate_socd_transition(
        &self,
        old_active: Option<KeyCode>,
        new_active: Option<KeyCode>,
    ) -> ProcessResult {
        match (old_active, new_active) {
            (None, None) => ProcessResult::None,
            (None, Some(new_key)) => {
                // Press new key
                ProcessResult::EmitKey(new_key, true)
            }
            (Some(old_key), None) => {
                // Release old key
                ProcessResult::EmitKey(old_key, false)
            }
            (Some(old_key), Some(new_key)) if old_key == new_key => {
                // Same key, no change
                ProcessResult::None
            }
            (Some(old_key), Some(new_key)) => {
                // Transition: release old, press new
                ProcessResult::MultipleEvents(vec![(old_key, false), (new_key, true)])
            }
        }
    }
}

// === ProcessResult ===

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessResult {
    /// Emit a single key event
    EmitKey(KeyCode, bool),
    /// Emit a tap (press + release)
    TapKeyPressRelease(KeyCode),
    /// Emit multiple events in sequence
    MultipleEvents(Vec<(KeyCode, bool)>),
    /// Type a string
    TypeString(String, bool),
    /// Run a shell command
    RunCommand(String),
    /// Don't emit anything
    None,
}

// === evdev ↔ KeyCode Conversion ===

#[must_use]
pub const fn evdev_to_keycode(key: Key) -> Option<KeyCode> {
    match key {
        // Letters
        Key::KEY_A => Some(KeyCode::KC_A),
        Key::KEY_B => Some(KeyCode::KC_B),
        Key::KEY_C => Some(KeyCode::KC_C),
        Key::KEY_D => Some(KeyCode::KC_D),
        Key::KEY_E => Some(KeyCode::KC_E),
        Key::KEY_F => Some(KeyCode::KC_F),
        Key::KEY_G => Some(KeyCode::KC_G),
        Key::KEY_H => Some(KeyCode::KC_H),
        Key::KEY_I => Some(KeyCode::KC_I),
        Key::KEY_J => Some(KeyCode::KC_J),
        Key::KEY_K => Some(KeyCode::KC_K),
        Key::KEY_L => Some(KeyCode::KC_L),
        Key::KEY_M => Some(KeyCode::KC_M),
        Key::KEY_N => Some(KeyCode::KC_N),
        Key::KEY_O => Some(KeyCode::KC_O),
        Key::KEY_P => Some(KeyCode::KC_P),
        Key::KEY_Q => Some(KeyCode::KC_Q),
        Key::KEY_R => Some(KeyCode::KC_R),
        Key::KEY_S => Some(KeyCode::KC_S),
        Key::KEY_T => Some(KeyCode::KC_T),
        Key::KEY_U => Some(KeyCode::KC_U),
        Key::KEY_V => Some(KeyCode::KC_V),
        Key::KEY_W => Some(KeyCode::KC_W),
        Key::KEY_X => Some(KeyCode::KC_X),
        Key::KEY_Y => Some(KeyCode::KC_Y),
        Key::KEY_Z => Some(KeyCode::KC_Z),

        // Numbers
        Key::KEY_1 => Some(KeyCode::KC_1),
        Key::KEY_2 => Some(KeyCode::KC_2),
        Key::KEY_3 => Some(KeyCode::KC_3),
        Key::KEY_4 => Some(KeyCode::KC_4),
        Key::KEY_5 => Some(KeyCode::KC_5),
        Key::KEY_6 => Some(KeyCode::KC_6),
        Key::KEY_7 => Some(KeyCode::KC_7),
        Key::KEY_8 => Some(KeyCode::KC_8),
        Key::KEY_9 => Some(KeyCode::KC_9),
        Key::KEY_0 => Some(KeyCode::KC_0),

        // Modifiers
        Key::KEY_LEFTCTRL => Some(KeyCode::KC_LCTL),
        Key::KEY_LEFTSHIFT => Some(KeyCode::KC_LSFT),
        Key::KEY_LEFTALT => Some(KeyCode::KC_LALT),
        Key::KEY_LEFTMETA => Some(KeyCode::KC_LGUI),
        Key::KEY_RIGHTCTRL => Some(KeyCode::KC_RCTL),
        Key::KEY_RIGHTSHIFT => Some(KeyCode::KC_RSFT),
        Key::KEY_RIGHTALT => Some(KeyCode::KC_RALT),
        Key::KEY_RIGHTMETA => Some(KeyCode::KC_RGUI),

        // Special keys
        Key::KEY_ESC => Some(KeyCode::KC_ESC),
        Key::KEY_CAPSLOCK => Some(KeyCode::KC_CAPS),
        Key::KEY_TAB => Some(KeyCode::KC_TAB),
        Key::KEY_SPACE => Some(KeyCode::KC_SPC),
        Key::KEY_ENTER => Some(KeyCode::KC_ENT),
        Key::KEY_BACKSPACE => Some(KeyCode::KC_BSPC),
        Key::KEY_DELETE => Some(KeyCode::KC_DEL),
        Key::KEY_GRAVE => Some(KeyCode::KC_GRV),
        Key::KEY_MINUS => Some(KeyCode::KC_MINS),
        Key::KEY_EQUAL => Some(KeyCode::KC_EQL),
        Key::KEY_LEFTBRACE => Some(KeyCode::KC_LBRC),
        Key::KEY_RIGHTBRACE => Some(KeyCode::KC_RBRC),
        Key::KEY_BACKSLASH => Some(KeyCode::KC_BSLS),
        Key::KEY_SEMICOLON => Some(KeyCode::KC_SCLN),
        Key::KEY_APOSTROPHE => Some(KeyCode::KC_QUOT),
        Key::KEY_COMMA => Some(KeyCode::KC_COMM),
        Key::KEY_DOT => Some(KeyCode::KC_DOT),
        Key::KEY_SLASH => Some(KeyCode::KC_SLSH),

        // Arrow keys
        Key::KEY_LEFT => Some(KeyCode::KC_LEFT),
        Key::KEY_DOWN => Some(KeyCode::KC_DOWN),
        Key::KEY_UP => Some(KeyCode::KC_UP),
        Key::KEY_RIGHT => Some(KeyCode::KC_RGHT),

        // Function keys
        Key::KEY_F1 => Some(KeyCode::KC_F1),
        Key::KEY_F2 => Some(KeyCode::KC_F2),
        Key::KEY_F3 => Some(KeyCode::KC_F3),
        Key::KEY_F4 => Some(KeyCode::KC_F4),
        Key::KEY_F5 => Some(KeyCode::KC_F5),
        Key::KEY_F6 => Some(KeyCode::KC_F6),
        Key::KEY_F7 => Some(KeyCode::KC_F7),
        Key::KEY_F8 => Some(KeyCode::KC_F8),
        Key::KEY_F9 => Some(KeyCode::KC_F9),
        Key::KEY_F10 => Some(KeyCode::KC_F10),
        Key::KEY_F11 => Some(KeyCode::KC_F11),
        Key::KEY_F12 => Some(KeyCode::KC_F12),
        Key::KEY_F13 => Some(KeyCode::KC_F13),
        Key::KEY_F14 => Some(KeyCode::KC_F14),
        Key::KEY_F15 => Some(KeyCode::KC_F15),
        Key::KEY_F16 => Some(KeyCode::KC_F16),
        Key::KEY_F17 => Some(KeyCode::KC_F17),
        Key::KEY_F18 => Some(KeyCode::KC_F18),
        Key::KEY_F19 => Some(KeyCode::KC_F19),
        Key::KEY_F20 => Some(KeyCode::KC_F20),
        Key::KEY_F21 => Some(KeyCode::KC_F21),
        Key::KEY_F22 => Some(KeyCode::KC_F22),
        Key::KEY_F23 => Some(KeyCode::KC_F23),
        Key::KEY_F24 => Some(KeyCode::KC_F24),

        // Navigation keys
        Key::KEY_PAGEUP => Some(KeyCode::KC_PGUP),
        Key::KEY_PAGEDOWN => Some(KeyCode::KC_PGDN),
        Key::KEY_HOME => Some(KeyCode::KC_HOME),
        Key::KEY_END => Some(KeyCode::KC_END),
        Key::KEY_INSERT => Some(KeyCode::KC_INS),
        Key::KEY_SYSRQ => Some(KeyCode::KC_PSCR),

        // Numpad keys
        Key::KEY_KP0 => Some(KeyCode::KC_KP_0),
        Key::KEY_KP1 => Some(KeyCode::KC_KP_1),
        Key::KEY_KP2 => Some(KeyCode::KC_KP_2),
        Key::KEY_KP3 => Some(KeyCode::KC_KP_3),
        Key::KEY_KP4 => Some(KeyCode::KC_KP_4),
        Key::KEY_KP5 => Some(KeyCode::KC_KP_5),
        Key::KEY_KP6 => Some(KeyCode::KC_KP_6),
        Key::KEY_KP7 => Some(KeyCode::KC_KP_7),
        Key::KEY_KP8 => Some(KeyCode::KC_KP_8),
        Key::KEY_KP9 => Some(KeyCode::KC_KP_9),
        Key::KEY_KPSLASH => Some(KeyCode::KC_KP_SLASH),
        Key::KEY_KPASTERISK => Some(KeyCode::KC_KP_ASTERISK),
        Key::KEY_KPMINUS => Some(KeyCode::KC_KP_MINUS),
        Key::KEY_KPPLUS => Some(KeyCode::KC_KP_PLUS),
        Key::KEY_KPENTER => Some(KeyCode::KC_KP_ENTER),
        Key::KEY_KPDOT => Some(KeyCode::KC_KP_DOT),
        Key::KEY_NUMLOCK => Some(KeyCode::KC_NUM_LOCK),

        // Media keys
        Key::KEY_MUTE => Some(KeyCode::KC_MUTE),
        Key::KEY_VOLUMEUP => Some(KeyCode::KC_VOL_UP),
        Key::KEY_VOLUMEDOWN => Some(KeyCode::KC_VOL_DN),
        Key::KEY_PLAYPAUSE => Some(KeyCode::KC_MEDIA_PLAY_PAUSE),
        Key::KEY_STOPCD => Some(KeyCode::KC_MEDIA_STOP),
        Key::KEY_NEXTSONG => Some(KeyCode::KC_MEDIA_NEXT_TRACK),
        Key::KEY_PREVIOUSSONG => Some(KeyCode::KC_MEDIA_PREV_TRACK),
        Key::KEY_MEDIA => Some(KeyCode::KC_MEDIA_SELECT),

        // System keys
        Key::KEY_POWER => Some(KeyCode::KC_PWR),
        Key::KEY_SLEEP => Some(KeyCode::KC_SLEP),
        Key::KEY_WAKEUP => Some(KeyCode::KC_WAKE),
        Key::KEY_CALC => Some(KeyCode::KC_CALC),
        Key::KEY_COMPUTER => Some(KeyCode::KC_MY_COMP),
        Key::KEY_SEARCH => Some(KeyCode::KC_WWW_SEARCH),
        Key::KEY_HOMEPAGE => Some(KeyCode::KC_WWW_HOME),
        Key::KEY_BACK => Some(KeyCode::KC_WWW_BACK),
        Key::KEY_FORWARD => Some(KeyCode::KC_WWW_FORWARD),
        Key::KEY_STOP => Some(KeyCode::KC_WWW_STOP),
        Key::KEY_REFRESH => Some(KeyCode::KC_WWW_REFRESH),
        Key::KEY_BOOKMARKS => Some(KeyCode::KC_WWW_FAVORITES),

        // Locking keys
        Key::KEY_SCROLLLOCK => Some(KeyCode::KC_SCRL),
        Key::KEY_PAUSE => Some(KeyCode::KC_PAUS),

        // Application keys
        Key::KEY_PROPS => Some(KeyCode::KC_APP),
        Key::KEY_MENU => Some(KeyCode::KC_MENU),

        // Multimedia keys
        Key::KEY_BRIGHTNESSUP => Some(KeyCode::KC_BRIU),
        Key::KEY_BRIGHTNESSDOWN => Some(KeyCode::KC_BRID),
        Key::KEY_DISPLAYTOGGLE => Some(KeyCode::KC_DISPLAY_OFF),
        Key::KEY_WLAN => Some(KeyCode::KC_WLAN),
        Key::KEY_BLUETOOTH => Some(KeyCode::KC_BLUETOOTH),
        Key::KEY_SWITCHVIDEOMODE => Some(KeyCode::KC_KEYBOARD_LAYOUT),

        // International keys
        Key::KEY_102ND => Some(KeyCode::KC_INTL_BACKSLASH),
        Key::KEY_YEN => Some(KeyCode::KC_INTL_YEN),
        Key::KEY_RO => Some(KeyCode::KC_INTL_RO),

        _ => None,
    }
}

#[must_use]
pub const fn keycode_to_evdev(keycode: KeyCode) -> Key {
    match keycode {
        // Letters
        KeyCode::KC_A => Key::KEY_A,
        KeyCode::KC_B => Key::KEY_B,
        KeyCode::KC_C => Key::KEY_C,
        KeyCode::KC_D => Key::KEY_D,
        KeyCode::KC_E => Key::KEY_E,
        KeyCode::KC_F => Key::KEY_F,
        KeyCode::KC_G => Key::KEY_G,
        KeyCode::KC_H => Key::KEY_H,
        KeyCode::KC_I => Key::KEY_I,
        KeyCode::KC_J => Key::KEY_J,
        KeyCode::KC_K => Key::KEY_K,
        KeyCode::KC_L => Key::KEY_L,
        KeyCode::KC_M => Key::KEY_M,
        KeyCode::KC_N => Key::KEY_N,
        KeyCode::KC_O => Key::KEY_O,
        KeyCode::KC_P => Key::KEY_P,
        KeyCode::KC_Q => Key::KEY_Q,
        KeyCode::KC_R => Key::KEY_R,
        KeyCode::KC_S => Key::KEY_S,
        KeyCode::KC_T => Key::KEY_T,
        KeyCode::KC_U => Key::KEY_U,
        KeyCode::KC_V => Key::KEY_V,
        KeyCode::KC_W => Key::KEY_W,
        KeyCode::KC_X => Key::KEY_X,
        KeyCode::KC_Y => Key::KEY_Y,
        KeyCode::KC_Z => Key::KEY_Z,

        // Numbers
        KeyCode::KC_1 => Key::KEY_1,
        KeyCode::KC_2 => Key::KEY_2,
        KeyCode::KC_3 => Key::KEY_3,
        KeyCode::KC_4 => Key::KEY_4,
        KeyCode::KC_5 => Key::KEY_5,
        KeyCode::KC_6 => Key::KEY_6,
        KeyCode::KC_7 => Key::KEY_7,
        KeyCode::KC_8 => Key::KEY_8,
        KeyCode::KC_9 => Key::KEY_9,
        KeyCode::KC_0 => Key::KEY_0,

        // Modifiers
        KeyCode::KC_LCTL => Key::KEY_LEFTCTRL,
        KeyCode::KC_LSFT => Key::KEY_LEFTSHIFT,
        KeyCode::KC_LALT => Key::KEY_LEFTALT,
        KeyCode::KC_LGUI | KeyCode::KC_LCMD => Key::KEY_LEFTMETA, // KC_LCMD is alias
        KeyCode::KC_RCTL => Key::KEY_RIGHTCTRL,
        KeyCode::KC_RSFT => Key::KEY_RIGHTSHIFT,
        KeyCode::KC_RALT => Key::KEY_RIGHTALT,
        KeyCode::KC_RGUI | KeyCode::KC_RCMD => Key::KEY_RIGHTMETA, // KC_RCMD is alias

        // Special keys
        KeyCode::KC_ESC => Key::KEY_ESC,
        KeyCode::KC_CAPS => Key::KEY_CAPSLOCK,
        KeyCode::KC_TAB => Key::KEY_TAB,
        KeyCode::KC_SPC => Key::KEY_SPACE,
        KeyCode::KC_ENT => Key::KEY_ENTER,
        KeyCode::KC_BSPC => Key::KEY_BACKSPACE,
        KeyCode::KC_DEL => Key::KEY_DELETE,
        KeyCode::KC_GRV => Key::KEY_GRAVE,
        KeyCode::KC_MINS => Key::KEY_MINUS,
        KeyCode::KC_EQL => Key::KEY_EQUAL,
        KeyCode::KC_LBRC => Key::KEY_LEFTBRACE,
        KeyCode::KC_RBRC => Key::KEY_RIGHTBRACE,
        KeyCode::KC_BSLS => Key::KEY_BACKSLASH,
        KeyCode::KC_SCLN => Key::KEY_SEMICOLON,
        KeyCode::KC_QUOT => Key::KEY_APOSTROPHE,
        KeyCode::KC_COMM => Key::KEY_COMMA,
        KeyCode::KC_DOT => Key::KEY_DOT,
        KeyCode::KC_SLSH => Key::KEY_SLASH,

        // Arrow keys
        KeyCode::KC_LEFT => Key::KEY_LEFT,
        KeyCode::KC_DOWN => Key::KEY_DOWN,
        KeyCode::KC_UP => Key::KEY_UP,
        KeyCode::KC_RGHT => Key::KEY_RIGHT,

        // Function keys
        KeyCode::KC_F1 => Key::KEY_F1,
        KeyCode::KC_F2 => Key::KEY_F2,
        KeyCode::KC_F3 => Key::KEY_F3,
        KeyCode::KC_F4 => Key::KEY_F4,
        KeyCode::KC_F5 => Key::KEY_F5,
        KeyCode::KC_F6 => Key::KEY_F6,
        KeyCode::KC_F7 => Key::KEY_F7,
        KeyCode::KC_F8 => Key::KEY_F8,
        KeyCode::KC_F9 => Key::KEY_F9,
        KeyCode::KC_F10 => Key::KEY_F10,
        KeyCode::KC_F11 => Key::KEY_F11,
        KeyCode::KC_F12 => Key::KEY_F12,
        KeyCode::KC_F13 => Key::KEY_F13,
        KeyCode::KC_F14 => Key::KEY_F14,
        KeyCode::KC_F15 => Key::KEY_F15,
        KeyCode::KC_F16 => Key::KEY_F16,
        KeyCode::KC_F17 => Key::KEY_F17,
        KeyCode::KC_F18 => Key::KEY_F18,
        KeyCode::KC_F19 => Key::KEY_F19,
        KeyCode::KC_F20 => Key::KEY_F20,
        KeyCode::KC_F21 => Key::KEY_F21,
        KeyCode::KC_F22 => Key::KEY_F22,
        KeyCode::KC_F23 => Key::KEY_F23,
        KeyCode::KC_F24 => Key::KEY_F24,

        // Navigation keys
        KeyCode::KC_PGUP => Key::KEY_PAGEUP,
        KeyCode::KC_PGDN => Key::KEY_PAGEDOWN,
        KeyCode::KC_HOME => Key::KEY_HOME,
        KeyCode::KC_END => Key::KEY_END,
        KeyCode::KC_INS => Key::KEY_INSERT,
        KeyCode::KC_PSCR => Key::KEY_SYSRQ,

        // Numpad keys
        KeyCode::KC_KP_0 => Key::KEY_KP0,
        KeyCode::KC_KP_1 => Key::KEY_KP1,
        KeyCode::KC_KP_2 => Key::KEY_KP2,
        KeyCode::KC_KP_3 => Key::KEY_KP3,
        KeyCode::KC_KP_4 => Key::KEY_KP4,
        KeyCode::KC_KP_5 => Key::KEY_KP5,
        KeyCode::KC_KP_6 => Key::KEY_KP6,
        KeyCode::KC_KP_7 => Key::KEY_KP7,
        KeyCode::KC_KP_8 => Key::KEY_KP8,
        KeyCode::KC_KP_9 => Key::KEY_KP9,
        KeyCode::KC_KP_SLASH => Key::KEY_KPSLASH,
        KeyCode::KC_KP_ASTERISK => Key::KEY_KPASTERISK,
        KeyCode::KC_KP_MINUS => Key::KEY_KPMINUS,
        KeyCode::KC_KP_PLUS => Key::KEY_KPPLUS,
        KeyCode::KC_KP_ENTER => Key::KEY_KPENTER,
        KeyCode::KC_KP_DOT => Key::KEY_KPDOT,
        KeyCode::KC_NUM_LOCK => Key::KEY_NUMLOCK,

        // Media keys
        KeyCode::KC_MUTE => Key::KEY_MUTE,
        KeyCode::KC_VOL_UP => Key::KEY_VOLUMEUP,
        KeyCode::KC_VOL_DN => Key::KEY_VOLUMEDOWN,
        KeyCode::KC_MEDIA_PLAY_PAUSE => Key::KEY_PLAYPAUSE,
        KeyCode::KC_MEDIA_STOP => Key::KEY_STOPCD,
        KeyCode::KC_MEDIA_NEXT_TRACK => Key::KEY_NEXTSONG,
        KeyCode::KC_MEDIA_PREV_TRACK => Key::KEY_PREVIOUSSONG,
        KeyCode::KC_MEDIA_SELECT => Key::KEY_MEDIA,

        // System keys
        KeyCode::KC_PWR => Key::KEY_POWER,
        KeyCode::KC_SLEP => Key::KEY_SLEEP,
        KeyCode::KC_WAKE => Key::KEY_WAKEUP,
        KeyCode::KC_CALC => Key::KEY_CALC,
        KeyCode::KC_MY_COMP => Key::KEY_COMPUTER,
        KeyCode::KC_WWW_SEARCH => Key::KEY_SEARCH,
        KeyCode::KC_WWW_HOME => Key::KEY_HOMEPAGE,
        KeyCode::KC_WWW_BACK => Key::KEY_BACK,
        KeyCode::KC_WWW_FORWARD => Key::KEY_FORWARD,
        KeyCode::KC_WWW_STOP => Key::KEY_STOP,
        KeyCode::KC_WWW_REFRESH => Key::KEY_REFRESH,
        KeyCode::KC_WWW_FAVORITES => Key::KEY_BOOKMARKS,

        // Locking keys
        KeyCode::KC_SCRL => Key::KEY_SCROLLLOCK,
        KeyCode::KC_PAUS => Key::KEY_PAUSE,

        // Application keys
        KeyCode::KC_APP => Key::KEY_PROPS,
        KeyCode::KC_MENU => Key::KEY_MENU,

        // Multimedia keys
        KeyCode::KC_BRIU => Key::KEY_BRIGHTNESSUP,
        KeyCode::KC_BRID => Key::KEY_BRIGHTNESSDOWN,
        KeyCode::KC_DISPLAY_OFF => Key::KEY_DISPLAYTOGGLE,
        KeyCode::KC_WLAN => Key::KEY_WLAN,
        KeyCode::KC_BLUETOOTH => Key::KEY_BLUETOOTH,
        KeyCode::KC_KEYBOARD_LAYOUT => Key::KEY_SWITCHVIDEOMODE,

        // International keys
        KeyCode::KC_INTL_BACKSLASH => Key::KEY_102ND,
        KeyCode::KC_INTL_YEN => Key::KEY_YEN,
        KeyCode::KC_INTL_RO => Key::KEY_RO,
    }
}
