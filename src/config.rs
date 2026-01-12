use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// QMK-inspired keycode enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum KeyCode {
    // Letters
    KC_A,
    KC_B,
    KC_C,
    KC_D,
    KC_E,
    KC_F,
    KC_G,
    KC_H,
    KC_I,
    KC_J,
    KC_K,
    KC_L,
    KC_M,
    KC_N,
    KC_O,
    KC_P,
    KC_Q,
    KC_R,
    KC_S,
    KC_T,
    KC_U,
    KC_V,
    KC_W,
    KC_X,
    KC_Y,
    KC_Z,

    // Numbers
    KC_1,
    KC_2,
    KC_3,
    KC_4,
    KC_5,
    KC_6,
    KC_7,
    KC_8,
    KC_9,
    KC_0,

    // Modifiers
    KC_LCTL,
    KC_LSFT,
    KC_LALT,
    KC_LGUI,
    KC_LCMD, // KC_LCMD is alias for KC_LGUI
    KC_RCTL,
    KC_RSFT,
    KC_RALT,
    KC_RGUI,
    KC_RCMD, // KC_RCMD is alias for KC_RGUI

    // Special keys
    KC_ESC,
    KC_CAPS,
    KC_TAB,
    KC_SPC,
    KC_ENT,
    KC_BSPC,
    KC_DEL,
    KC_GRV,
    KC_MINS,
    KC_EQL,
    KC_LBRC,
    KC_RBRC,
    KC_BSLS,
    KC_SCLN,
    KC_QUOT,
    KC_COMM,
    KC_DOT,
    KC_SLSH,

    // Arrow keys
    KC_LEFT,
    KC_DOWN,
    KC_UP,
    KC_RGHT,

    // Function keys
    KC_F1,
    KC_F2,
    KC_F3,
    KC_F4,
    KC_F5,
    KC_F6,
    KC_F7,
    KC_F8,
    KC_F9,
    KC_F10,
    KC_F11,
    KC_F12,
    KC_F13,
    KC_F14,
    KC_F15,
    KC_F16,
    KC_F17,
    KC_F18,
    KC_F19,
    KC_F20,
    KC_F21,
    KC_F22,
    KC_F23,
    KC_F24,

    // Navigation keys
    KC_PGUP,
    KC_PGDN,
    KC_HOME,
    KC_END,
    KC_INS,
    KC_PSCR,

    // Numpad keys
    KC_KP_0,
    KC_KP_1,
    KC_KP_2,
    KC_KP_3,
    KC_KP_4,
    KC_KP_5,
    KC_KP_6,
    KC_KP_7,
    KC_KP_8,
    KC_KP_9,
    KC_KP_SLASH,
    KC_KP_ASTERISK,
    KC_KP_MINUS,
    KC_KP_PLUS,
    KC_KP_ENTER,
    KC_KP_DOT,
    KC_NUM_LOCK,

    // Media keys
    KC_MUTE,
    KC_VOL_UP,
    KC_VOL_DN,
    KC_MEDIA_PLAY_PAUSE,
    KC_MEDIA_STOP,
    KC_MEDIA_NEXT_TRACK,
    KC_MEDIA_PREV_TRACK,
    KC_MEDIA_SELECT,

    // System keys
    KC_PWR,
    KC_SLEP,
    KC_WAKE,
    KC_CALC,
    KC_MY_COMP,
    KC_WWW_SEARCH,
    KC_WWW_HOME,
    KC_WWW_BACK,
    KC_WWW_FORWARD,
    KC_WWW_STOP,
    KC_WWW_REFRESH,
    KC_WWW_FAVORITES,

    // Locking keys
    KC_SCRL,
    KC_PAUS,

    // Application keys
    KC_APP,
    KC_MENU,

    // Multimedia keys
    KC_BRIU,
    KC_BRID,
    KC_DISPLAY_OFF,
    KC_WLAN,
    KC_BLUETOOTH,
    KC_KEYBOARD_LAYOUT,

    // International keys
    KC_INTL_BACKSLASH,
    KC_INTL_YEN,
    KC_INTL_RO,
}

/// Layer identifier - fully generic string-based layers
/// "base" and "game_mode" are reserved layer names
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Layer(pub String);

impl Layer {
    /// Base layer (always exists)
    pub fn base() -> Self {
        Layer("base".to_string())
    }

    /// Check if this is the base layer
    pub fn is_base(&self) -> bool {
        self.0 == "base"
    }

    /// Create a new layer from string
    pub fn new(name: impl Into<String>) -> Self {
        Layer(name.into())
    }
}

/// Key action - what happens when a key is pressed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Direct key mapping
    Key(KeyCode),
    /// QMK-style Mod-Tap: advanced tap/hold with configurable behavior
    /// MT(tap_key, hold_key) - Tap for tap_key, hold for hold_key
    /// Supports: permissive hold, roll detection, chord detection, adaptive timing
    MT(KeyCode, KeyCode),
    /// Switch to layer
    TO(Layer),
    /// SOCD (Simultaneous Opposite Cardinal Direction) - fully generic
    /// When this key is pressed, unpress all opposing keys
    /// Format: SOCD(this_key, [opposing_keys...])
    /// Example: SOCD(KC_W, [KC_S]) or SOCD(KC_W, [KC_S, KC_DOWN])
    SOCD(KeyCode, Vec<KeyCode>),
    /// Run arbitrary shell command
    /// Example: CMD("/usr/bin/notify-send 'Hello'")
    CMD(String),
}

/// Game mode detection methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionMethod {
    GamescopeAppId,
    SteamAppPrefix,
    IsGameEnvVar,
    ProcessTreeWalk,
}

/// Layer configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerConfig {
    pub remaps: HashMap<KeyCode, Action>,
}

/// Game mode configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GameMode {
    #[serde(default)]
    pub remaps: HashMap<KeyCode, Action>,
}

impl GameMode {
    #[must_use]
    pub const fn auto_detect_enabled() -> bool {
        true
    }

    #[must_use]
    pub fn detection_methods() -> Vec<DetectionMethod> {
        vec![
            DetectionMethod::GamescopeAppId,
            DetectionMethod::SteamAppPrefix,
            DetectionMethod::IsGameEnvVar,
            DetectionMethod::ProcessTreeWalk,
        ]
    }

    #[must_use]
    pub const fn process_tree_depth() -> u32 {
        10
    }
}

/// Per-keyboard override configuration
/// This has the EXACT same structure as the main Config, but all fields are optional
/// This allows you to copy the global config and paste it here - it will just override the specified fields
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PerKeyboardConfig {
    pub tapping_term_ms: Option<u32>,
    pub mt_config: Option<MtConfig>,
    pub remaps: Option<HashMap<KeyCode, Action>>,
    pub layers: Option<HashMap<Layer, LayerConfig>>,
    pub game_mode: Option<GameMode>,
}

/// MT (Mod-Tap) configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MtConfig {
    /// Enable permissive hold - if another key is pressed while MT is pending,
    /// immediately resolve to hold (default: true)
    #[serde(default = "default_true")]
    pub permissive_hold: bool,

    /// Enable same-hand roll detection - rolls on same hand favor tap (default: true)
    #[serde(default = "default_true", alias = "enable_roll_detection")]
    pub same_hand_roll_detection: bool,

    /// Enable opposite-hand chord detection - chords on opposite hands favor hold (default: true)
    #[serde(default = "default_true", alias = "enable_chord_detection")]
    pub opposite_hand_chord_detection: bool,

    /// Enable multi-mod detection - multiple modifiers held simultaneously
    /// on same hand all promote to hold (default: true)
    #[serde(default = "default_true", alias = "enable_multi_mod_detection")]
    pub multi_mod_detection: bool,

    /// Minimum number of MT keys held to trigger multi-mod (default: 2)
    #[serde(default = "default_multi_mod_threshold")]
    pub multi_mod_threshold: usize,

    /// Enable adaptive timing - adjust thresholds based on user behavior (default: false)
    #[serde(default, alias = "enable_adaptive_timing")]
    pub adaptive_timing: bool,

    /// Enable predictive intent scoring (default: false)
    #[serde(default, alias = "enable_predictive_scoring")]
    pub predictive_scoring: bool,

    /// Roll detection window in ms (default: 150)
    #[serde(default = "default_roll_window", alias = "roll_threshold_ms")]
    pub roll_detection_window_ms: u32,

    /// Chord detection window in ms (default: 50)
    #[serde(default = "default_chord_window", alias = "chord_threshold_ms")]
    pub chord_detection_window_ms: u32,

    /// Enable double-tap-then-hold - double tap to hold the tap key until released (default: false)
    #[serde(default, alias = "enable_double_tap_hold")]
    pub double_tap_then_hold: bool,

    /// Window (ms) for detecting double-taps (default: 300)
    #[serde(default = "default_double_tap_window")]
    pub double_tap_window_ms: u32,

    /// Enable cross-hand unwrap - when holding a modifier on one hand,
    /// MT keys on the opposite hand will unwrap to their tap key (default: true)
    /// Example: Hold ; (right hand, becomes Win), press f (left hand MT) → types 'f' not Shift
    #[serde(default = "default_true", alias = "enable_cross_hand_unwrap")]
    pub cross_hand_unwrap: bool,

    /// Target margin (ms) to keep adaptive threshold above average tap duration (default: 30)
    /// Example: If your average tap is 45ms, threshold becomes 45 + 30 = 75ms
    #[serde(default = "default_adaptive_margin", alias = "target_margin_ms")]
    pub adaptive_target_margin_ms: u32,

    /// Pause adaptive learning in game mode (default: true)
    #[serde(default = "default_true")]
    pub pause_learning_in_game_mode: bool,

    /// Exponential moving average alpha for adaptive learning (default: 0.02)
    #[serde(default = "default_ema_alpha")]
    pub ema_alpha: f32,

    /// Auto-save adaptive stats interval in seconds (default: 30)
    #[serde(default = "default_auto_save_interval")]
    pub auto_save_interval_secs: u32,
}

fn default_ema_alpha() -> f32 {
    0.02
}

fn default_auto_save_interval() -> u32 {
    30
}

fn default_true() -> bool {
    true
}
fn default_multi_mod_threshold() -> usize {
    2
}
fn default_roll_window() -> u32 {
    150
}
fn default_chord_window() -> u32 {
    50
}
fn default_double_tap_window() -> u32 {
    300
}
fn default_adaptive_margin() -> u32 {
    30
}

impl Default for MtConfig {
    fn default() -> Self {
        Self {
            permissive_hold: true,
            same_hand_roll_detection: true,
            opposite_hand_chord_detection: true,
            multi_mod_detection: true,
            multi_mod_threshold: 2,
            adaptive_timing: false,
            predictive_scoring: false,
            roll_detection_window_ms: 150,
            chord_detection_window_ms: 50,
            double_tap_then_hold: false,
            double_tap_window_ms: 300,
            cross_hand_unwrap: true,
            adaptive_target_margin_ms: 30,
            pause_learning_in_game_mode: true,
            ema_alpha: 0.02,
            auto_save_interval_secs: 30,
        }
    }
}

/// Main configuration structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_tapping_term")]
    pub tapping_term_ms: u32,
    #[serde(default)]
    pub mt_config: MtConfig,
    #[serde(default)]
    pub enabled_keyboards: Option<Vec<String>>,
    #[serde(default)]
    pub remaps: HashMap<KeyCode, Action>,
    #[serde(default)]
    pub layers: HashMap<Layer, LayerConfig>,
    #[serde(default)]
    pub game_mode: GameMode,
    #[serde(default)]
    pub per_keyboard_overrides: HashMap<String, PerKeyboardConfig>,

    /// Enable hot config reload - automatically reload config when file changes (default: false)
    /// When enabled, changes to config.ron are immediately applied without restarting daemon
    #[serde(default)]
    pub hot_config_reload: bool,

    /// Per-keyboard configs inherit global layout by default (default: true)
    /// - true: per_keyboard_overrides merge with global config (override specific fields)
    /// - false: per_keyboard_overrides replace global config (build from scratch)
    #[serde(default = "default_true_bool")]
    pub per_keyboard_inherits_global_layout: bool,
}

fn default_tapping_term() -> u32 {
    130
}

fn default_true_bool() -> bool {
    true
}

impl Config {
    /// Load config from RON file
    #[allow(clippy::missing_errors_doc)]
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config = ron::from_str(&content)?;
        Ok(config)
    }

    /// Save config to RON file
    #[allow(clippy::missing_errors_doc)]
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let pretty = ron::ser::PrettyConfig::default();
        let content = ron::ser::to_string_pretty(self, pretty)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get default config path
    #[allow(clippy::missing_errors_doc)]
    pub fn default_path() -> anyhow::Result<std::path::PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Failed to get config dir"))?;
        Ok(config_dir.join("keyboard-middleware").join("config.ron"))
    }

    /// Get effective config for a specific keyboard
    /// Applies per-keyboard overrides on top of the global config (or replaces it)
    #[must_use]
    pub fn for_keyboard(&self, keyboard_id: &str) -> Self {
        if let Some(override_cfg) = self.per_keyboard_overrides.get(keyboard_id) {
            if self.per_keyboard_inherits_global_layout {
                // INHERITING MODE: Start with global config, merge/override with per-keyboard settings
                let mut config = self.clone();

                // Apply all overrides - each field is optional and only overrides if Some
                if let Some(term) = override_cfg.tapping_term_ms {
                    config.tapping_term_ms = term;
                }
                if let Some(mt) = &override_cfg.mt_config {
                    config.mt_config = mt.clone();
                }

                // MERGE remaps: extend global remaps with per-keyboard remaps
                // Per-keyboard remaps override global ones for the same keys
                if let Some(remaps) = &override_cfg.remaps {
                    config.remaps.extend(remaps.clone());
                }

                // MERGE layers: extend global layers with per-keyboard layers
                // Per-keyboard layers override global ones for the same layer names
                if let Some(layers) = &override_cfg.layers {
                    config.layers.extend(layers.clone());
                }

                // MERGE game_mode: extend global game_mode remaps with per-keyboard game_mode remaps
                if let Some(game_mode) = &override_cfg.game_mode {
                    config.game_mode.remaps.extend(game_mode.remaps.clone());
                }

                config
            } else {
                // NON-INHERITING MODE: Build from scratch with per-keyboard config only
                // Use defaults for any fields not specified in per-keyboard config
                Config {
                    tapping_term_ms: override_cfg
                        .tapping_term_ms
                        .unwrap_or_else(default_tapping_term),
                    mt_config: override_cfg.mt_config.clone().unwrap_or_default(),
                    enabled_keyboards: self.enabled_keyboards.clone(), // Keep global enabled_keyboards
                    remaps: override_cfg.remaps.clone().unwrap_or_default(),
                    layers: override_cfg.layers.clone().unwrap_or_default(),
                    game_mode: override_cfg.game_mode.clone().unwrap_or_default(),
                    per_keyboard_overrides: HashMap::new(), // Don't nest overrides
                    hot_config_reload: self.hot_config_reload, // Keep global hot reload setting
                    per_keyboard_inherits_global_layout: self.per_keyboard_inherits_global_layout, // Keep global setting
                }
            }
        } else {
            // No per-keyboard override found, return global config as-is
            self.clone()
        }
    }

    /// Save only `enabled_keyboards` field, preserving rest of file
    #[allow(clippy::missing_errors_doc)]
    /// Save only the enabled_keyboards field, preserving all other formatting
    pub fn save_enabled_keyboards_only(&self, path: &std::path::Path) -> anyhow::Result<()> {
        // Read the original file
        let content = std::fs::read_to_string(path)?;

        // Find the enabled_keyboards field
        let start_marker = "enabled_keyboards:";

        if let Some(start_pos) = content.find(start_marker) {
            // Find where the field starts (after the colon)
            let field_start = start_pos + start_marker.len();

            // Find the end of this field (next field or closing paren)
            let remaining = &content[field_start..];

            // Find the end by looking for comma at the end of the value
            let mut end_pos = field_start;
            let mut depth = 0;
            let mut in_string = false;
            let mut found_value_start = false;

            for (i, ch) in remaining.chars().enumerate() {
                match ch {
                    '"' if i == 0
                        || remaining.as_bytes().get(i.wrapping_sub(1)) != Some(&b'\\') =>
                    {
                        in_string = !in_string;
                    }
                    '[' if !in_string => {
                        depth += 1;
                        found_value_start = true;
                    }
                    ']' if !in_string => {
                        depth -= 1;
                        if depth == 0 && found_value_start {
                            // Found the end of the array
                            end_pos = field_start + i + 1;
                            break;
                        }
                    }
                    'N' if !in_string
                        && !found_value_start
                        && remaining[i..].starts_with("None") =>
                    {
                        end_pos = field_start + i + 4;
                        break;
                    }
                    'S' if !in_string
                        && !found_value_start
                        && remaining[i..].starts_with("Some") =>
                    {
                        found_value_start = true;
                    }
                    '(' if !in_string && found_value_start => depth += 1,
                    ')' if !in_string && found_value_start => {
                        depth -= 1;
                        if depth == 0 {
                            end_pos = field_start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if end_pos == field_start {
                // Couldn't parse, fall back to full save
                return self.save(path);
            }

            // Generate the new value
            let new_value = if let Some(ref keyboards) = self.enabled_keyboards {
                if keyboards.is_empty() {
                    " Some([])".to_string()
                } else {
                    let mut result = " Some([\n".to_string();
                    for kbd in keyboards {
                        result.push_str(&format!("        \"{}\",\n", kbd));
                    }
                    result.push_str("    ])");
                    result
                }
            } else {
                " None".to_string()
            };

            // Build the new content
            let new_content = format!(
                "{}{}{}",
                &content[..field_start],
                new_value,
                &content[end_pos..]
            );

            // Write it back
            std::fs::write(path, new_content)?;
            Ok(())
        } else {
            // enabled_keyboards field not found, fall back to full save
            self.save(path)
        }
    }

    /// Validate config without printing - returns errors as a Vec<String>
    pub fn validate_silent(&self) -> Result<()> {
        use std::collections::{HashMap, HashSet};

        let mut errors: Vec<String> = Vec::new();

        // Validation 1: Check SOCD pairs are symmetric
        let mut socd_map: HashMap<KeyCode, KeyCode> = HashMap::new();

        let mut extract_socd = |remaps: &HashMap<KeyCode, Action>| {
            let mut pairs = Vec::new();
            for (key, action) in remaps {
                if let Action::SOCD(this_key, opposing_keys) = action {
                    if key != this_key {
                        errors.push(format!(
                            "SOCD key mismatch: {:?} maps to SOCD({:?}, ...)",
                            key, this_key
                        ));
                    }
                    for opposing_key in opposing_keys {
                        pairs.push((*this_key, *opposing_key));
                    }
                }
            }
            pairs
        };

        for (key1, key2) in extract_socd(&self.remaps) {
            socd_map.insert(key1, key2);
        }
        for layer_config in self.layers.values() {
            for (key1, key2) in extract_socd(&layer_config.remaps) {
                socd_map.insert(key1, key2);
            }
        }
        for (key1, key2) in extract_socd(&self.game_mode.remaps) {
            socd_map.insert(key1, key2);
        }

        // Check symmetry
        let mut socd_checked = HashSet::new();
        for (key1, key2) in &socd_map {
            if socd_checked.contains(key1) {
                continue;
            }
            if let Some(reverse) = socd_map.get(key2) {
                if reverse != key1 {
                    errors.push(format!(
                        "SOCD pair asymmetric: {:?} → {:?}, but {:?} → {:?}",
                        key1, key2, key2, reverse
                    ));
                }
                socd_checked.insert(*key1);
                socd_checked.insert(*key2);
            } else {
                errors.push(format!(
                    "SOCD missing reverse pair: {:?} → {:?}, but {:?} not defined",
                    key1, key2, key2
                ));
            }
        }

        // Validation 2: Check timing values are reasonable
        if self.tapping_term_ms == 0 || self.tapping_term_ms > 1000 {
            errors.push(format!(
                "tapping_term_ms out of reasonable range (0-1000): {}",
                self.tapping_term_ms
            ));
        }

        // Validate MT config timing
        if self.mt_config.double_tap_window_ms == 0 || self.mt_config.double_tap_window_ms > 1000 {
            errors.push(format!(
                "mt_config.double_tap_window_ms out of reasonable range (0-1000): {}",
                self.mt_config.double_tap_window_ms
            ));
        }

        // Validation 3: Check layer references
        let mut referenced_layers = HashSet::new();

        let extract_layer_refs = |remaps: &HashMap<KeyCode, Action>| {
            let mut refs = Vec::new();
            for action in remaps.values() {
                if let Action::TO(layer) = action {
                    refs.push(layer.0.clone());
                }
            }
            refs
        };

        for layer_name in extract_layer_refs(&self.remaps) {
            referenced_layers.insert(layer_name);
        }
        for layer_config in self.layers.values() {
            for layer_name in extract_layer_refs(&layer_config.remaps) {
                referenced_layers.insert(layer_name);
            }
        }

        for layer_name in &referenced_layers {
            if layer_name != "base" && !self.layers.contains_key(&Layer(layer_name.clone())) {
                errors.push(format!("Referenced layer not defined: \"{}\"", layer_name));
            }
        }

        if !errors.is_empty() {
            Err(anyhow::anyhow!(
                "Config validation failed: {}",
                errors.join("; ")
            ))
        } else {
            Ok(())
        }
    }
}
