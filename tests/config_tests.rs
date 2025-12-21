use keyboard_middleware::config::{Config, KeyCode, Action, Layer};
use std::collections::HashMap;
use std::fs;

#[test]
fn test_load_config_from_file() {
    // Read the actual config file
    let config_path = dirs::config_dir()
        .expect("Failed to get config dir")
        .join("keyboard-middleware")
        .join("config.ron");

    let config_str = fs::read_to_string(&config_path)
        .expect("Failed to read config.ron");

    // Parse it
    let config: Config = ron::from_str(&config_str)
        .expect("Failed to parse config.ron");

    // Basic sanity checks
    assert!(config.enabled_keyboards.is_some());
    assert_eq!(config.tapping_term_ms, 130);
}

#[test]
fn test_validate_all_config_fields() {
    let config_path = dirs::config_dir()
        .expect("Failed to get config dir")
        .join("keyboard-middleware")
        .join("config.ron");

    let config_str = fs::read_to_string(&config_path)
        .expect("Failed to read config.ron");

    let config: Config = ron::from_str(&config_str)
        .expect("Failed to parse config.ron");

    // Validate top-level fields
    assert_eq!(config.tapping_term_ms, 130);
    assert_eq!(config.double_tap_window_ms, None);
    assert!(config.password.is_some());
    assert!(config.enabled_keyboards.is_some());

    // Validate remaps exist and contain expected mappings
    assert!(!config.remaps.is_empty());
    assert_eq!(config.remaps.get(&KeyCode::KC_CAPS), Some(&Action::Key(KeyCode::KC_ESC)));
    assert_eq!(config.remaps.get(&KeyCode::KC_ESC), Some(&Action::Key(KeyCode::KC_GRV)));
    assert_eq!(config.remaps.get(&KeyCode::KC_LALT), Some(&Action::TO(Layer::L_NAV)));

    // Validate homerow mods
    assert_eq!(config.remaps.get(&KeyCode::KC_A), Some(&Action::HR(KeyCode::KC_A, KeyCode::KC_LGUI)));
    assert_eq!(config.remaps.get(&KeyCode::KC_S), Some(&Action::HR(KeyCode::KC_S, KeyCode::KC_LALT)));

    // Validate layers
    assert!(!config.layers.is_empty());
    let nav_layer = config.layers.get(&Layer::L_NAV).expect("L_NAV layer should exist");
    assert!(!nav_layer.remaps.is_empty());
    assert_eq!(nav_layer.remaps.get(&KeyCode::KC_H), Some(&Action::Key(KeyCode::KC_LEFT)));
    assert_eq!(nav_layer.remaps.get(&KeyCode::KC_J), Some(&Action::Key(KeyCode::KC_DOWN)));

    // Validate game mode
    assert!(config.game_mode.auto_detect);
    assert_eq!(config.game_mode.detection_methods.len(), 4);
    assert_eq!(config.game_mode.process_tree_depth, 10);
    assert!(!config.game_mode.remaps.is_empty());

    // Validate SOCD config
    assert!(config.game_mode.socd.enabled);

    // Validate keyboard overrides is empty
    assert!(config.keyboard_overrides.is_empty());
}

#[test]
fn test_save_config_roundtrip() {
    // Load config
    let config_path = dirs::config_dir()
        .expect("Failed to get config dir")
        .join("keyboard-middleware")
        .join("config.ron");

    let original_str = fs::read_to_string(&config_path)
        .expect("Failed to read config.ron");

    let config: Config = ron::from_str(&original_str)
        .expect("Failed to parse config.ron");

    // Serialize back to RON
    let serialized = ron::ser::to_string_pretty(&config, ron::ser::PrettyConfig::default())
        .expect("Failed to serialize config");

    // Parse again
    let config2: Config = ron::from_str(&serialized)
        .expect("Failed to re-parse serialized config");

    // Configs should be equal - test all fields
    assert_eq!(config.tapping_term_ms, config2.tapping_term_ms);
    assert_eq!(config.double_tap_window_ms, config2.double_tap_window_ms);
    assert_eq!(config.enabled_keyboards, config2.enabled_keyboards);
    assert_eq!(config.password, config2.password);
    assert_eq!(config.remaps, config2.remaps);
    assert_eq!(config.layers, config2.layers);
    assert_eq!(config.game_mode, config2.game_mode);
    assert_eq!(config.keyboard_overrides, config2.keyboard_overrides);
}

#[test]
fn test_config_helper_methods() {
    // Test default_path works
    let path = Config::default_path().expect("Failed to get default path");
    assert!(path.to_string_lossy().contains("keyboard-middleware"));
    assert!(path.to_string_lossy().ends_with("config.ron"));

    // Test load from default path
    let config = Config::load(&path).expect("Failed to load config");
    assert_eq!(config.tapping_term_ms, 130);
}

#[test]
fn test_save_and_load_from_disk() {
    // Load original config
    let original_path = Config::default_path().expect("Failed to get default path");
    let original_config = Config::load(&original_path).expect("Failed to load config");

    // Save to temporary file
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join("test_config.ron");

    // Clean up if exists from previous test
    let _ = std::fs::remove_file(&temp_path);

    // Save config
    original_config.save(&temp_path).expect("Failed to save config");

    // Load it back
    let loaded_config = Config::load(&temp_path).expect("Failed to load saved config");

    // Verify they match
    assert_eq!(original_config.tapping_term_ms, loaded_config.tapping_term_ms);
    assert_eq!(original_config.enabled_keyboards, loaded_config.enabled_keyboards);
    assert_eq!(original_config.remaps, loaded_config.remaps);
    assert_eq!(original_config.layers, loaded_config.layers);
    assert_eq!(original_config.game_mode, loaded_config.game_mode);

    // Clean up
    let _ = std::fs::remove_file(&temp_path);
}
