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
    /// Simple overload: tap for key, hold for modifier (no permissive hold)
    OVERLOAD(KeyCode, KeyCode),
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


/// Layer configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerConfig {
    pub remaps: HashMap<KeyCode, Action>,
}

/// Game mode configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameMode {
    pub remaps: HashMap<KeyCode, Action>,
}

impl GameMode {
    pub fn auto_detect_enabled() -> bool {
        true
    }

    pub fn detection_methods() -> Vec<DetectionMethod> {
        vec![
            DetectionMethod::GamescopeAppId,
            DetectionMethod::SteamAppPrefix,
            DetectionMethod::IsGameEnvVar,
            DetectionMethod::ProcessTreeWalk,
        ]
    }

    pub fn process_tree_depth() -> u32 {
        10
    }
}

/// Per-keyboard override configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardOverride {
    pub keymap: Option<KeymapOverride>,
    pub settings: Option<SettingsOverride>,
}

/// Keymap override - specify which layers/remaps to override
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeymapOverride {
    pub base_remaps: Option<HashMap<KeyCode, Action>>,
    pub layers: Option<HashMap<Layer, LayerConfig>>,
    pub game_mode_remaps: Option<HashMap<KeyCode, Action>>,
}

/// Settings override
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsOverride {
    pub tapping_term_ms: Option<u32>,
    pub double_tap_window_ms: Option<u32>,
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
    pub keyboard_overrides: HashMap<String, KeyboardOverride>,
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

    /// Get effective config for a specific keyboard
    pub fn for_keyboard(&self, keyboard_id: &str) -> Config {
        let mut config = self.clone();

        if let Some(override_cfg) = self.keyboard_overrides.get(keyboard_id) {
            // Apply settings overrides
            if let Some(settings) = &override_cfg.settings {
                if let Some(term) = settings.tapping_term_ms {
                    config.tapping_term_ms = term;
                }
                if let Some(window) = settings.double_tap_window_ms {
                    config.double_tap_window_ms = Some(window);
                }
            }

            // Apply keymap overrides
            if let Some(keymap) = &override_cfg.keymap {
                if let Some(base) = &keymap.base_remaps {
                    config.remaps = base.clone();
                }
                if let Some(layers) = &keymap.layers {
                    config.layers = layers.clone();
                }
                if let Some(game) = &keymap.game_mode_remaps {
                    config.game_mode.remaps = game.clone();
                }
            }
        }

        config
    }

    /// Save only enabled_keyboards field, preserving rest of file
    pub fn save_enabled_keyboards_only(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = content.lines().collect();

        let mut new_lines = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            if line.trim().starts_with("enabled_keyboards:") {
                // Found field - replace with new value
                if let Some(keyboards) = &self.enabled_keyboards {
                    if keyboards.is_empty() {
                        new_lines.push("    enabled_keyboards: None,".to_string());
                    } else {
                        new_lines.push("    enabled_keyboards: Some([".to_string());
                        for kb in keyboards {
                            new_lines.push(format!("        \"{}\",", kb));
                        }
                        new_lines.push("    ]),".to_string());
                    }
                } else {
                    new_lines.push("    enabled_keyboards: None,".to_string());
                }

                // Skip original field (handle both Some([...]) and None)
                i += 1;
                while i < lines.len() {
                    let skip_line = lines[i];
                    if skip_line.trim().ends_with("],") || skip_line.trim() == "None," {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            } else {
                new_lines.push(line.to_string());
                i += 1;
            }
        }

        // Atomic write using temp file
        let temp_path = path.with_extension("ron.tmp");
        std::fs::write(&temp_path, new_lines.join("\n") + "\n")?;
        std::fs::rename(&temp_path, path)?;

        Ok(())
    }
}
