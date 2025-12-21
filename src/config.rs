use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// QMK-inspired keycode enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum KeyCode {
    // Letters
    KC_A, KC_B, KC_C, KC_D, KC_E, KC_F, KC_G, KC_H, KC_I, KC_J, KC_K, KC_L, KC_M,
    KC_N, KC_O, KC_P, KC_Q, KC_R, KC_S, KC_T, KC_U, KC_V, KC_W, KC_X, KC_Y, KC_Z,

    // Numbers
    KC_1, KC_2, KC_3, KC_4, KC_5, KC_6, KC_7, KC_8, KC_9, KC_0,

    // Modifiers
    KC_LCTL, KC_LSFT, KC_LALT, KC_LGUI,
    KC_RCTL, KC_RSFT, KC_RALT, KC_RGUI,

    // Special keys
    KC_ESC, KC_CAPS, KC_TAB, KC_SPC, KC_ENT, KC_BSPC, KC_DEL,
    KC_GRV, KC_MINS, KC_EQL, KC_LBRC, KC_RBRC, KC_BSLS,
    KC_SCLN, KC_QUOT, KC_COMM, KC_DOT, KC_SLSH,

    // Arrow keys
    KC_LEFT, KC_DOWN, KC_UP, KC_RGHT,

    // Function keys
    KC_F1, KC_F2, KC_F3, KC_F4, KC_F5, KC_F6,
    KC_F7, KC_F8, KC_F9, KC_F10, KC_F11, KC_F12,
}

/// Layer identifier - supports unlimited layers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum Layer {
    L_BASE,
    L_NAV,
    L_NUM,
    L_SYM,
    L_FN,
}

/// Key action - what happens when a key is pressed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Direct key mapping
    Key(KeyCode),
    /// Homerow mod: tap for key, hold for modifier
    HR(KeyCode, KeyCode),
    /// Switch to layer
    TO(Layer),
    /// SOCD (Simultaneous Opposite Cardinal Direction) - for gaming
    Socd(KeyCode, KeyCode),
    /// Type password (double-tap to add Enter)
    Password,
}

/// Game mode detection methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionMethod {
    GamescopeAppId,
    SteamAppPrefix,
    IsGameEnvVar,
    ProcessTreeWalk,
}

/// SOCD resolution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocdMode {
    LastInputPriority,
    Neutral,
}

/// Override trigger type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverrideTriggerType {
    BothShifts,
    KeyCombo,
}

/// SOCD configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocdConfig {
    pub enabled: bool,
    pub keys: Option<Vec<(KeyCode, KeyCode)>>,
    pub mode: SocdMode,
}

/// Override trigger configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideTrigger {
    pub trigger_type: OverrideTriggerType,
    pub key_combo: Vec<KeyCode>,
}

/// Layer configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerConfig {
    pub remaps: HashMap<KeyCode, Action>,
}

/// Game mode configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameMode {
    pub auto_detect: bool,
    pub detection_methods: Vec<DetectionMethod>,
    pub process_tree_depth: u32,
    pub remaps: HashMap<KeyCode, Action>,
    pub socd: SocdConfig,
    pub override_trigger: OverrideTrigger,
}

/// Main configuration structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub tapping_term_ms: u32,
    pub double_tap_window_ms: Option<u32>,
    pub password: Option<String>,
    pub enabled_keyboards: Option<Vec<String>>,
    pub remaps: HashMap<KeyCode, Action>,
    pub layers: HashMap<Layer, LayerConfig>,
    pub game_mode: GameMode,
    pub keyboard_overrides: HashMap<String, String>, // TODO: define proper type
}

impl Config {
    /// Load config from RON file
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config = ron::from_str(&content)?;
        Ok(config)
    }

    /// Save config to RON file
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let pretty = ron::ser::PrettyConfig::default();
        let content = ron::ser::to_string_pretty(self, pretty)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get default config path
    pub fn default_path() -> anyhow::Result<std::path::PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Failed to get config dir"))?;
        Ok(config_dir.join("keyboard-middleware").join("config.ron"))
    }
}
