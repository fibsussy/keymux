#[cfg(test)]
mod keyboard_tests {
    use super::super::*;
    use evdev::Key;

    #[test]
    fn test_socd_basic() {
        let mut socd = SocdCleaner::new();

        // Press W
        let keys = socd.handle_press(Key::KEY_W);
        assert!(keys.contains(&Key::KEY_W));

        // Press S (opposite direction) - should switch to S
        let keys = socd.handle_press(Key::KEY_S);
        assert!(!keys.contains(&Key::KEY_W));
        assert!(keys.contains(&Key::KEY_S));

        // Release S - should go back to W
        let keys = socd.handle_release(Key::KEY_S);
        assert!(keys.contains(&Key::KEY_W));
        assert!(!keys.contains(&Key::KEY_S));
    }

    #[test]
    fn test_socd_horizontal() {
        let mut socd = SocdCleaner::new();

        // Press A
        let keys = socd.handle_press(Key::KEY_A);
        assert!(keys.contains(&Key::KEY_A));

        // Press D (opposite) - last input wins
        let keys = socd.handle_press(Key::KEY_D);
        assert!(!keys.contains(&Key::KEY_A));
        assert!(keys.contains(&Key::KEY_D));
    }

    #[test]
    fn test_socd_diagonal() {
        let mut socd = SocdCleaner::new();

        // Press W
        let keys = socd.handle_press(Key::KEY_W);
        assert!(keys.contains(&Key::KEY_W));

        // Press A (perpendicular) - both should be active
        let keys = socd.handle_press(Key::KEY_A);
        assert!(keys.contains(&Key::KEY_W));
        assert!(keys.contains(&Key::KEY_A));
    }

    #[test]
    fn test_home_row_mod_init() {
        let state = KeyboardState::new();

        // Check home row mods are initialized
        assert!(state.home_row_mods.contains_key(&Key::KEY_A));
        assert!(state.home_row_mods.contains_key(&Key::KEY_S));
        assert!(state.home_row_mods.contains_key(&Key::KEY_D));
        assert!(state.home_row_mods.contains_key(&Key::KEY_F));
        assert!(state.home_row_mods.contains_key(&Key::KEY_J));
        assert!(state.home_row_mods.contains_key(&Key::KEY_K));
        assert!(state.home_row_mods.contains_key(&Key::KEY_L));
        assert!(state.home_row_mods.contains_key(&Key::KEY_SEMICOLON));

        // Check correct modifiers
        let hrm_a = state.home_row_mods.get(&Key::KEY_A).unwrap();
        assert_eq!(hrm_a.modifier, Key::KEY_LEFTMETA);
        assert_eq!(hrm_a.base_key, Key::KEY_A);

        let hrm_d = state.home_row_mods.get(&Key::KEY_D).unwrap();
        assert_eq!(hrm_d.modifier, Key::KEY_LEFTCTRL);
        assert_eq!(hrm_d.base_key, Key::KEY_D);
    }

    #[test]
    fn test_game_mode_entry_detection() {
        let mut state = KeyboardState::new();

        // Rapid WASD should trigger game mode
        for _ in 0..4 {
            state.check_game_mode_entry(Key::KEY_W, true);
            state.check_game_mode_entry(Key::KEY_A, true);
            state.check_game_mode_entry(Key::KEY_S, true);
            state.check_game_mode_entry(Key::KEY_D, true);
        }

        // Should have entered game mode
        assert!(state.game_mode);
    }

    #[test]
    fn test_wasd_key_detection() {
        assert!(KeyboardState::is_wasd_key(Key::KEY_W));
        assert!(KeyboardState::is_wasd_key(Key::KEY_A));
        assert!(KeyboardState::is_wasd_key(Key::KEY_S));
        assert!(KeyboardState::is_wasd_key(Key::KEY_D));
        assert!(!KeyboardState::is_wasd_key(Key::KEY_Q));
        assert!(!KeyboardState::is_wasd_key(Key::KEY_E));
    }
}
