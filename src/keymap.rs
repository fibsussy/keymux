use evdev::Key;
use std::collections::HashMap;
use tracing::warn;

use crate::config::{Action as ConfigAction, Config, KeyCode, Layer};
use crate::modtap::{MtAction, MtConfig as ModtapConfig, MtProcessor, MtResolution, RollingStats};

/// What a key press is doing (recorded on press, replayed on release)
#[derive(Debug, Clone)]
enum KeyAction {
    /// Emitted a regular key
    RegularKey(KeyCode),
    /// Activated a modifier
    Modifier(KeyCode),
    /// Layer activation
    Layer(Layer),
    /// MT key managed by MT processor
    MtManaged,
    /// SOCD managed key
    SocdManaged,
}

/// SOCD group configuration
#[derive(Debug, Clone)]
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

    /// MT (Mod-Tap) processor
    mt_processor: MtProcessor,

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

    /// SOCD state tracking - maps each key to its group ID
    socd_key_to_group: HashMap<KeyCode, usize>,
    /// SOCD groups by ID
    socd_groups: Vec<SocdGroup>,

    /// Track ALL keyboard key statistics (100% coverage)
    all_key_stats: HashMap<KeyCode, RollingStats>,
    /// Track when each key was pressed (for measuring tap duration)
    key_press_times: HashMap<KeyCode, std::time::Instant>,
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
        let mut socd_definitions: HashMap<KeyCode, Vec<KeyCode>> = HashMap::new();

        let extract_socd = |remaps: &HashMap<KeyCode, ConfigAction>,
                            defs: &mut HashMap<KeyCode, Vec<KeyCode>>| {
            for (_keycode, action) in remaps {
                if let ConfigAction::SOCD(this_action, opposing_actions) = action {
                    // Extract KeyCode from Action (only support Key actions for now)
                    if let ConfigAction::Key(this_key) = this_action.as_ref() {
                        let mut opposing_keys = Vec::new();
                        for opp_action in opposing_actions {
                            if let ConfigAction::Key(opp_key) = opp_action.as_ref() {
                                opposing_keys.push(*opp_key);
                            }
                        }
                        if !opposing_keys.is_empty() {
                            defs.insert(*this_key, opposing_keys);
                        }
                    }
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
            let mut all_keys = vec![this_key];
            all_keys.extend(opposing_keys);

            let group_id = socd_groups.len();
            socd_groups.push(SocdGroup {
                all_keys: all_keys.clone(),
                held_stack: Vec::new(),
                active_key: None,
            });

            for key in all_keys {
                socd_key_to_group.insert(key, group_id);
            }
        }

        // Build MT processor config
        let mt_config = ModtapConfig {
            tapping_term_ms: config.tapping_term_ms,
            permissive_hold: config.mt_config.permissive_hold,
            same_hand_roll_detection: config.mt_config.same_hand_roll_detection,
            opposite_hand_chord_detection: config.mt_config.opposite_hand_chord_detection,
            multi_mod_detection: config.mt_config.multi_mod_detection,
            multi_mod_threshold: config.mt_config.multi_mod_threshold,
            adaptive_timing: config.mt_config.adaptive_timing,
            predictive_scoring: config.mt_config.predictive_scoring,
            roll_detection_window_ms: config.mt_config.roll_detection_window_ms,
            chord_detection_window_ms: config.mt_config.chord_detection_window_ms,
            double_tap_then_hold: config.mt_config.double_tap_then_hold,
            double_tap_window_ms: config.mt_config.double_tap_window_ms,
            cross_hand_unwrap: config.mt_config.cross_hand_unwrap,
            adaptive_target_margin_ms: config.mt_config.adaptive_target_margin_ms,
        };

        Self {
            held_keys: HashMap::new(),
            mt_processor: MtProcessor::new(mt_config),
            current_layer: Layer::base(),
            base_remaps: config.remaps.clone(),
            layers,
            game_mode_active: false,
            game_mode_remaps: config.game_mode.remaps.clone(),
            socd_key_to_group,
            socd_groups,
            all_key_stats: HashMap::new(),
            key_press_times: HashMap::new(),
        }
    }

    /// Set game mode state
    pub fn set_game_mode(&mut self, active: bool) {
        self.game_mode_active = active;
        self.mt_processor.set_game_mode(active);
    }

    /// Get all currently held keys (for graceful shutdown)
    pub fn get_held_keys(&self) -> Vec<KeyCode> {
        self.held_keys.keys().copied().collect()
    }

    /// Save adaptive timing stats to disk for a specific user
    pub fn save_adaptive_stats(&self, user_id: u32) -> Result<(), std::io::Error> {
        let home = Self::get_user_home(user_id);

        // Save MT stats
        let mt_path = std::path::PathBuf::from(format!(
            "{}/.config/keyboard-middleware/adaptive_stats.json",
            home
        ));
        self.mt_processor.save_stats(&mt_path)?;

        // Save ALL key stats
        let all_path = std::path::PathBuf::from(format!(
            "{}/.config/keyboard-middleware/all_key_stats.json",
            home
        ));
        self.save_all_key_stats(&all_path)?;

        Ok(())
    }

    /// Load adaptive timing stats from disk for a specific user
    pub fn load_adaptive_stats(&mut self, user_id: u32) -> Result<(), std::io::Error> {
        let home = Self::get_user_home(user_id);

        // Load MT stats
        let mt_path = std::path::PathBuf::from(format!(
            "{}/.config/keyboard-middleware/adaptive_stats.json",
            home
        ));
        self.mt_processor.load_stats(&mt_path)?;

        // Load ALL key stats
        let all_path = std::path::PathBuf::from(format!(
            "{}/.config/keyboard-middleware/all_key_stats.json",
            home
        ));
        self.load_all_key_stats(&all_path)?;

        Ok(())
    }

    /// Save all key stats to disk
    fn save_all_key_stats(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if self.all_key_stats.is_empty() {
            return Ok(());
        }

        let mut stats_map: std::collections::HashMap<String, RollingStats> =
            std::collections::HashMap::new();
        for (keycode, stats) in &self.all_key_stats {
            let key_str = format!("{:?}", keycode).replace("KC_", "");
            stats_map.insert(key_str, stats.clone());
        }

        let json = serde_json::to_string_pretty(&stats_map)?;
        std::fs::write(path, json)?;
        tracing::info!("ALL KEYS: Saved {} key stats", self.all_key_stats.len());
        Ok(())
    }

    /// Load all key stats from disk
    fn load_all_key_stats(&mut self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if !path.exists() {
            return Ok(());
        }

        let json = std::fs::read_to_string(path)?;
        let stats_map: std::collections::HashMap<String, RollingStats> =
            serde_json::from_str(&json)?;

        self.all_key_stats.clear();
        for (key_str, stats) in stats_map {
            let key_json = format!("\"KC_{}\"", key_str);
            if let Ok(keycode) = serde_json::from_str::<KeyCode>(&key_json) {
                self.all_key_stats.insert(keycode, stats);
            }
        }

        tracing::info!("ALL KEYS: Loaded {} key stats", self.all_key_stats.len());
        Ok(())
    }

    /// Get all key stats for display
    pub fn get_all_key_stats(&self) -> HashMap<KeyCode, RollingStats> {
        self.all_key_stats.clone()
    }

    /// Get home directory for a user ID
    fn get_user_home(user_id: u32) -> String {
        // Try to get the actual user's home directory from /etc/passwd
        use std::process::Command;

        let output = Command::new("getent")
            .args(&["passwd", &user_id.to_string()])
            .output();

        if let Ok(output) = output {
            if let Ok(line) = String::from_utf8(output.stdout) {
                // Format: username:x:uid:gid:gecos:home:shell
                if let Some(home) = line.split(':').nth(5) {
                    return home.trim().to_string();
                }
            }
        }

        // Fallback to /root if we can't determine
        "/root".to_string()
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

        // Track press time for ALL keys (100% keyboard coverage)
        self.key_press_times
            .insert(keycode, std::time::Instant::now());

        // Look up action for this key
        let action = self.lookup_action(keycode);

        match action {
            Some(ConfigAction::Key(output_key)) => {
                // Simple key remap
                actions.push(KeyAction::RegularKey(output_key));
                self.held_keys.insert(keycode, actions);

                // Notify MT processor (for permissive hold)
                let mt_resolutions = self.mt_processor.on_other_key_press(output_key);
                if !mt_resolutions.is_empty() {
                    let mut events = vec![(output_key, true)];
                    events.extend(self.apply_mt_resolutions(mt_resolutions));
                    ProcessResult::MultipleEvents(events)
                } else {
                    ProcessResult::EmitKey(output_key, true)
                }
            }
            Some(ConfigAction::MT(tap_action, hold_action)) => {
                // MT now uses Box<Action> - extract KeyCode if it's a simple Key action
                let tap_key_opt = match tap_action.as_ref() {
                    ConfigAction::Key(kc) => Some(*kc),
                    _ => None,
                };
                let hold_key_opt = match hold_action.as_ref() {
                    ConfigAction::Key(kc) => Some(*kc),
                    _ => None,
                };

                if let (Some(tap_key), Some(hold_key)) = (tap_key_opt, hold_key_opt) {
                    // MT key - first check if other MT keys need to be resolved
                    let mt_resolutions = self.mt_processor.on_other_key_press(tap_key);

                    // Then register this MT key
                    if let Some(resolution) = self.mt_processor.on_press(keycode, tap_key, hold_key)
                    {
                        // Double-tap detected, emit the hold immediately
                        actions.push(KeyAction::MtManaged);
                        self.held_keys.insert(keycode, actions);

                        if !mt_resolutions.is_empty() {
                            let mut events = self.apply_mt_resolutions(mt_resolutions);
                            events.extend(self.mt_resolution_to_events(&resolution));
                            ProcessResult::MultipleEvents(events)
                        } else {
                            self.apply_mt_resolution_single(resolution)
                        }
                    } else {
                        // Normal MT processing
                        actions.push(KeyAction::MtManaged);
                        self.held_keys.insert(keycode, actions);

                        if !mt_resolutions.is_empty() {
                            ProcessResult::MultipleEvents(self.apply_mt_resolutions(mt_resolutions))
                        } else {
                            ProcessResult::None
                        }
                    }
                } else {
                    // MT with complex nested actions not yet supported
                    warn!("MT with non-Key actions not yet supported (e.g., MT(TO(...), ...))");
                    ProcessResult::None
                }
            }
            Some(ConfigAction::TO(layer)) => {
                // Layer switch
                self.current_layer = layer.clone();
                actions.push(KeyAction::Layer(layer));
                self.held_keys.insert(keycode, actions);
                ProcessResult::None
            }
            Some(ConfigAction::SOCD(this_action, _opposing_actions)) => {
                // SOCD handling - extract KeyCode from Action
                if let ConfigAction::Key(this_key) = this_action.as_ref() {
                    actions.push(KeyAction::SocdManaged);
                    self.held_keys.insert(keycode, actions);
                    self.apply_socd_to_key_press(*this_key)
                } else {
                    warn!("SOCD with non-Key actions not yet supported");
                    ProcessResult::None
                }
            }
            Some(ConfigAction::CMD(command)) => {
                // Run arbitrary command
                ProcessResult::RunCommand(command.clone())
            }
            Some(ConfigAction::OSM(_modifier_action)) => {
                // TODO: OneShot Modifier - to be implemented next
                warn!("OSM action not yet implemented");
                ProcessResult::None
            }
            Some(ConfigAction::DT(_tap_action, _double_tap_action)) => {
                // TODO: Double-Tap - to be implemented next
                warn!("DT action not yet implemented");
                ProcessResult::None
            }
            None => {
                // No remap - check if MT keys are pending (permissive hold)
                let mt_resolutions = self.mt_processor.on_other_key_press(keycode);

                if !mt_resolutions.is_empty() {
                    // MT keys resolved, emit them first then this key
                    let mut events = self.apply_mt_resolutions(mt_resolutions);
                    events.push((keycode, true));

                    actions.push(KeyAction::RegularKey(keycode));
                    self.held_keys.insert(keycode, actions);
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
        // Track tap duration for ALL keys (100% keyboard coverage)
        if let Some(press_time) = self.key_press_times.remove(&keycode) {
            let duration_ms = press_time.elapsed().as_millis() as f32;

            // Only record taps below threshold (not holds)
            // This prevents survivorship bias - only successful taps are tracked
            let threshold_ms = 130.0; // Same threshold for all keys
            if duration_ms < threshold_ms && !self.game_mode_active {
                let stats = self
                    .all_key_stats
                    .entry(keycode)
                    .or_insert_with(|| RollingStats::new(threshold_ms));
                stats.update_tap(duration_ms, 30.0); // Use 30ms target margin
            }
        }

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
                    KeyAction::MtManaged => {
                        // Let MT processor handle the release
                        if let Some(resolution) = self.mt_processor.on_release(keycode) {
                            return self.apply_mt_resolution_single(resolution);
                        }
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

    /// Apply MT resolutions and return events to emit
    fn apply_mt_resolutions(&mut self, resolutions: Vec<MtResolution>) -> Vec<(KeyCode, bool)> {
        let mut events = Vec::new();

        for resolution in resolutions {
            events.extend(self.mt_resolution_to_events(&resolution));
        }

        events
    }

    /// Convert a single MT resolution to events
    fn mt_resolution_to_events(&self, resolution: &MtResolution) -> Vec<(KeyCode, bool)> {
        match resolution.action {
            MtAction::TapPress(key) => vec![(key, true)],
            MtAction::TapPressRelease(key) => vec![(key, true), (key, false)],
            MtAction::HoldPress(key) => vec![(key, true)],
            MtAction::HoldPressRelease(key) => vec![(key, true), (key, false)],
            MtAction::ReleaseHold(key) => vec![(key, false)],
        }
    }

    /// Apply single MT resolution
    fn apply_mt_resolution_single(&self, resolution: MtResolution) -> ProcessResult {
        match resolution.action {
            MtAction::TapPress(key) => ProcessResult::EmitKey(key, true),
            MtAction::TapPressRelease(key) => ProcessResult::TapKeyPressRelease(key),
            MtAction::HoldPress(key) => ProcessResult::EmitKey(key, true),
            MtAction::HoldPressRelease(key) => {
                ProcessResult::MultipleEvents(vec![(key, true), (key, false)])
            }
            MtAction::ReleaseHold(key) => ProcessResult::EmitKey(key, false),
        }
    }

    // === SOCD Helpers ===

    /// Apply SOCD key press - uses stack-based last-input-priority
    fn apply_socd_to_key_press(&mut self, keycode: KeyCode) -> ProcessResult {
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
            (None, Some(new_key)) => ProcessResult::EmitKey(new_key, true),
            (Some(old_key), None) => ProcessResult::EmitKey(old_key, false),
            (Some(old_key), Some(new_key)) if old_key == new_key => ProcessResult::None,
            (Some(old_key), Some(new_key)) => {
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
        KeyCode::KC_LGUI | KeyCode::KC_LCMD => Key::KEY_LEFTMETA,
        KeyCode::KC_RCTL => Key::KEY_RIGHTCTRL,
        KeyCode::KC_RSFT => Key::KEY_RIGHTSHIFT,
        KeyCode::KC_RALT => Key::KEY_RIGHTALT,
        KeyCode::KC_RGUI | KeyCode::KC_RCMD => Key::KEY_RIGHTMETA,

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
