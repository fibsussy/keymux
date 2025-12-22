use evdev::Key;
use std::collections::HashMap;
use std::time::Instant;

use crate::config::{Action as ConfigAction, Config, KeyCode, Layer, Passwords};

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

/// QMK-inspired keymap processor
pub struct KeymapProcessor {
    /// What each physical key is currently doing (indexed by `KeyCode`)
    held_keys: HashMap<KeyCode, Vec<KeyAction>>,
    /// Which home row mods are pending (bit flags for efficiency)
    pending_hrm: u8,
    /// Last tap time for each HR mod (for double-tap detection)
    hrm_last_tap: [Option<Instant>; 8],
    /// Double-tap window in ms
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

    /// SOCD state tracking
    socd_w_held: bool,
    socd_s_held: bool,
    socd_a_held: bool,
    socd_d_held: bool,
    socd_last_vertical: Option<KeyCode>,
    socd_last_horizontal: Option<KeyCode>,
    socd_active_keys: [Option<KeyCode>; 2], // [vertical, horizontal]

    /// Password from config
    password: Option<String>,
    /// Last password key tap time
    password_last_tap: Option<Instant>,
}

impl KeymapProcessor {
    /// Create a new keymap processor from config
    #[must_use] 
    pub fn new(config: &Config) -> Self {
        let mut layers = HashMap::new();
        for (layer, layer_config) in &config.layers {
            layers.insert(*layer, layer_config.remaps.clone());
        }

        // Load password from separate file
        let password = Passwords::default_path()
            .ok()
            .and_then(|path| Passwords::load(&path).ok())
            .flatten();

        Self {
            held_keys: HashMap::new(),
            pending_hrm: 0,
            hrm_last_tap: [None; 8],
            double_tap_window_ms: config.double_tap_window_ms.unwrap_or(300),
            current_layer: Layer::L_BASE,
            base_remaps: config.remaps.clone(),
            layers,
            game_mode_active: false,
            game_mode_remaps: config.game_mode.remaps.clone(),
            tapping_term_ms: config.tapping_term_ms,
            overload_press_times: HashMap::new(),
            socd_w_held: false,
            socd_s_held: false,
            socd_a_held: false,
            socd_d_held: false,
            socd_last_vertical: None,
            socd_last_horizontal: None,
            socd_active_keys: [None; 2],
            password,
            password_last_tap: None,
        }
    }

    /// Set game mode state
    pub const fn set_game_mode(&mut self, active: bool) {
        self.game_mode_active = active;
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
                    // Check if any other HR mod is pending
                    if self.has_pending_hrm() {
                        // Resolve all pending HRMs to hold
                        let mod_keys = self.resolve_pending_hrms_to_hold();
                        // Mark this one as pending too
                        self.set_hrm_pending(keycode);
                        actions.push(KeyAction::HomeRowModPending { tap_key, hold_key });
                        self.held_keys.insert(keycode, actions);

                        // Emit all the modifiers
                        ProcessResult::MultipleEvents(
                            mod_keys.into_iter().map(|k| (k, true)).collect()
                        )
                    } else {
                        // First HR mod - mark as pending
                        self.set_hrm_pending(keycode);
                        actions.push(KeyAction::HomeRowModPending { tap_key, hold_key });
                        self.held_keys.insert(keycode, actions);
                        ProcessResult::None
                    }
                }
            }
            Some(ConfigAction::TO(layer)) => {
                // Layer switch
                self.current_layer = layer;
                actions.push(KeyAction::Layer(layer));
                self.held_keys.insert(keycode, actions);
                ProcessResult::None
            }
            Some(ConfigAction::OVERLOAD(tap_key, hold_key)) => {
                // Overload - simple tap/hold without permissive hold logic
                // Record press time
                self.overload_press_times.insert(keycode, Instant::now());

                // Check for double-tap
                if self.is_double_tap_overload(keycode) {
                    // Double-tap: hold the base (tap) key
                    actions.push(KeyAction::OverloadHolding { hold_key: tap_key });
                    self.held_keys.insert(keycode, actions);
                    ProcessResult::EmitKey(tap_key, true)
                } else {
                    // Pending - emit hold key immediately
                    actions.push(KeyAction::OverloadPending { tap_key, hold_key });
                    self.held_keys.insert(keycode, actions);
                    ProcessResult::EmitKey(hold_key, true)
                }
            }
            Some(ConfigAction::Socd(key1, _key2)) => {
                // SOCD handling
                actions.push(KeyAction::SocdManaged);
                self.held_keys.insert(keycode, actions);
                self.apply_socd_to_key_press(key1)
            }
            Some(ConfigAction::Password) => {
                // Password typer with double-tap for Enter only
                let is_double_tap = if let Some(last_tap) = self.password_last_tap {
                    let elapsed = Instant::now().duration_since(last_tap).as_millis() as u32;
                    elapsed < self.double_tap_window_ms
                } else {
                    false
                };
                self.password_last_tap = Some(Instant::now());

                if is_double_tap {
                    // Second tap: just press Enter
                    ProcessResult::TapKeyPressRelease(KeyCode::KC_ENT)
                } else {
                    // First tap: type password
                    self.password.as_ref().map_or(ProcessResult::None, |password| {
                        ProcessResult::TypeString(password.clone(), false)
                    })
                }
            }
            None => {
                // No remap - check if another key is pressed while HR mod pending (permissive hold)
                if self.has_pending_hrm() && !self.is_hrm_key(keycode) {
                    // Resolve all pending HRMs to hold
                    let mod_keys = self.resolve_pending_hrms_to_hold();

                    // Then emit this key
                    actions.push(KeyAction::RegularKey(keycode));
                    self.held_keys.insert(keycode, actions);

                    let mut events: Vec<(KeyCode, bool)> = mod_keys.into_iter().map(|k| (k, true)).collect();
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
                        self.current_layer = Layer::L_BASE;
                    }
                    KeyAction::HomeRowModPending { tap_key, hold_key: _ } => {
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
                        // Check elapsed time to decide tap vs hold
                        if let Some(press_time) = self.overload_press_times.remove(&keycode) {
                            let elapsed = Instant::now().duration_since(press_time).as_millis() as u32;

                            if elapsed < self.tapping_term_ms {
                                // Quick tap: release hold, tap the tap_key
                                events.push((hold_key, false));
                                return ProcessResult::TapKeyPressRelease(tap_key);
                            }
                            // Held: just release hold
                        }
                        events.push((hold_key, false));
                    }
                    KeyAction::OverloadHolding { hold_key } => {
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
        if self.current_layer != Layer::L_BASE {
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

        for bit in 0..8 {
            if (self.pending_hrm & (1 << bit)) != 0 {
                // This HR mod is pending - resolve it
                if let Some(keycode) = hrm_bit_to_keycode(bit) {
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
            }
        }

        mod_keys
    }

    // === Home Row Mod Helpers ===

    const fn is_hrm_key(&self, keycode: KeyCode) -> bool {
        matches!(
            keycode,
            KeyCode::KC_A | KeyCode::KC_S | KeyCode::KC_D | KeyCode::KC_F |
            KeyCode::KC_J | KeyCode::KC_K | KeyCode::KC_L | KeyCode::KC_SCLN
        )
    }

    const fn has_pending_hrm(&self) -> bool {
        self.pending_hrm != 0
    }

    const fn set_hrm_pending(&mut self, keycode: KeyCode) {
        if let Some(bit) = keycode_to_hrm_bit(keycode) {
            self.pending_hrm |= 1 << bit;
        }
    }

    const fn clear_hrm_pending(&mut self, keycode: KeyCode) {
        if let Some(bit) = keycode_to_hrm_bit(keycode) {
            self.pending_hrm &= !(1 << bit);
        }
    }

    fn is_double_tap(&self, keycode: KeyCode) -> bool {
        if let Some(bit) = keycode_to_hrm_bit(keycode) {
            if let Some(last_tap) = self.hrm_last_tap[bit as usize] {
                let elapsed = Instant::now().duration_since(last_tap).as_millis() as u32;
                return elapsed < self.double_tap_window_ms;
            }
        }
        false
    }

    fn set_hrm_last_tap(&mut self, keycode: KeyCode) {
        if let Some(bit) = keycode_to_hrm_bit(keycode) {
            self.hrm_last_tap[bit as usize] = Some(Instant::now());
        }
    }

    /// Check if this is a double-tap for OVERLOAD (hold base key)
    fn is_double_tap_overload(&self, keycode: KeyCode) -> bool {
        if let Some(press_time) = self.overload_press_times.get(&keycode) {
            let elapsed = Instant::now().duration_since(*press_time).as_millis() as u32;
            return elapsed < self.double_tap_window_ms;
        }
        false
    }

    // === SOCD Helpers ===

    /// Handle SOCD key press, returns new active keys [vertical, horizontal]
    const fn socd_handle_press(&mut self, keycode: KeyCode) -> [Option<KeyCode>; 2] {
        match keycode {
            KeyCode::KC_W => {
                self.socd_w_held = true;
                self.socd_last_vertical = Some(KeyCode::KC_W);
            }
            KeyCode::KC_A => {
                self.socd_a_held = true;
                self.socd_last_horizontal = Some(KeyCode::KC_A);
            }
            KeyCode::KC_S => {
                self.socd_s_held = true;
                self.socd_last_vertical = Some(KeyCode::KC_S);
            }
            KeyCode::KC_D => {
                self.socd_d_held = true;
                self.socd_last_horizontal = Some(KeyCode::KC_D);
            }
            _ => {}
        }
        self.compute_socd_active_keys()
    }

    /// Handle SOCD key release, returns new active keys [vertical, horizontal]
    const fn socd_handle_release(&mut self, keycode: KeyCode) -> [Option<KeyCode>; 2] {
        match keycode {
            KeyCode::KC_W => self.socd_w_held = false,
            KeyCode::KC_A => self.socd_a_held = false,
            KeyCode::KC_S => self.socd_s_held = false,
            KeyCode::KC_D => self.socd_d_held = false,
            _ => {}
        }
        self.compute_socd_active_keys()
    }

    /// Compute which SOCD keys should be active based on held state
    const fn compute_socd_active_keys(&mut self) -> [Option<KeyCode>; 2] {
        // Index 0 = vertical key, 1 = horizontal key

        // Vertical resolution (using last input priority)
        if self.socd_w_held && !self.socd_s_held {
            self.socd_active_keys[0] = Some(KeyCode::KC_W);
        } else if self.socd_s_held && !self.socd_w_held {
            self.socd_active_keys[0] = Some(KeyCode::KC_S);
        } else if self.socd_w_held && self.socd_s_held {
            // Both held: last input wins
            self.socd_active_keys[0] = self.socd_last_vertical;
        } else {
            self.socd_active_keys[0] = None;
        }

        // Horizontal resolution (using last input priority)
        if self.socd_a_held && !self.socd_d_held {
            self.socd_active_keys[1] = Some(KeyCode::KC_A);
        } else if self.socd_d_held && !self.socd_a_held {
            self.socd_active_keys[1] = Some(KeyCode::KC_D);
        } else if self.socd_a_held && self.socd_d_held {
            // Both held: last input wins
            self.socd_active_keys[1] = self.socd_last_horizontal;
        } else {
            self.socd_active_keys[1] = None;
        }

        self.socd_active_keys
    }

    /// Apply SOCD key press - compute new active keys and return events to emit
    fn apply_socd_to_key_press(&mut self, keycode: KeyCode) -> ProcessResult {
        let old_keys = self.socd_active_keys;
        let new_keys = self.socd_handle_press(keycode);
        self.generate_socd_events(old_keys, new_keys)
    }

    /// Apply SOCD key release - compute new active keys and return events to emit
    fn apply_socd_to_key_release(&mut self, keycode: KeyCode) -> ProcessResult {
        let old_keys = self.socd_active_keys;
        let new_keys = self.socd_handle_release(keycode);
        self.generate_socd_events(old_keys, new_keys)
    }

    /// Generate events to transition from `old_keys` to `new_keys`
    fn generate_socd_events(&self, old_keys: [Option<KeyCode>; 2], new_keys: [Option<KeyCode>; 2]) -> ProcessResult {
        let mut events = Vec::new();

        // Release keys that are no longer active
        for old_key in old_keys.iter().flatten() {
            // Check if this key is still in new_keys
            if !new_keys.contains(&Some(*old_key)) {
                events.push((*old_key, false));
            }
        }

        // Press keys that are newly active
        for new_key in new_keys.iter().flatten() {
            // Check if this key was already active
            if !old_keys.contains(&Some(*new_key)) {
                events.push((*new_key, true));
            }
        }

        if events.is_empty() {
            ProcessResult::None
        } else if events.len() == 1 {
            ProcessResult::EmitKey(events[0].0, events[0].1)
        } else {
            ProcessResult::MultipleEvents(events)
        }
    }
}

// === HR Mod Bit Mapping ===

const fn keycode_to_hrm_bit(keycode: KeyCode) -> Option<u8> {
    match keycode {
        KeyCode::KC_A => Some(0),
        KeyCode::KC_S => Some(1),
        KeyCode::KC_D => Some(2),
        KeyCode::KC_F => Some(3),
        KeyCode::KC_J => Some(4),
        KeyCode::KC_K => Some(5),
        KeyCode::KC_L => Some(6),
        KeyCode::KC_SCLN => Some(7),
        _ => None,
    }
}

const fn hrm_bit_to_keycode(bit: u8) -> Option<KeyCode> {
    match bit {
        0 => Some(KeyCode::KC_A),
        1 => Some(KeyCode::KC_S),
        2 => Some(KeyCode::KC_D),
        3 => Some(KeyCode::KC_F),
        4 => Some(KeyCode::KC_J),
        5 => Some(KeyCode::KC_K),
        6 => Some(KeyCode::KC_L),
        7 => Some(KeyCode::KC_SCLN),
        _ => None,
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
        KeyCode::KC_LGUI => Key::KEY_LEFTMETA,
        KeyCode::KC_RCTL => Key::KEY_RIGHTCTRL,
        KeyCode::KC_RSFT => Key::KEY_RIGHTSHIFT,
        KeyCode::KC_RALT => Key::KEY_RIGHTALT,
        KeyCode::KC_RGUI => Key::KEY_RIGHTMETA,

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
    }
}
