#![allow(clippy::inline_always)]

use evdev::Key;
use std::time::Instant;

use crate::config::KeyRemapping;
use crate::socd::SocdCleaner;

/// What a specific key press is doing (recorded when pressed, replayed on release)
#[derive(Debug, Clone, Copy)]
#[repr(u8)]  // Optimize discriminant to single byte
pub enum Action {
    /// This key press activated a modifier
    Modifier(Key),
    /// This key press emitted a regular key
    RegularKey(Key),
    /// This key is being handled by SOCD cleaner (don't release manually)
    SocdManaged,
    /// This key activated nav layer
    NavLayerActivation,
    /// Home row mod waiting for decision (tap or hold)
    HomeRowModPending { hrm_key: Key },
    /// Home row mod holding the base key (double-tap-and-hold)
    HomeRowModHoldingBase { base_key: Key },
}

/// What a physical key is currently doing
/// Fixed-size array for max 2 actions - zero allocations!
#[derive(Debug, Clone, Copy)]
#[repr(C)]  // Predictable memory layout
pub struct KeyAction {
    // Fixed array: max 2 actions (most keys = 1, some = 2, never more)
    actions: [Option<Action>; 2],
    // Track number of actions for fast iteration
    count: u8,
}

impl Default for KeyAction {
    #[inline(always)]
    fn default() -> Self {
        Self::empty()
    }
}

impl KeyAction {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            actions: [None, None],
            count: 0,
        }
    }

    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            actions: [None, None],
            count: 0,
        }
    }

    #[inline(always)]
    pub const fn add(&mut self, action: Action) {
        self.actions[self.count as usize] = Some(action);
        self.count += 1;
    }

    #[inline(always)]
    const fn is_occupied(&self) -> bool {
        self.count > 0
    }

    #[inline(always)]
    pub const fn clear(&mut self) {
        self.actions = [None, None];
        self.count = 0;
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &Action> {
        self.actions[0..self.count as usize].iter().filter_map(|a| a.as_ref())
    }
}

/// Reference counting for modifiers (fast array-based lookup)
#[derive(Debug, Clone)]
#[repr(C, align(8))]  // Align to cache line for better performance
pub struct ModifierState {
    counts: [u8; 8], // Fixed size array for speed
}

impl ModifierState {
    #[inline(always)]
    pub const fn new() -> Self {
        Self { counts: [0; 8] }
    }

    #[inline(always)]
    const fn modifier_index(key: Key) -> Option<usize> {
        match key {
            Key::KEY_LEFTSHIFT => Some(0),
            Key::KEY_RIGHTSHIFT => Some(1),
            Key::KEY_LEFTCTRL => Some(2),
            Key::KEY_RIGHTCTRL => Some(3),
            Key::KEY_LEFTALT => Some(4),
            Key::KEY_RIGHTALT => Some(5),
            Key::KEY_LEFTMETA => Some(6),
            Key::KEY_RIGHTMETA => Some(7),
            _ => None,
        }
    }

    #[inline(always)]
    const fn index_to_key(idx: usize) -> Key {
        match idx {
            0 => Key::KEY_LEFTSHIFT,
            1 => Key::KEY_RIGHTSHIFT,
            2 => Key::KEY_LEFTCTRL,
            3 => Key::KEY_RIGHTCTRL,
            4 => Key::KEY_LEFTALT,
            5 => Key::KEY_RIGHTALT,
            6 => Key::KEY_LEFTMETA,
            7 => Key::KEY_RIGHTMETA,
            _ => Key::KEY_RESERVED,
        }
    }

    pub fn get_active_modifiers(&self) -> [Option<Key>; 8] {
        let mut result = [None; 8];
        for (i, &count) in self.counts.iter().enumerate() {
            if count > 0 {
                result[i] = Some(Self::index_to_key(i));
            }
        }
        result
    }

    #[inline(always)]
    pub const fn increment(&mut self, key: Key) {
        if let Some(idx) = Self::modifier_index(key) {
            self.counts[idx] = self.counts[idx].saturating_add(1);
        }
    }

    #[inline(always)]
    pub const fn decrement(&mut self, key: Key) -> bool {
        if let Some(idx) = Self::modifier_index(key) {
            if self.counts[idx] > 0 {
                self.counts[idx] -= 1;
                return self.counts[idx] == 0; // Return true if we should release
            }
        }
        true // If not tracked, release immediately
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]  // Pack tightly, no padding
pub struct HomeRowMod {
    pub modifier: Key,
    pub base_key: Key,
}

pub struct KeyboardState {
    // Memory layout optimized: hot fields first, cold fields last
    // TRUE O(1) array lookup by key code (0-255) - NO HASHING!
    // Box to save stack space (256 * ~40 bytes = 10KB on stack otherwise)
    held_keys: Box<[KeyAction; 256]>,
    // Pending home row mods as bit flags (8 keys = 1 byte!)
    pending_hrm_keys: u8,
    // Last tap times for home row mods (8 keys for double-tap detection)
    // None means never tapped, Some(Instant) is the last tap time
    hrm_last_tap_times: [Option<Instant>; 8],
    // Double-tap window in milliseconds
    double_tap_window_ms: u64,
    // Reference counting for modifiers
    pub modifier_state: ModifierState,
    pub socd_cleaner: SocdCleaner,
    // Key remapping configuration (stack-allocated, ~2 bytes)
    pub key_remapping: KeyRemapping,
    // Frequently accessed bools (packed together for cache)
    pub game_mode: bool,
    pub nav_layer_active: bool,
    pub password_typed_in_nav: bool,
    pub caps_lock_on: bool,
    // Cold fields (rarely accessed) - Box to save stack space
    pub password: Option<Box<str>>,
}

impl KeyboardState {
    pub fn new(password: Option<String>, key_remapping: KeyRemapping, double_tap_window_ms: u64) -> Self {
        // Create array with vec and convert to boxed array
        let mut held_keys_vec = Vec::with_capacity(256);
        for _ in 0..256 {
            held_keys_vec.push(KeyAction::empty());
        }
        let held_keys: Box<[KeyAction; 256]> = held_keys_vec.into_boxed_slice().try_into().unwrap();

        Self {
            held_keys,
            // Bit flags: 0 = no pending keys
            pending_hrm_keys: 0,
            // Initialize all tap times to None
            hrm_last_tap_times: [None; 8],
            double_tap_window_ms,
            modifier_state: ModifierState::new(),
            socd_cleaner: SocdCleaner::new(),
            key_remapping,
            game_mode: false,
            nav_layer_active: false,
            password_typed_in_nav: false,
            caps_lock_on: false,
            // Box password to save stack space (cold data, rarely accessed)
            password: password.map(|s| s.into_boxed_str()),
        }
    }

    // TRUE O(1) operations - direct array indexing, zero hashing!
    #[inline(always)]
    #[allow(dead_code)]
    fn get_held_key(&self, key: Key) -> Option<&KeyAction> {
        let slot = &self.held_keys[key.code() as usize];
        if slot.is_occupied() {
            Some(slot)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_held_key_mut(&mut self, key: Key) -> Option<&mut KeyAction> {
        let slot = &mut self.held_keys[key.code() as usize];
        if slot.is_occupied() {
            Some(slot)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn insert_held_key(&mut self, key: Key, action: KeyAction) {
        self.held_keys[key.code() as usize] = action;
    }

    #[inline(always)]
    pub fn remove_held_key(&mut self, key: Key) -> Option<KeyAction> {
        let slot = &mut self.held_keys[key.code() as usize];
        if slot.is_occupied() {
            let action = *slot;
            *slot = KeyAction::empty();
            Some(action)
        } else {
            None
        }
    }

    // Bit flag operations for home row mod tracking (8 keys = 8 bits)
    #[inline(always)]
    const fn hrm_key_to_bit(key: Key) -> Option<u8> {
        match key {
            Key::KEY_A => Some(0),
            Key::KEY_S => Some(1),
            Key::KEY_D => Some(2),
            Key::KEY_F => Some(3),
            Key::KEY_J => Some(4),
            Key::KEY_K => Some(5),
            Key::KEY_L => Some(6),
            Key::KEY_SEMICOLON => Some(7),
            _ => None,
        }
    }

    #[inline(always)]
    pub const fn is_hrm_pending(&self, key: Key) -> bool {
        if let Some(bit) = Self::hrm_key_to_bit(key) {
            (self.pending_hrm_keys & (1 << bit)) != 0
        } else {
            false
        }
    }

    #[inline(always)]
    pub const fn set_hrm_pending(&mut self, key: Key) {
        if let Some(bit) = Self::hrm_key_to_bit(key) {
            self.pending_hrm_keys |= 1 << bit;
        }
    }

    #[inline(always)]
    pub const fn clear_hrm_pending(&mut self, key: Key) {
        if let Some(bit) = Self::hrm_key_to_bit(key) {
            self.pending_hrm_keys &= !(1 << bit);
        }
    }

    #[inline(always)]
    pub const fn has_pending_hrm(&self) -> bool {
        self.pending_hrm_keys != 0
    }

    // Double-tap detection for home row mods
    #[inline(always)]
    pub fn set_hrm_last_tap(&mut self, key: Key) {
        if let Some(bit) = Self::hrm_key_to_bit(key) {
            self.hrm_last_tap_times[bit as usize] = Some(Instant::now());
        }
    }

    #[inline(always)]
    pub fn is_double_tap(&self, key: Key) -> bool {
        if let Some(bit) = Self::hrm_key_to_bit(key) {
            if let Some(last_tap) = self.hrm_last_tap_times[bit as usize] {
                let elapsed = last_tap.elapsed().as_millis();
                return elapsed <= self.double_tap_window_ms as u128;
            }
        }
        false
    }

    // Home row mod configuration
    #[inline(always)]
    pub const fn get_home_row_mod(key: Key) -> Option<HomeRowMod> {
        match key {
            // Left hand home row
            Key::KEY_A => Some(HomeRowMod {
                modifier: Key::KEY_LEFTMETA,
                base_key: Key::KEY_A,
            }),
            Key::KEY_S => Some(HomeRowMod {
                modifier: Key::KEY_LEFTALT,
                base_key: Key::KEY_S,
            }),
            Key::KEY_D => Some(HomeRowMod {
                modifier: Key::KEY_LEFTCTRL,
                base_key: Key::KEY_D,
            }),
            Key::KEY_F => Some(HomeRowMod {
                modifier: Key::KEY_LEFTSHIFT,
                base_key: Key::KEY_F,
            }),
            // Right hand home row
            Key::KEY_J => Some(HomeRowMod {
                modifier: Key::KEY_RIGHTSHIFT,
                base_key: Key::KEY_J,
            }),
            Key::KEY_K => Some(HomeRowMod {
                modifier: Key::KEY_RIGHTCTRL,
                base_key: Key::KEY_K,
            }),
            Key::KEY_L => Some(HomeRowMod {
                modifier: Key::KEY_RIGHTALT,
                base_key: Key::KEY_L,
            }),
            Key::KEY_SEMICOLON => Some(HomeRowMod {
                modifier: Key::KEY_RIGHTMETA,
                base_key: Key::KEY_SEMICOLON,
            }),
            _ => None,
        }
    }

    #[inline(always)]
    pub const fn is_home_row_mod(key: Key) -> bool {
        matches!(
            key,
            Key::KEY_A
                | Key::KEY_S
                | Key::KEY_D
                | Key::KEY_F
                | Key::KEY_J
                | Key::KEY_K
                | Key::KEY_L
                | Key::KEY_SEMICOLON
        )
    }

    #[inline(always)]
    pub const fn is_wasd_key(key: Key) -> bool {
        matches!(key, Key::KEY_W | Key::KEY_A | Key::KEY_S | Key::KEY_D)
    }
}
