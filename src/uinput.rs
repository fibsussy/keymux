use anyhow::Result;
use evdev::{uinput::VirtualDeviceBuilder, AttributeSet, EventType, InputEvent, Key};
use smallvec::SmallVec;
use std::thread;
use std::time::Duration;

// Constants for frequently used event values (avoid recomputation)
const SYN_REPORT: i32 = 0;
const SYN_CODE: u16 = 0;

pub struct VirtualKeyboard {
    device: evdev::uinput::VirtualDevice,
    // Simple 2-element array for SOCD (max one vertical + one horizontal)
    active_socd_keys: [Option<Key>; 2],
}

impl VirtualKeyboard {
    pub fn new() -> Result<Self> {
        let mut keys = AttributeSet::<Key>::new();

        // Register all keyboard keys
        for i in 0..256 {
            let key = Key::new(i);
            keys.insert(key);
        }

        let device = VirtualDeviceBuilder::new()?
            .name("Keyboard Middleware Virtual Keyboard")
            .with_keys(&keys)?
            .build()?;

        // Give udev/systemd time to recognize the device
        thread::sleep(Duration::from_millis(200));

        Ok(Self {
            device,
            active_socd_keys: [None; 2],
        })
    }

    #[inline(always)]
    pub fn press_key(&mut self, key: Key) -> Result<()> {
        let events = [
            InputEvent::new(EventType::KEY, key.code(), 1),
            InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT),
        ];
        self.device.emit(&events)?;
        Ok(())
    }

    #[inline(always)]
    pub fn release_key(&mut self, key: Key) -> Result<()> {
        let events = [
            InputEvent::new(EventType::KEY, key.code(), 0),
            InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT),
        ];
        self.device.emit(&events)?;
        Ok(())
    }

    #[inline(always)]
    pub fn tap_key(&mut self, key: Key) -> Result<()> {
        // Emit press + release as a single batch with SYN
        let events = [
            InputEvent::new(EventType::KEY, key.code(), 1),
            InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT),
            InputEvent::new(EventType::KEY, key.code(), 0),
            InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT),
        ];
        self.device.emit(&events)?;
        Ok(())
    }

    #[inline]
    pub fn update_socd_keys(&mut self, new_keys: [Option<Key>; 2]) -> Result<()> {
        // Ultra-optimized: batch ALL events into single emit
        let mut events = SmallVec::<[InputEvent; 8]>::new();

        // Release keys that are no longer active
        for &old_key_opt in &self.active_socd_keys {
            if let Some(old_key) = old_key_opt {
                // Check if this key is still in new_keys
                if !new_keys.contains(&Some(old_key)) {
                    events.push(InputEvent::new(EventType::KEY, old_key.code(), 0));
                    events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
                }
            }
        }

        // Press keys that are newly active
        for new_key_opt in new_keys {
            if let Some(new_key) = new_key_opt {
                // Check if this key was already active
                if !self.active_socd_keys.contains(&Some(new_key)) {
                    events.push(InputEvent::new(EventType::KEY, new_key.code(), 1));
                    events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
                }
            }
        }

        // Only emit if there are changes (common case: already correct state)
        if !events.is_empty() {
            self.device.emit(&events)?;
        }

        // Update state (direct copy)
        self.active_socd_keys = new_keys;
        Ok(())
    }

    pub fn type_string(&mut self, text: &str) -> Result<()> {
        // INSTANT typing - batch all events into single emit for maximum speed
        let mut events = SmallVec::<[InputEvent; 128]>::new();

        for ch in text.chars() {
            let (key, needs_shift) = char_to_key(ch);

            // Press shift if needed
            if needs_shift {
                events.push(InputEvent::new(EventType::KEY, Key::KEY_LEFTSHIFT.code(), 1));
                events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
            }

            // Press key
            events.push(InputEvent::new(EventType::KEY, key.code(), 1));
            events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));

            // Release key
            events.push(InputEvent::new(EventType::KEY, key.code(), 0));
            events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));

            // Release shift if needed
            if needs_shift {
                events.push(InputEvent::new(EventType::KEY, Key::KEY_LEFTSHIFT.code(), 0));
                events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
            }
        }

        // Emit ALL events at once - INSTANT password typing!
        self.device.emit(&events)?;
        Ok(())
    }

    pub fn type_string_with_mods(&mut self, text: &str, active_mods: &[Option<Key>], caps_lock_on: bool) -> Result<()> {
        // STOW/UNSTOW pattern: save mods AND caps lock, type clean, restore everything
        let mut events = SmallVec::<[InputEvent; 256]>::new();

        // STOW: Turn off caps lock if it's on (toggle state, not a modifier)
        if caps_lock_on {
            events.push(InputEvent::new(EventType::KEY, Key::KEY_CAPSLOCK.code(), 1));
            events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
            events.push(InputEvent::new(EventType::KEY, Key::KEY_CAPSLOCK.code(), 0));
            events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
        }

        // STOW: Release all active modifiers
        for &mod_key_opt in active_mods {
            if let Some(mod_key) = mod_key_opt {
                events.push(InputEvent::new(EventType::KEY, mod_key.code(), 0));
                events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
            }
        }

        // TYPE: Password in clean state (no modifiers or caps lock active)
        for ch in text.chars() {
            let (key, needs_shift) = char_to_key(ch);

            // Press shift if needed
            if needs_shift {
                events.push(InputEvent::new(EventType::KEY, Key::KEY_LEFTSHIFT.code(), 1));
                events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
            }

            // Press key
            events.push(InputEvent::new(EventType::KEY, key.code(), 1));
            events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));

            // Release key
            events.push(InputEvent::new(EventType::KEY, key.code(), 0));
            events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));

            // Release shift if needed
            if needs_shift {
                events.push(InputEvent::new(EventType::KEY, Key::KEY_LEFTSHIFT.code(), 0));
                events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
            }
        }

        // UNSTOW: Restore all previously active modifiers
        for &mod_key_opt in active_mods {
            if let Some(mod_key) = mod_key_opt {
                events.push(InputEvent::new(EventType::KEY, mod_key.code(), 1));
                events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
            }
        }

        // UNSTOW: Turn caps lock back on if it was on
        if caps_lock_on {
            events.push(InputEvent::new(EventType::KEY, Key::KEY_CAPSLOCK.code(), 1));
            events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
            events.push(InputEvent::new(EventType::KEY, Key::KEY_CAPSLOCK.code(), 0));
            events.push(InputEvent::new(EventType::SYNCHRONIZATION, SYN_CODE, SYN_REPORT));
        }

        // Emit ALL events at once - INSTANT with proper modifier AND caps lock handling!
        self.device.emit(&events)?;
        Ok(())
    }
}

const fn char_to_key(ch: char) -> (Key, bool) {
    match ch {
        'a' => (Key::KEY_A, false),
        'b' => (Key::KEY_B, false),
        'c' => (Key::KEY_C, false),
        'd' => (Key::KEY_D, false),
        'e' => (Key::KEY_E, false),
        'f' => (Key::KEY_F, false),
        'g' => (Key::KEY_G, false),
        'h' => (Key::KEY_H, false),
        'i' => (Key::KEY_I, false),
        'j' => (Key::KEY_J, false),
        'k' => (Key::KEY_K, false),
        'l' => (Key::KEY_L, false),
        'm' => (Key::KEY_M, false),
        'n' => (Key::KEY_N, false),
        'o' => (Key::KEY_O, false),
        'p' => (Key::KEY_P, false),
        'q' => (Key::KEY_Q, false),
        'r' => (Key::KEY_R, false),
        's' => (Key::KEY_S, false),
        't' => (Key::KEY_T, false),
        'u' => (Key::KEY_U, false),
        'v' => (Key::KEY_V, false),
        'w' => (Key::KEY_W, false),
        'x' => (Key::KEY_X, false),
        'y' => (Key::KEY_Y, false),
        'z' => (Key::KEY_Z, false),
        'A' => (Key::KEY_A, true),
        'B' => (Key::KEY_B, true),
        'C' => (Key::KEY_C, true),
        'D' => (Key::KEY_D, true),
        'E' => (Key::KEY_E, true),
        'F' => (Key::KEY_F, true),
        'G' => (Key::KEY_G, true),
        'H' => (Key::KEY_H, true),
        'I' => (Key::KEY_I, true),
        'J' => (Key::KEY_J, true),
        'K' => (Key::KEY_K, true),
        'L' => (Key::KEY_L, true),
        'M' => (Key::KEY_M, true),
        'N' => (Key::KEY_N, true),
        'O' => (Key::KEY_O, true),
        'P' => (Key::KEY_P, true),
        'Q' => (Key::KEY_Q, true),
        'R' => (Key::KEY_R, true),
        'S' => (Key::KEY_S, true),
        'T' => (Key::KEY_T, true),
        'U' => (Key::KEY_U, true),
        'V' => (Key::KEY_V, true),
        'W' => (Key::KEY_W, true),
        'X' => (Key::KEY_X, true),
        'Y' => (Key::KEY_Y, true),
        'Z' => (Key::KEY_Z, true),
        '0' => (Key::KEY_0, false),
        '1' => (Key::KEY_1, false),
        '2' => (Key::KEY_2, false),
        '3' => (Key::KEY_3, false),
        '4' => (Key::KEY_4, false),
        '5' => (Key::KEY_5, false),
        '6' => (Key::KEY_6, false),
        '7' => (Key::KEY_7, false),
        '8' => (Key::KEY_8, false),
        '9' => (Key::KEY_9, false),
        ')' => (Key::KEY_0, true),
        '!' => (Key::KEY_1, true),
        '@' => (Key::KEY_2, true),
        '#' => (Key::KEY_3, true),
        '$' => (Key::KEY_4, true),
        '%' => (Key::KEY_5, true),
        '^' => (Key::KEY_6, true),
        '&' => (Key::KEY_7, true),
        '*' => (Key::KEY_8, true),
        '(' => (Key::KEY_9, true),
        ' ' => (Key::KEY_SPACE, false),
        '-' => (Key::KEY_MINUS, false),
        '_' => (Key::KEY_MINUS, true),
        '=' => (Key::KEY_EQUAL, false),
        '+' => (Key::KEY_EQUAL, true),
        '[' => (Key::KEY_LEFTBRACE, false),
        '{' => (Key::KEY_LEFTBRACE, true),
        ']' => (Key::KEY_RIGHTBRACE, false),
        '}' => (Key::KEY_RIGHTBRACE, true),
        '\\' => (Key::KEY_BACKSLASH, false),
        '|' => (Key::KEY_BACKSLASH, true),
        ';' => (Key::KEY_SEMICOLON, false),
        ':' => (Key::KEY_SEMICOLON, true),
        '\'' => (Key::KEY_APOSTROPHE, false),
        '"' => (Key::KEY_APOSTROPHE, true),
        ',' => (Key::KEY_COMMA, false),
        '<' => (Key::KEY_COMMA, true),
        '.' => (Key::KEY_DOT, false),
        '>' => (Key::KEY_DOT, true),
        '/' => (Key::KEY_SLASH, false),
        '?' => (Key::KEY_SLASH, true),
        '`' => (Key::KEY_GRAVE, false),
        '~' => (Key::KEY_GRAVE, true),
        _ => (Key::KEY_SPACE, false), // Default to space for unknown chars
    }
}
