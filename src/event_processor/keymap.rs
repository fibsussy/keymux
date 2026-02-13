use crate::keycode::KeyCode;
use std::collections::HashMap;
use tracing::warn;

use crate::config::{Action as ConfigAction, Config, Layer};

// Import action processors from the actions submodule
use super::actions::{
    DtConfig, DtProcessor, DtResolution, MtAction, MtConfig as ModtapConfig, MtProcessor,
    MtResolution, OsmConfig, OsmProcessor, OsmResolution, RollingStats,
};

/// What a key press is doing (recorded on press, replayed on release)
#[derive(Debug, Clone)]
#[allow(dead_code)]
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
    /// DT key managed by DT processor
    DtManaged,
    /// OSM key managed by OSM processor
    OsmManaged,
}

/// SOCD group configuration
#[derive(Debug, Clone)]
struct SocdGroup {
    /// All keys in this SOCD group
    #[allow(dead_code)]
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

    /// DT (Double-Tap) processor
    dt_processor: DtProcessor,

    /// OSM (OneShot Modifier) processor
    osm_processor: OsmProcessor,

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

    /// Track currently active modifiers from MT/OSM keys for proper shift handling
    active_modifiers: HashMap<KeyCode, KeyCode>, // Maps modifier key to the keycode that activated it
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
            for action in remaps.values() {
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
            hold_do_nothing_emits_tap: config.mt_config.hold_do_nothing_emits_tap,
        };

        // Build DT processor config
        let dt_config = DtConfig {
            tapping_term_ms: config.tapping_term_ms,
            double_tap_window_ms: config.double_tap_window_ms.unwrap_or(250),
        };

        // Build OSM processor config
        let osm_config = OsmConfig {
            oneshot_timeout_ms: config.oneshot_timeout_ms.unwrap_or(5000),
            tapping_term_ms: config.tapping_term_ms,
        };

        Self {
            held_keys: HashMap::new(),
            mt_processor: MtProcessor::new(mt_config),
            dt_processor: DtProcessor::new(dt_config),
            osm_processor: OsmProcessor::new(osm_config),
            current_layer: Layer::base(),
            base_remaps: config.remaps.clone(),
            layers,
            game_mode_active: false,
            game_mode_remaps: config.game_mode.remaps.clone(),
            socd_key_to_group,
            socd_groups,
            all_key_stats: HashMap::new(),
            key_press_times: HashMap::new(),
            active_modifiers: HashMap::new(),
        }
    }

    /// Set game mode state
    pub fn set_game_mode(&mut self, active: bool) {
        self.game_mode_active = active;
        self.mt_processor.set_game_mode(active);
    }

    /// Add an active modifier (for MT/OSM tracking)
    fn add_active_modifier(&mut self, source_key: KeyCode, modifier_key: KeyCode) {
        if modifier_key.is_modifier() {
            self.active_modifiers.insert(modifier_key, source_key);
        }
    }

    /// Remove an active modifier (for MT/OSM tracking)
    fn remove_active_modifier(&mut self, modifier_key: KeyCode) {
        self.active_modifiers.remove(&modifier_key);
    }

    /// Check if shift is currently active (from MT/OSM keys)
    fn has_active_shift(&self) -> bool {
        self.active_modifiers.contains_key(&KeyCode::KC_LSFT)
            || self.active_modifiers.contains_key(&KeyCode::KC_RSFT)
    }

    /// Apply active modifiers to a key if needed (for shifted output)
    #[allow(clippy::branches_sharing_code)] // Intentionally structured for future shift handling
    fn apply_modifiers_if_needed(&self, key: KeyCode) -> Vec<(KeyCode, bool)> {
        let mut events = Vec::new();

        if self.has_active_shift() {
            // For shift-sensitive keys when shift is active:
            // Use the system's native shift handling by letting the OS handle it
            // The modifier will already be pressed from the MT key
            events.push((key, true));
        } else {
            // For all other keys, emit normally
            events.push((key, true));
        }

        events
    }

    /// Check for DT timeouts and return events to emit
    /// Should be called periodically (e.g., every 1ms in the idle loop)
    /// Returns ProcessResult that can be emitted directly
    pub fn check_dt_timeouts(&mut self) -> ProcessResult {
        let timeouts = self.dt_processor.check_timeouts();
        if timeouts.is_empty() {
            ProcessResult::None
        } else {
            let events = self.process_dt_timeouts(timeouts);
            if events.is_empty() {
                ProcessResult::None
            } else {
                ProcessResult::MultipleEvents(events)
            }
        }
    }

    /// Get all currently held keys (for graceful shutdown)
    pub fn get_held_keys(&self) -> Vec<KeyCode> {
        self.held_keys.keys().copied().collect()
    }

    /// Save adaptive timing stats to disk for a specific user
    pub fn save_adaptive_stats(&self, user_id: u32) -> Result<(), std::io::Error> {
        let home = Self::get_user_home(user_id);

        // Save MT stats
        let mt_path =
            std::path::PathBuf::from(format!("{}/.config/keymux/adaptive_stats.json", home));
        self.mt_processor.save_stats(&mt_path)?;

        // Save ALL key stats
        let all_path =
            std::path::PathBuf::from(format!("{}/.config/keymux/all_key_stats.json", home));
        self.save_all_key_stats(&all_path)?;

        Ok(())
    }

    /// Load adaptive timing stats from disk for a specific user
    pub fn load_adaptive_stats(&mut self, user_id: u32) -> Result<(), std::io::Error> {
        let home = Self::get_user_home(user_id);

        // Load MT stats
        let mt_path =
            std::path::PathBuf::from(format!("{}/.config/keymux/adaptive_stats.json", home));
        self.mt_processor.load_stats(&mt_path)?;

        // Load ALL key stats
        let all_path =
            std::path::PathBuf::from(format!("{}/.config/keymux/all_key_stats.json", home));
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
    #[allow(dead_code)]
    pub fn get_all_key_stats(&self) -> HashMap<KeyCode, RollingStats> {
        self.all_key_stats.clone()
    }

    /// Get home directory for a user ID
    fn get_user_home(user_id: u32) -> String {
        // Try to get the actual user's home directory from /etc/passwd
        use std::process::Command;

        let output = Command::new("getent")
            .args(["passwd", &user_id.to_string()])
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

    /// Extract a KeyCode from an Action (for processor compatibility)
    /// Returns None if the action is not a simple Key(KC_*) action
    const fn extract_keycode(action: &ConfigAction) -> Option<KeyCode> {
        match action {
            ConfigAction::Key(kc) => Some(*kc),
            _ => None,
        }
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

        // Check DT timeouts at the start of every key press
        // This ensures pending taps are emitted even if user is typing other keys
        let dt_timeout_events = {
            let timeouts = self.dt_processor.check_timeouts();
            if !timeouts.is_empty() {
                self.process_dt_timeouts(timeouts)
            } else {
                Vec::new()
            }
        };

        // Look up action for this key
        let action = self.lookup_action(keycode);

        match action {
            Some(ConfigAction::Key(output_key)) => {
                // Simple key remap
                actions.push(KeyAction::RegularKey(output_key));
                self.held_keys.insert(keycode, actions);

                // Notify MT processor (for permissive hold)
                let mt_resolutions = self.mt_processor.on_other_key_press(output_key);
                let result = if !mt_resolutions.is_empty() {
                    // Emit MT resolutions FIRST (modifiers), then the current key
                    // This ensures shift is pressed before the key that needs shifting
                    let mut events = self.apply_mt_resolutions(mt_resolutions);
                    events.push((output_key, true));
                    ProcessResult::MultipleEvents(events)
                } else {
                    // Apply active modifiers if needed
                    let modifier_events = self.apply_modifiers_if_needed(output_key);
                    if modifier_events.len() == 1 {
                        ProcessResult::EmitKey(modifier_events[0].0, modifier_events[0].1)
                    } else {
                        ProcessResult::MultipleEvents(modifier_events)
                    }
                };
                self.combine_with_timeouts(dt_timeout_events, result)
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
                    #[allow(clippy::branches_sharing_code)] // Refactoring causes borrow issues
                    let result = if let Some(resolution) =
                        self.mt_processor.on_press(keycode, tap_key, hold_key)
                    {
                        // Double-tap detected, emit the hold immediately
                        actions.push(KeyAction::MtManaged);
                        self.held_keys.insert(keycode, actions);

                        if !mt_resolutions.is_empty() {
                            let mut events = self.apply_mt_resolutions(mt_resolutions);
                            events.extend(self.mt_resolution_to_events(&resolution));
                            ProcessResult::MultipleEvents(events)
                        } else {
                            self.apply_mt_resolution_with_tracking(keycode, resolution)
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
                    };
                    self.combine_with_timeouts(dt_timeout_events, result)
                } else {
                    // MT with complex nested actions not yet supported
                    warn!("MT with non-Key actions not yet supported (e.g., MT(TO(...), ...))");
                    self.combine_with_timeouts(dt_timeout_events, ProcessResult::None)
                }
            }
            Some(ConfigAction::TO(layer)) => {
                // Layer switch
                self.current_layer = layer.clone();
                actions.push(KeyAction::Layer(layer));
                self.held_keys.insert(keycode, actions);
                self.combine_with_timeouts(dt_timeout_events, ProcessResult::None)
            }
            Some(ConfigAction::SOCD(this_action, _opposing_actions)) => {
                // SOCD handling - extract KeyCode from Action
                let result = if let ConfigAction::Key(this_key) = this_action.as_ref() {
                    actions.push(KeyAction::SocdManaged);
                    self.held_keys.insert(keycode, actions);
                    self.apply_socd_to_key_press(*this_key)
                } else {
                    warn!("SOCD with non-Key actions not yet supported");
                    ProcessResult::None
                };
                self.combine_with_timeouts(dt_timeout_events, result)
            }
            Some(ConfigAction::CMD(command)) => {
                // Run arbitrary command
                self.combine_with_timeouts(dt_timeout_events, ProcessResult::RunCommand(command))
            }
            Some(ConfigAction::OSM(modifier_action)) => {
                // OSM (OneShot Modifier) - extract KeyCode for simple cases
                if let Some(modifier_key) = Self::extract_keycode(modifier_action.as_ref()) {
                    // Check OSM timeouts first
                    let timeouts = self.osm_processor.check_timeouts();
                    // Process timeouts if any (emit release events)
                    if !timeouts.is_empty() {
                        // These will be handled in the event loop
                    }

                    // Register this OSM key
                    let _resolution = self.osm_processor.on_press(keycode, modifier_key);

                    actions.push(KeyAction::OsmManaged);
                    self.held_keys.insert(keycode, actions);

                    // OSM doesn't emit on press, waits for release to determine tap/hold
                } else {
                    // OSM with complex actions not yet supported
                    warn!("OSM with non-Key actions not yet supported");
                }
                self.combine_with_timeouts(dt_timeout_events, ProcessResult::None)
            }
            Some(ConfigAction::DT(tap_action, double_tap_action)) => {
                // DT (Double-Tap) - extract KeyCodes for simple cases
                if let (Some(tap_key), Some(dtap_key)) = (
                    Self::extract_keycode(tap_action.as_ref()),
                    Self::extract_keycode(double_tap_action.as_ref()),
                ) {
                    // Register this DT key
                    let resolution = self.dt_processor.on_press(keycode, tap_key, dtap_key);

                    actions.push(KeyAction::DtManaged);
                    self.held_keys.insert(keycode, actions);

                    let result = match resolution {
                        DtResolution::PressSecond(key) => {
                            // Double-tap detected! Press second action
                            ProcessResult::EmitKey(key, true)
                        }
                        DtResolution::HoldFirst(key) => {
                            // Held beyond tapping term - start holding first action
                            ProcessResult::EmitKey(key, true)
                        }
                        DtResolution::TapFirst(key) => {
                            // Single-tap timeout - emit first action as tap
                            ProcessResult::TapKeyPressRelease(key)
                        }
                        DtResolution::Undecided => {
                            // Still pending - no events for this key
                            ProcessResult::None
                        }
                        _ => ProcessResult::None,
                    };

                    self.combine_with_timeouts(dt_timeout_events, result)
                } else {
                    // DT with complex actions not yet supported
                    warn!("DT with non-Key actions not yet supported");
                    ProcessResult::None
                }
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
                    self.combine_with_timeouts(
                        dt_timeout_events,
                        ProcessResult::MultipleEvents(events),
                    )
                } else {
                    // Pass through unchanged
                    actions.push(KeyAction::RegularKey(keycode));
                    self.held_keys.insert(keycode, actions);
                    self.combine_with_timeouts(
                        dt_timeout_events,
                        ProcessResult::EmitKey(keycode, true),
                    )
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

        // Check DT timeouts on release too
        // This is CRITICAL - without this, single-taps never emit!
        let dt_timeout_events = {
            let timeouts = self.dt_processor.check_timeouts();
            if !timeouts.is_empty() {
                self.process_dt_timeouts(timeouts)
            } else {
                Vec::new()
            }
        };

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
                            return self.apply_mt_resolution_with_tracking(keycode, resolution);
                        }
                    }
                    KeyAction::SocdManaged => {
                        // Apply SOCD release logic
                        return self.apply_socd_to_key_release(keycode);
                    }
                    KeyAction::DtManaged => {
                        // Let DT processor handle the release
                        let resolution = self.dt_processor.on_release(keycode);
                        let result = match resolution {
                            DtResolution::ReleaseFirst(key) => {
                                // Was holding first action - release it
                                ProcessResult::EmitKey(key, false)
                            }
                            DtResolution::ReleaseSecond(key) => {
                                // Was holding second action (double-tap-hold) - release it
                                ProcessResult::EmitKey(key, false)
                            }
                            DtResolution::Undecided => {
                                // Still in Pending or Tapped state, waiting
                                ProcessResult::None
                            }
                            _ => {
                                // Other resolutions shouldn't come from on_release
                                ProcessResult::None
                            }
                        };
                        return self.combine_with_timeouts(dt_timeout_events, result);
                    }
                    KeyAction::OsmManaged => {
                        // Let OSM processor handle the release
                        let resolution = self.osm_processor.on_release(keycode);
                        match resolution {
                            OsmResolution::ActivateModifier(mod_key) => {
                                // Tapped - activate one-shot
                                return ProcessResult::EmitKey(mod_key, true);
                            }
                            OsmResolution::ReleaseModifier(mod_key) => {
                                // Held - release normal modifier
                                return ProcessResult::EmitKey(mod_key, false);
                            }
                            OsmResolution::None => {
                                return ProcessResult::None;
                            }
                        }
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
    fn apply_mt_resolutions(&self, resolutions: Vec<MtResolution>) -> Vec<(KeyCode, bool)> {
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

    /// Apply single MT resolution with modifier tracking
    fn apply_mt_resolution_with_tracking(
        &mut self,
        source_key: KeyCode,
        resolution: MtResolution,
    ) -> ProcessResult {
        match &resolution.action {
            MtAction::TapPress(key) => {
                // Tap doesn't hold modifier, no tracking needed
                ProcessResult::EmitKey(*key, true)
            }
            MtAction::TapPressRelease(key) => {
                // Tap release, no tracking needed
                ProcessResult::TapKeyPressRelease(*key)
            }
            MtAction::HoldPress(key) => {
                // Hold press - track the modifier if it's a modifier key
                if key.is_modifier() {
                    self.add_active_modifier(source_key, *key);
                }
                ProcessResult::EmitKey(*key, true)
            }
            MtAction::HoldPressRelease(key) => {
                // Hold press and release - track modifier if needed, then release
                if key.is_modifier() {
                    self.add_active_modifier(source_key, *key);
                }
                ProcessResult::MultipleEvents(vec![(*key, true), (*key, false)])
            }
            MtAction::ReleaseHold(key) => {
                // Hold release - stop tracking the modifier
                if key.is_modifier() {
                    self.remove_active_modifier(*key);
                }
                ProcessResult::EmitKey(*key, false)
            }
        }
    }

    /// Apply single MT resolution (immutable version for compatibility)
    #[allow(dead_code)]
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

    // === DT Helpers ===

    /// Process DT timeout resolutions and return events to emit
    fn process_dt_timeouts(&self, timeouts: Vec<(KeyCode, DtResolution)>) -> Vec<(KeyCode, bool)> {
        let mut events = Vec::new();

        for (_keycode, resolution) in timeouts {
            match resolution {
                DtResolution::HoldFirst(key) => {
                    // Start holding first action
                    events.push((key, true));
                }
                DtResolution::TapFirst(key) => {
                    // Emit single-tap
                    events.push((key, true));
                    events.push((key, false));
                }
                _ => {
                    // Other resolutions shouldn't come from check_timeouts
                }
            }
        }

        events
    }

    /// Combine DT timeout events with a ProcessResult
    fn combine_with_timeouts(
        &self,
        timeout_events: Vec<(KeyCode, bool)>,
        result: ProcessResult,
    ) -> ProcessResult {
        if timeout_events.is_empty() {
            return result;
        }

        match result {
            ProcessResult::None => {
                if timeout_events.is_empty() {
                    ProcessResult::None
                } else {
                    ProcessResult::MultipleEvents(timeout_events)
                }
            }
            ProcessResult::EmitKey(key, pressed) => {
                let mut all_events = timeout_events;
                all_events.push((key, pressed));
                ProcessResult::MultipleEvents(all_events)
            }
            ProcessResult::TapKeyPressRelease(key) => {
                let mut all_events = timeout_events;
                all_events.push((key, true));
                all_events.push((key, false));
                ProcessResult::MultipleEvents(all_events)
            }
            ProcessResult::MultipleEvents(mut events) => {
                let mut all_events = timeout_events;
                all_events.append(&mut events);
                ProcessResult::MultipleEvents(all_events)
            }
            other => other, // Pass through TypeString etc unchanged
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
#[allow(dead_code)]
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
