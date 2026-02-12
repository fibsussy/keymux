/// Double-Tap (DT) processor - QMK-inspired tap dance with proper hold support
///
/// Timing Model:
/// - tapping_term_ms: How long to hold before first action activates as hold
/// - double_tap_window_ms: Total window from first press to detect double-tap
///
/// State Machine:
/// 1. Unpressed → Press → Pending (start timer from FIRST PRESS)
/// 2. From Pending:
///    - Hold > tapping_term → Holding (emit/hold first action)
///    - Release before tapping_term → Tapped (wait for second tap)
/// 3. From Tapped:
///    - Second press within double_tap_window (from FIRST PRESS!) → DoubleTapping
///    - Timeout expires (double_tap_window from FIRST PRESS) → emit single-tap
/// 4. From DoubleTapping:
///    - If held → continue holding second action
///    - If released → release second action
/// 5. From Holding:
///    - On release → release first action
///
/// Key behaviors:
/// - ALL timing is measured from the FIRST PRESS (not from release!)
/// - Single-tap emits when double_tap_window expires (even if no other key pressed)
/// - Hold activates at tapping_term (typically < double_tap_window)
/// - Double-tap must complete within double_tap_window from first press
use crate::keycode::KeyCode;
use std::collections::HashMap;
use std::time::Instant;

/// State of a double-tap key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtState {
    /// First press, determining if tap or hold
    Pending,
    /// Held beyond tapping_term, emitting first action as hold
    Holding,
    /// Released quickly, waiting for potential second tap
    Tapped,
    /// Second press detected, emitting second action
    DoubleTapping,
}

/// Double-tap key tracking
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DtKey {
    /// Physical keycode
    pub keycode: KeyCode,
    /// First action (single-tap/hold)
    pub first_action_key: KeyCode,
    /// Second action (double-tap)
    pub second_action_key: KeyCode,
    /// When first press occurred
    pub first_press_at: Instant,
    /// When first release occurred (if released)
    pub first_release_at: Option<Instant>,
    /// Current state
    pub state: DtState,
    /// Has the action been emitted yet?
    pub action_emitted: bool,
}

impl DtKey {
    pub fn new(keycode: KeyCode, first_key: KeyCode, second_key: KeyCode) -> Self {
        Self {
            keycode,
            first_action_key: first_key,
            second_action_key: second_key,
            first_press_at: Instant::now(),
            first_release_at: None,
            state: DtState::Pending,
            action_emitted: false,
        }
    }

    /// Time since first press
    pub fn elapsed_since_press(&self) -> u128 {
        self.first_press_at.elapsed().as_millis()
    }

    /// Time since first release (if released)
    #[allow(dead_code)]
    pub fn elapsed_since_release(&self) -> Option<u128> {
        self.first_release_at.map(|t| t.elapsed().as_millis())
    }
}

/// Result of DT processing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DtResolution {
    /// Start emitting first action as hold (press it)
    HoldFirst(KeyCode),
    /// Release held first action
    ReleaseFirst(KeyCode),
    /// Emit first action as tap (press+release)
    TapFirst(KeyCode),
    /// Start emitting second action (press it) - for double-tap
    PressSecond(KeyCode),
    /// Release second action
    ReleaseSecond(KeyCode),
    /// Still undecided
    Undecided,
}

/// Double-Tap processor configuration
#[derive(Debug, Clone)]
pub struct DtConfig {
    /// Tapping term - hold longer than this = hold first action (ms)
    pub tapping_term_ms: u32,
    /// Double-tap window - time to wait for second tap after first release (ms)
    pub double_tap_window_ms: u64,
}

impl Default for DtConfig {
    fn default() -> Self {
        Self {
            tapping_term_ms: 200,      // Standard tapping term
            double_tap_window_ms: 250, // Window for second tap
        }
    }
}

/// Double-Tap processor - manages all DT keys
pub struct DtProcessor {
    /// Config
    config: DtConfig,

    /// Currently tracked DT keys
    tracked_keys: HashMap<KeyCode, DtKey>,
}

impl DtProcessor {
    /// Create new DT processor
    pub fn new(config: DtConfig) -> Self {
        Self {
            config,
            tracked_keys: HashMap::new(),
        }
    }

    /// Handle key press
    pub fn on_press(
        &mut self,
        keycode: KeyCode,
        first_key: KeyCode,
        second_key: KeyCode,
    ) -> DtResolution {
        if let Some(dt_key) = self.tracked_keys.get_mut(&keycode) {
            // Already tracking this key - check if it's a second tap
            if dt_key.state == DtState::Tapped {
                // Check if within double-tap window FROM FIRST PRESS (not release!)
                // This matches QMK behavior - the entire double-tap sequence
                // must happen within the double_tap_window_ms
                if dt_key.elapsed_since_press() <= self.config.double_tap_window_ms as u128 {
                    // Double-tap detected! Emit second action
                    dt_key.state = DtState::DoubleTapping;
                    dt_key.action_emitted = true;
                    return DtResolution::PressSecond(dt_key.second_action_key);
                }
            }

            // If not a valid double-tap, remove old tracking and start fresh
            self.tracked_keys.remove(&keycode);
        }

        // First press - start tracking
        let dt_key = DtKey::new(keycode, first_key, second_key);
        self.tracked_keys.insert(keycode, dt_key);

        DtResolution::Undecided
    }

    /// Handle key release
    pub fn on_release(&mut self, keycode: KeyCode) -> DtResolution {
        if let Some(dt_key) = self.tracked_keys.get_mut(&keycode) {
            match dt_key.state {
                DtState::Pending => {
                    // Released quickly - transition to Tapped state
                    dt_key.state = DtState::Tapped;
                    dt_key.first_release_at = Some(Instant::now());
                    DtResolution::Undecided
                }
                DtState::Holding => {
                    // Was holding first action - release it
                    let key = dt_key.first_action_key;
                    self.tracked_keys.remove(&keycode);
                    DtResolution::ReleaseFirst(key)
                }
                DtState::DoubleTapping => {
                    // Was double-tapping - release second action
                    let key = dt_key.second_action_key;
                    self.tracked_keys.remove(&keycode);
                    DtResolution::ReleaseSecond(key)
                }
                DtState::Tapped => {
                    // Shouldn't happen (already released), but handle gracefully
                    DtResolution::Undecided
                }
            }
        } else {
            DtResolution::Undecided
        }
    }

    /// Check for timeouts and state transitions
    /// Should be called periodically (e.g., on every key event)
    pub fn check_timeouts(&mut self) -> Vec<(KeyCode, DtResolution)> {
        let mut resolutions = Vec::new();
        let tapping_term = self.config.tapping_term_ms as u128;
        let double_tap_window = self.config.double_tap_window_ms as u128;

        // Collect keys that need state transitions
        let mut transitions = Vec::new();

        for (keycode, dt_key) in &self.tracked_keys {
            match dt_key.state {
                DtState::Pending => {
                    // Check if held beyond tapping term → transition to Holding
                    // BUT: only if tapping_term < double_tap_window
                    // This allows hold to activate while still in double-tap window
                    if dt_key.elapsed_since_press() > tapping_term {
                        transitions.push((*keycode, DtState::Holding));
                    }
                }
                DtState::Tapped => {
                    // Check if double-tap window expired → emit single-tap
                    // Use elapsed_since_press() for consistency - the entire interaction
                    // must complete within double_tap_window_ms
                    if dt_key.elapsed_since_press() > double_tap_window {
                        transitions.push((*keycode, DtState::Tapped)); // Mark for cleanup
                    }
                }
                _ => {}
            }
        }

        // Apply state transitions and generate resolutions
        for (keycode, new_state) in transitions {
            if let Some(dt_key) = self.tracked_keys.get_mut(&keycode) {
                match new_state {
                    DtState::Holding => {
                        // Transition to holding first action
                        dt_key.state = DtState::Holding;
                        dt_key.action_emitted = true;
                        resolutions
                            .push((keycode, DtResolution::HoldFirst(dt_key.first_action_key)));
                    }
                    DtState::Tapped => {
                        // Timeout expired in Tapped state → emit single-tap
                        let key = dt_key.first_action_key;
                        self.tracked_keys.remove(&keycode);
                        resolutions.push((keycode, DtResolution::TapFirst(key)));
                    }
                    _ => {}
                }
            }
        }

        resolutions
    }

    /// Get currently tracked keys (for debugging)
    #[allow(dead_code)]
    pub fn tracked_count(&self) -> usize {
        self.tracked_keys.len()
    }
}
