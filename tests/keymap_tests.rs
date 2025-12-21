use keyboard_middleware::config::{Action, Config, KeyCode};
use keyboard_middleware::keymap::{evdev_to_keycode, keycode_to_evdev, KeymapProcessor, ProcessResult};
use evdev::Key;

#[test]
fn test_evdev_to_keycode_conversion() {
    // Test letters
    assert_eq!(evdev_to_keycode(Key::KEY_A), Some(KeyCode::KC_A));
    assert_eq!(evdev_to_keycode(Key::KEY_Z), Some(KeyCode::KC_Z));

    // Test numbers
    assert_eq!(evdev_to_keycode(Key::KEY_1), Some(KeyCode::KC_1));
    assert_eq!(evdev_to_keycode(Key::KEY_0), Some(KeyCode::KC_0));

    // Test special keys
    assert_eq!(evdev_to_keycode(Key::KEY_ESC), Some(KeyCode::KC_ESC));
    assert_eq!(evdev_to_keycode(Key::KEY_CAPSLOCK), Some(KeyCode::KC_CAPS));
    assert_eq!(evdev_to_keycode(Key::KEY_SPACE), Some(KeyCode::KC_SPC));

    // Test modifiers
    assert_eq!(evdev_to_keycode(Key::KEY_LEFTCTRL), Some(KeyCode::KC_LCTL));
    assert_eq!(evdev_to_keycode(Key::KEY_LEFTSHIFT), Some(KeyCode::KC_LSFT));
}

#[test]
fn test_keycode_to_evdev_conversion() {
    // Test letters
    assert_eq!(keycode_to_evdev(KeyCode::KC_A), Key::KEY_A);
    assert_eq!(keycode_to_evdev(KeyCode::KC_Z), Key::KEY_Z);

    // Test special keys
    assert_eq!(keycode_to_evdev(KeyCode::KC_ESC), Key::KEY_ESC);
    assert_eq!(keycode_to_evdev(KeyCode::KC_CAPS), Key::KEY_CAPSLOCK);

    // Test modifiers
    assert_eq!(keycode_to_evdev(KeyCode::KC_LCTL), Key::KEY_LEFTCTRL);
}

#[test]
fn test_keymap_processor_simple_remap() {
    // Load config from file
    let config_path = Config::default_path().expect("Failed to get config path");
    let config = Config::load(&config_path).expect("Failed to load config");

    let mut processor = KeymapProcessor::new(&config);

    // Test CAPS -> ESC remap (from config)
    let result = processor.process_key(KeyCode::KC_CAPS, true);
    assert_eq!(result, ProcessResult::EmitKey(KeyCode::KC_ESC, true));

    let result = processor.process_key(KeyCode::KC_CAPS, false);
    assert_eq!(result, ProcessResult::EmitKey(KeyCode::KC_ESC, false));

    // Test ESC -> GRV remap (from config)
    let result = processor.process_key(KeyCode::KC_ESC, true);
    assert_eq!(result, ProcessResult::EmitKey(KeyCode::KC_GRV, true));
}

#[test]
fn test_keymap_processor_passthrough() {
    let config_path = Config::default_path().expect("Failed to get config path");
    let config = Config::load(&config_path).expect("Failed to load config");

    let mut processor = KeymapProcessor::new(&config);

    // Test unmapped key (should pass through)
    let result = processor.process_key(KeyCode::KC_B, true);
    assert_eq!(result, ProcessResult::EmitKey(KeyCode::KC_B, true));

    let result = processor.process_key(KeyCode::KC_B, false);
    assert_eq!(result, ProcessResult::EmitKey(KeyCode::KC_B, false));
}

#[test]
fn test_keymap_processor_multiple_remaps() {
    let config_path = Config::default_path().expect("Failed to get config path");
    let config = Config::load(&config_path).expect("Failed to load config");

    let mut processor = KeymapProcessor::new(&config);

    // Test multiple remaps in sequence
    assert_eq!(
        processor.process_key(KeyCode::KC_CAPS, true),
        ProcessResult::EmitKey(KeyCode::KC_ESC, true)
    );
    assert_eq!(
        processor.process_key(KeyCode::KC_ESC, true),
        ProcessResult::EmitKey(KeyCode::KC_GRV, true)
    );
    assert_eq!(
        processor.process_key(KeyCode::KC_B, true),
        ProcessResult::EmitKey(KeyCode::KC_B, true)
    );
}

#[test]
fn test_roundtrip_conversion() {
    // Test that converting back and forth preserves the value
    let keycodes = vec![
        KeyCode::KC_A, KeyCode::KC_ESC, KeyCode::KC_SPC,
        KeyCode::KC_LCTL, KeyCode::KC_F1, KeyCode::KC_1,
    ];

    for kc in keycodes {
        let evdev_key = keycode_to_evdev(kc);
        let back_to_kc = evdev_to_keycode(evdev_key);
        assert_eq!(back_to_kc, Some(kc));
    }
}
