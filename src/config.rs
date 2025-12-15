use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Config {
    pub tapping_term_ms: u64,
    pub enable_game_mode_auto: bool,
    pub enable_socd: bool,
    pub password: Option<String>,
    /// Set of hardware IDs for enabled keyboards (if empty, all keyboards enabled)
    pub enabled_keyboards: Option<HashSet<String>>,
    /// Per-keyboard key remapping configuration
    pub key_remapping: Option<HashMap<String, KeyRemapping>>,
}

/// Key remapping configuration for a keyboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRemapping {
    /// Map Caps Lock to Esc (default: true)
    #[serde(default = "default_true")]
    pub caps_to_esc: bool,
    /// Map Esc to Grave/Tilde (default: false, normally Esc → Caps Lock)
    #[serde(default)]
    pub esc_to_grave: bool,
}

const fn default_true() -> bool {
    true
}

impl Default for KeyRemapping {
    fn default() -> Self {
        Self {
            caps_to_esc: true,
            esc_to_grave: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tapping_term_ms: 130,
            enable_game_mode_auto: true,
            enable_socd: true,
            password: None,
            enabled_keyboards: None, // None means all keyboards enabled
            key_remapping: None, // None means use default remapping
        }
    }
}

impl Config {
    #[allow(dead_code)]
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Self {
        fs::read_to_string(path).map_or_else(|_| Self::default(), |contents| toml::from_str(&contents).unwrap_or_default())
    }

    #[allow(dead_code)]
    pub fn save<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }
}
