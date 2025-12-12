use evdev::Key;
use std::collections::HashSet;

/// SOCD (Simultaneous Opposite Cardinal Directions) Cleaner
/// Implements "last input priority" to prevent impossible inputs in games
pub struct SocdCleaner {
    w_held: bool,
    a_held: bool,
    s_held: bool,
    d_held: bool,
    last_vertical: Option<Key>,
    last_horizontal: Option<Key>,
}

impl SocdCleaner {
    pub const fn new() -> Self {
        Self {
            w_held: false,
            a_held: false,
            s_held: false,
            d_held: false,
            last_vertical: None,
            last_horizontal: None,
        }
    }

    pub const fn reset(&mut self) {
        self.w_held = false;
        self.a_held = false;
        self.s_held = false;
        self.d_held = false;
        self.last_vertical = None;
        self.last_horizontal = None;
    }

    pub fn handle_press(&mut self, key: Key) -> HashSet<Key> {
        match key {
            Key::KEY_W => {
                self.w_held = true;
                self.last_vertical = Some(Key::KEY_W);
            }
            Key::KEY_A => {
                self.a_held = true;
                self.last_horizontal = Some(Key::KEY_A);
            }
            Key::KEY_S => {
                self.s_held = true;
                self.last_vertical = Some(Key::KEY_S);
            }
            Key::KEY_D => {
                self.d_held = true;
                self.last_horizontal = Some(Key::KEY_D);
            }
            _ => {}
        }

        self.compute_active_keys()
    }

    pub fn handle_release(&mut self, key: Key) -> HashSet<Key> {
        match key {
            Key::KEY_W => self.w_held = false,
            Key::KEY_A => self.a_held = false,
            Key::KEY_S => self.s_held = false,
            Key::KEY_D => self.d_held = false,
            _ => {}
        }

        self.compute_active_keys()
    }

    fn compute_active_keys(&self) -> HashSet<Key> {
        let mut active = HashSet::new();

        // Vertical resolution
        if self.w_held && !self.s_held {
            active.insert(Key::KEY_W);
        } else if self.s_held && !self.w_held {
            active.insert(Key::KEY_S);
        } else if self.w_held && self.s_held {
            // Both held: last input wins
            if let Some(last) = self.last_vertical {
                active.insert(last);
            }
        }

        // Horizontal resolution
        if self.a_held && !self.d_held {
            active.insert(Key::KEY_A);
        } else if self.d_held && !self.a_held {
            active.insert(Key::KEY_D);
        } else if self.a_held && self.d_held {
            // Both held: last input wins
            if let Some(last) = self.last_horizontal {
                active.insert(last);
            }
        }

        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_press() {
        let mut socd = SocdCleaner::new();
        let keys = socd.handle_press(Key::KEY_W);
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&Key::KEY_W));
    }

    #[test]
    fn test_opposite_directions() {
        let mut socd = SocdCleaner::new();
        socd.handle_press(Key::KEY_W);
        let keys = socd.handle_press(Key::KEY_S);

        // Last input priority: S should win
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&Key::KEY_S));
    }

    #[test]
    fn test_release_opposite() {
        let mut socd = SocdCleaner::new();
        socd.handle_press(Key::KEY_W);
        socd.handle_press(Key::KEY_S);
        let keys = socd.handle_release(Key::KEY_S);

        // After releasing S, W should be active
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&Key::KEY_W));
    }

    #[test]
    fn test_horizontal_and_vertical() {
        let mut socd = SocdCleaner::new();
        socd.handle_press(Key::KEY_W);
        let keys = socd.handle_press(Key::KEY_A);

        // Both should be active (no conflict)
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&Key::KEY_W));
        assert!(keys.contains(&Key::KEY_A));
    }
}
