# Keyboard Middleware - Architecture & Features

## Overview

This document describes the architecture and comprehensive feature set of the keyboard middleware system, designed to provide high-performance, fully customizable key processing with robust management capabilities.

This is a **QMK-inspired keyboard remapping daemon** that runs on Linux, providing instant key processing with zero-latency while supporting advanced features like home row mods, layers, SOCD, game mode detection, and command execution.

## Design Goals

1. **Zero-latency hot path**: Key event processing must be synchronous and allocation-free
2. **100% customizable**: Every key on every keyboard can be remapped, no hardcoded layouts
3. **Async management**: Device discovery, configuration, and user management should be async
4. **Multi-user support**: Proper session-based keyboard ownership
5. **Self-healing**: Automatic recovery from failures and clean error handling
6. **Hot-reload**: Configuration changes without service interruption

---

## Feature Set (QMK-Inspired)

### Core Features

#### 1. **Unlimited Custom Layers**

Layers are **fully generic** and string-based. You can create as many layers as you want with any names you want.

**Reserved Layers:**
- `"base"` - The default layer, always active
- `"game_mode"` - Special layer activated during gaming

**Example:**
```ron
layers: {
    "nav": ( remaps: { /* vim navigation */ } ),
    "num": ( remaps: { /* numpad */ } ),
    "symbols": ( remaps: { /* symbols */ } ),
    "coding": ( remaps: { /* IDE shortcuts */ } ),
    "media": ( remaps: { /* media controls */ } ),
    "custom_layer_123": ( remaps: { /* anything */ } ),
}
```

**How it works:**
- Layers stack on top of base layer
- Key lookups check: Game Mode → Current Layer → Base Layer
- Switch layers with `TO("layer_name")`
- Releasing the layer switch key returns to base

**Use cases:**
- Navigation layer (Vim arrows on hjkl)
- Number layer (numpad on home row)
- Symbol layer (programming symbols)
- Media control layer
- IDE/app-specific layers

---

#### 2. **Home Row Mods (HR)**

**Tap for letter, hold for modifier** - works on **ANY key**, not just ASDF JKL;

**Syntax:** `HR(tap_key, hold_key)`

**Example:**
```ron
remaps: {
    // Traditional QWERTY home row mods
    KC_A: HR(KC_A, KC_LGUI),   // Tap A, hold for Super
    KC_S: HR(KC_S, KC_LALT),   // Tap S, hold for Alt
    KC_D: HR(KC_D, KC_LCTL),   // Tap D, hold for Ctrl
    KC_F: HR(KC_F, KC_LSFT),   // Tap F, hold for Shift
    
    // Colemak/Dvorak/custom home rows work too!
    KC_T: HR(KC_T, KC_LGUI),
    KC_N: HR(KC_N, KC_LSFT),
    
    // Even non-home-row keys!
    KC_SPC: HR(KC_SPC, KC_LSFT),
}
```

**Features:**
- **Permissive hold**: If another key is pressed while holding, immediately activates modifier
- **Double-tap-and-hold**: Double-tap then hold = repeat the tap key (hold 'a' instead of Gui)
- **Configurable timing**: `tapping_term_ms` controls tap vs hold threshold
- **Works on ANY key**: Not limited to home row positions

**How it works:**
1. Press HR key → marked as "pending"
2. If another key pressed → resolve to modifier (permissive hold)
3. If released quickly → emit tap key
4. If held past threshold → emit modifier
5. Double-tap quickly → hold the tap key instead

**Use cases:**
- Reduce finger travel to modifiers
- Custom layouts (Colemak, Dvorak, Workman)
- Ergonomic keyboard layouts
- One-handed typing setups

---

#### 3. **OVERLOAD (Simple Tap/Hold)**

**Like HR mods but simpler** - no permissive hold, just timing-based. Works on **ANY key**.

**Syntax:** `OVERLOAD(tap_key, hold_key)`

**Example:**
```ron
remaps: {
    KC_SPC: OVERLOAD(KC_SPC, KC_LCTL),  // Space or Ctrl
    KC_ENT: OVERLOAD(KC_ENT, KC_LSFT),  // Enter or Shift
    KC_TAB: OVERLOAD(KC_TAB, KC_LGUI),  // Tab or Super
}
```

**Difference from HR:**
- **No permissive hold**: Other keys don't trigger modifier early
- **Pure timing**: Only `tapping_term_ms` determines behavior
- **Simpler logic**: Easier to predict

**How it works:**
1. Press OVERLOAD key → wait for release or timeout
2. If released before `tapping_term_ms` → tap key
3. If held past `tapping_term_ms` → modifier key
4. Double-tap → hold the tap key

**Use cases:**
- Space/Shift for aggressive typists
- Modifier keys you want explicit control over
- Keys where permissive hold is too aggressive

---

#### 4. **SOCD (Simultaneous Opposite Cardinal Directions)**

**Generic SOCD for gaming** - no longer hardcoded to WASD!

**Syntax:** `Socd { this_key: KC_X, opposing_key: KC_Y }`

**Strategy:** Last Input Priority (LIP) - pressing W then S = S wins, release S = W reactivates

**Example:**
```ron
game_mode: (
    remaps: {
        // WASD SOCD (default)
        KC_W: Socd { this_key: KC_W, opposing_key: KC_S },
        KC_S: Socd { this_key: KC_S, opposing_key: KC_W },
        KC_A: Socd { this_key: KC_A, opposing_key: KC_D },
        KC_D: Socd { this_key: KC_D, opposing_key: KC_A },
        
        // Arrow keys SOCD (for racing games!)
        KC_UP: Socd { this_key: KC_UP, opposing_key: KC_DOWN },
        KC_DOWN: Socd { this_key: KC_DOWN, opposing_key: KC_UP },
        KC_LEFT: Socd { this_key: KC_LEFT, opposing_key: KC_RGHT },
        KC_RGHT: Socd { this_key: KC_RGHT, opposing_key: KC_LEFT },
        
        // Custom game bindings
        KC_I: Socd { this_key: KC_I, opposing_key: KC_K },  // Forward/Back
        KC_K: Socd { this_key: KC_K, opposing_key: KC_I },
    },
)
```

**How it works:**
1. Pressing W → W active
2. While holding W, press S → S takes over (W released, S pressed)
3. Release S → W reactivates automatically
4. Works independently for vertical (W/S) and horizontal (A/D) axes

**Use cases:**
- FPS games (WASD movement)
- Racing games (arrow keys)
- Fighting games (hitbox controllers)
- Any game with opposing direction inputs
- Custom control schemes

---

#### 5. **Command Runner**

**Execute arbitrary shell commands on key press**

**Syntax:** `CMD("command")`

**Example:**
```ron
remaps: {
    KC_F1: CMD("/usr/bin/notify-send 'Hello'"),
    KC_F2: CMD("/usr/bin/playerctl play-pause"),
    KC_F3: CMD("/usr/bin/brightnessctl set 10%+"),
}
```

**Features:**
- **Pure customizability**: Run any shell command
- **Background execution**: Commands run in background thread
- **System integration**: Control media, notifications, brightness, etc.

**Use cases:**
- Media controls (play/pause, volume, skip)
- System commands (screenshot, lock, sleep)
- Custom scripts and automation
- Notification triggers

---

#### 6. **Layer Switching**

**Switch to any named layer while key is held**

**Syntax:** `TO("layer_name")`

**Example:**
```ron
remaps: {
    KC_CAPS: TO("nav"),      // Hold Caps Lock → Nav layer
    KC_TAB: TO("symbols"),   // Hold Tab → Symbol layer  
    KC_LSFT: TO("num"),      // Hold Shift → Number layer
}
```

**How it works:**
1. Press layer switch key → switch to that layer
2. All key lookups now check: Game Mode → Active Layer → Base
3. Release layer switch key → return to base layer
4. Layers can be nested (layer key in another layer)

---

#### 7. **Simple Key Remapping**

**Remap any key to any other key**

**Syntax:** `Key(KC_OUTPUT)`

**Example:**
```ron
remaps: {
    KC_CAPS: Key(KC_ESC),        // Caps Lock → Escape
    KC_ESC: Key(KC_GRV),         // Escape → Backtick
    KC_RGHT: Key(KC_BSPC),       // Right Arrow → Backspace
}
```

---

#### 8. **Game Mode (Automatic Detection)**

**Special layer activated during gaming** - auto-detected or manual toggle

**Auto-detection methods:**
1. **Gamescope App ID**: Detects Steam games in Gamescope
2. **Steam App Prefix**: Detects processes starting with "steam_app_"
3. **IS_GAME env var**: Detects games setting IS_GAME=1
4. **Process tree walk**: Walks 10 levels up to find gaming processes

**Manual toggle:**
```bash
keyboard-middleware gamemode on   # Enable
keyboard-middleware gamemode off  # Disable  
```

**Configuration:**
```ron
game_mode: (
    remaps: {
        // Disable home row mods in games
        KC_A: Key(KC_A),
        KC_S: Key(KC_S),
        KC_D: Key(KC_D),
        KC_F: Key(KC_F),
        
        // Add SOCD
        KC_W: Socd { this_key: KC_W, opposing_key: KC_S },
        KC_S: Socd { this_key: KC_S, opposing_key: KC_W },
        KC_A: Socd { this_key: KC_A, opposing_key: KC_D },
        KC_D: Socd { this_key: KC_D, opposing_key: KC_A },
        
        // Keep essential remaps
        KC_CAPS: Key(KC_ESC),
    },
)
```

**How it works:**
1. Daemon monitors window focus (Niri compositor)
2. Checks if focused window is a game
3. Automatically enables/disables game_mode layer
4. Game mode has highest priority in lookup

**Use cases:**
- Disable home row mods (prevent accidental modifier activation)
- Enable SOCD for movement keys
- Gaming-specific bindings
- Different sensitivity for gaming vs typing

---

### Advanced Features

#### 9. **Per-Keyboard Overrides**

**Different configs for different keyboards**

**Example:**
```ron
keyboard_overrides: {
    "1234:5678:0100:0003:usb-...": (
        settings: (
            tapping_term_ms: 150,  // Faster for this keyboard
        ),
        keymap: (
            base_remaps: {
                KC_CAPS: Key(KC_LCTL),  // Different for this one
            },
            layers: {
                "nav": ( remaps: { /* custom nav */ } ),
            },
            game_mode_remaps: {
                KC_W: Key(KC_UP),  // Different game binds
            },
        ),
    ),
}
```

**Use cases:**
- Laptop keyboard vs external keyboard
- Gaming keyboard vs typing keyboard
- Ergonomic split keyboard with different layout
- Testing new configs without affecting main keyboard

---

#### 10. **Timing Customization**

**Fine-tune tap/hold behavior per keyboard**

**Settings:**
```ron
// Global settings
tapping_term_ms: 175,              // How long to wait for hold
double_tap_window_ms: Some(300),   // Window for double-tap detection

// Per-keyboard override
keyboard_overrides: {
    "keyboard-id": (
        settings: (
            tapping_term_ms: 150,            // Faster
            double_tap_window_ms: Some(250), // Quicker
        ),
    ),
}
```

**Recommendations:**
- **130-150ms**: Aggressive (fast typers, gaming keyboards)
- **175-200ms**: Balanced (most users)
- **200-250ms**: Conservative (slow/deliberate typers)

---

### Supported Keys

**All standard keyboard keys supported:**

- **Letters**: A-Z
- **Numbers**: 0-9, Numpad 0-9
- **Modifiers**: Ctrl, Shift, Alt, Super/Win/Cmd (left & right)
- **Function**: F1-F24
- **Navigation**: Arrows, Home, End, Page Up/Down, Insert, Delete
- **Editing**: Backspace, Enter, Tab, Space, Escape
- **Punctuation**: All standard symbols
- **Numpad**: All numpad keys + Num Lock
- **Locking**: Caps Lock, Scroll Lock
- **Media**: Play/Pause, Stop, Next, Previous, Volume, Mute
- **System**: Power, Sleep, Wake, Calculator, My Computer
- **Web**: Browser controls, Search, Home, Back, Forward, Refresh
- **Application**: Menu, App key

**Total: 150+ keys supported**

---

## Design Goals

---

## Technical Architecture

### Architecture Components

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Root Daemon   │◄──►│  Session Manager │◄──►│  User Sessions  │
│  (Async Event)  │    │   (Ownership)    │    │   (Per User)    │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ Device Pool     │    │  Config Manager  │    │   IPC Server   │
│ (Hotplug Mgr)   │    │  (Hot Reload)    │    │  (Multi-user)  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ Thread Pool     │    │  Game Mode Mgr   │    │  Niri Monitor   │
│ (Per Device)    │    │ (Smart Detection)│    │ (Auto-restart) │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

### Data Flow: Key Press Journey

```
1. Physical Keyboard
         ↓
2. evdev Device (/dev/input/eventX)
         ↓  
3. Event Processor Thread (GRABBED - exclusive access)
         ↓
4. Keymap Processor (zero-latency processing)
         ↓
   ┌─────────────────────────────┐
   │  Lookup Priority:           │
   │  1. Game Mode (if active)   │
   │  2. Current Layer           │
   │  3. Base Layer              │
   └─────────────────────────────┘
         ↓
5. Action Processing
   ├─ Key(x) → Emit key x
   ├─ HR(tap, hold) → Permissive hold logic
   ├─ OVERLOAD(tap, hold) → Timing logic
   ├─ TO(layer) → Switch layer
   ├─ SOCD() → SOCD resolution
   └─ CMD(command) → Execute shell command
         ↓
6. Virtual uinput Device
         ↓
7. Desktop Environment / Applications
```

### Thread Model

#### One Thread Per Event File

Each keyboard can expose multiple event files (e.g., `/dev/input/event3` for keys, `/dev/input/event4` for media keys). We spawn **one dedicated thread per event file** for maximum parallelism.

**Example:**
```
Keyboard "Keychron K2"
  ├─ Thread 1: /dev/input/event3 (main keys)
  └─ Thread 2: /dev/input/event4 (media keys)

Keyboard "Built-in Laptop"
  └─ Thread 3: /dev/input/event5 (all keys)
```

**Why:**
- Some keyboards split functionality across multiple event files
- Parallel processing for complex keyboards
- Prevents one device from blocking another

#### Thread Lifecycle

1. **Spawn**: Daemon creates thread when keyboard assigned to user
2. **Run**: Thread processes keys in tight loop with `yield_now()`
3. **Commands**: Non-blocking checks for game mode / shutdown via channels
4. **Shutdown**: Graceful cleanup on user logout or device disconnect
5. **No restart on periodic checks**: Threads only restart on actual hardware changes

#### State Preservation

Game mode state is preserved across thread restarts:
1. Stored in AsyncDaemon struct
2. Sent to new threads on creation
3. Prevents game mode resets during session changes

---

### Keymap Processing Logic

#### Home Row Mods (Detailed)

**State Machine:**
```
┌──────────────┐
│   Key Press  │
└──────┬───────┘
       │
       ▼
┌──────────────────────┐
│  Is Double-Tap?      │ ◄── Check last tap time
├──────┬───────────────┤
│ YES  │ NO            │
▼      ▼               │
Hold   Pending         │
Base   State   ◄───────┘
Key                    
       │               
       ├─ Other Key Pressed? → Resolve to HOLD (permissive)
       │
       ├─ Timeout? → Resolve to HOLD
       │  
       └─ Released? → TAP
```

**Data Structures:**
```rust
struct KeymapProcessor {
    held_keys: HashMap<KeyCode, Vec<KeyAction>>,
    hrm_last_tap: HashMap<KeyCode, Instant>,  // Generic, not fixed array
    pending_hrm: HashSet<KeyCode>,             // Generic, any key can be HR mod
    double_tap_window_ms: u32,
    // ...
}
```

**No hardcoded key restrictions** - works on any KeyCode!

---

#### SOCD Resolution (Detailed)

**Generic State Machine:**
```rust
struct SocdState {
    // Track all SOCD pairs dynamically
    held_keys: HashMap<KeyCode, bool>,           // Which keys held
    opposing_pairs: HashMap<KeyCode, KeyCode>,   // Key → its opposite
    last_input: HashMap<KeyCode, KeyCode>,       // For each pair, which was last
    active_keys: HashMap<KeyCode, Option<KeyCode>>, // Currently emitted key per pair
}
```

**Example for W/S pair:**
```
State: W=false, S=false, Active=None

1. Press W
   → W=true, Active=W, Emit: W press

2. Press S (while holding W)  
   → S=true, Active=S, last=S
   → Emit: W release, S press

3. Release S (still holding W)
   → S=false, Active=W, last=W  
   → Emit: S release, W press

4. Release W
   → W=false, Active=None
   → Emit: W release
```

**Supports:**
- Any opposing key pairs (not just WASD)
- Multiple independent pairs (W/S, A/D, Up/Down, Left/Right, custom)
- Last Input Priority (LIP) strategy
- Clean state management per pair

---

### Configuration System

#### RON Format

We use **RON (Rusty Object Notation)** - Rust's equivalent to JSON but more ergonomic:

**Why RON:**
- Native Rust data structure syntax
- Comments supported (`//`)
- Trailing commas allowed
- More readable than JSON for configs
- Strong typing with serde

**Example:**
```ron
(
    tapping_term_ms: 175,
    remaps: {
        KC_A: HR(KC_A, KC_LGUI),
        KC_CAPS: TO("nav"),
    },
    layers: {
        "nav": (
            remaps: {
                KC_H: Key(KC_LEFT),
                KC_J: Key(KC_DOWN),
            },
        ),
    },
)
```

#### Per-Keyboard Overrides

**Keyboard ID format:**
```
{vendor}:{product}:{version}:{interface}:usb-{bus}-{port}/input{N}
Example: 1234:5678:0100:0003:usb-0000:00:14.0-1/input0
```

**Override resolution:**
1. Load base config
2. If keyboard ID matches override, apply overrides:
   - Settings override global settings
   - Keymap overrides replace (not merge) base keymap
3. Result is keyboard-specific config

#### Config Validation

**Validation checks:**
- All referenced layers exist
- Key codes are valid
- SOCD pairs are symmetric (if W→S, then S→W)
- No circular layer references
- Timing values are reasonable (> 0ms, < 1000ms)

---

---

## Implementation Roadmap

### Current State vs Target State

#### ✅ Already Implemented (Keep As-Is)

1. **Multi-user daemon architecture** - Root daemon with per-user session management
2. **Thread-per-event-file** - Parallel processing for complex keyboards
3. **Game mode state preservation** - Survives thread restarts
4. **Hybrid sync/async** - Sync hot path, async management
5. **udev hotplug monitoring** - No periodic polling, pure event-driven
6. **IPC server** - Multi-user command interface
7. **Niri window monitor** - Automatic game mode detection
8. **Per-keyboard overrides** - Different configs per device
9. **Home row mods with permissive hold** - Advanced tap/hold logic
10. **OVERLOAD action** - Simple tap/hold without permissive hold
11. **Basic key remapping** - Direct key-to-key mapping
12. **Layer switching (TO action)** - Hold-to-activate layers
13. **Config hot-reload** - Via IPC commands
14. **Graceful shutdown** - Proper device ungrab and cleanup
15. **Virtual device creation** - uinput integration

#### ❌ Needs Implementation/Fixing

1. **Generic layers** - Currently hardcoded to 5 enum variants, need String-based
2. **Generic SOCD** - Currently hardcoded to WASD only, need config-driven
3. **Generic HR mods** - Currently limited to ASDF JKL; keys, need any-key support
4. **Expanded KeyCode enum** - Only ~70 keys, need 150+ (media, numpad, etc.)
5. **SOCD state management** - Hardcoded booleans, need dynamic HashMap-based
7. **HR mod double-tap tracking** - Fixed-size array, need HashMap
8. **evdev ↔ KeyCode conversions** - Only covers basic keys, need all keys

---

### Phase 1: Core Type System Refactor

**Goal:** Make config types fully generic and extensible

#### 1.1 Layer System (`config.rs`)

**Current:**
```rust
pub enum Layer {
    L_BASE,
    L_NAV,
    L_NUM,
    L_SYM,
    L_FN,
}
```

**Target:**
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Layer(pub String);

impl Layer {
    pub fn base() -> Self {
        Layer("base".to_string())
    }
    
    pub fn is_base(&self) -> bool {
        self.0 == "base"
    }
    
    pub fn new(name: impl Into<String>) -> Self {
        Layer(name.into())
    }
}
```

**Files to modify:**
- `src/config.rs` - Layer definition (DONE)
- `src/keymap.rs` - Replace all Layer::L_BASE with Layer::base()
- `src/config.rs` - Update Config struct field types
- `config.example.ron` - Update example to use string-based layers

**Config format change:**
```ron
// OLD
layers: {
    L_NAV: ( remaps: { ... } ),
}

// NEW
layers: {
    "nav": ( remaps: { ... } ),
    "symbols": ( remaps: { ... } ),
    "my_custom_layer": ( remaps: { ... } ),
}
```

#### 1.2 Action Types (`config.rs`)

**Current:**
```rust
pub enum Action {
    Key(KeyCode),
    HR(KeyCode, KeyCode),
    OVERLOAD(KeyCode, KeyCode),
    TO(Layer),
    Socd(KeyCode, KeyCode),  // ❌ Only works with WASD
}
```

**Target:**
```rust
pub enum Action {
    Key(KeyCode),
    HR(KeyCode, KeyCode),
    OVERLOAD(KeyCode, KeyCode),
    TO(Layer),
    SOCD(KeyCode, Vec<KeyCode>),  // Stack-based last-input-priority
    CMD(String),  // Execute shell command
}
```

**Files to modify:**
- `src/config.rs` - Action enum definition (DONE)
- `src/keymap.rs` - Update all pattern matches for SOCD and CMD
- `config.example.ron` - Update examples

**Config format change:**
```ron
// OLD
KC_W: Socd(KC_W, KC_S),

// NEW  
KC_W: SOCD(KC_W, [KC_S]),
KC_F1: CMD("/usr/bin/notify-send 'Hello'"),
```

---

### Phase 2: KeyCode Expansion

**Goal:** Support all possible keyboard keys (150+ total)

#### 2.1 Expand KeyCode Enum (`config.rs`)

**Add these categories:**

```rust
pub enum KeyCode {
    // ===== EXISTING (Keep) =====
    // Letters: KC_A to KC_Z
    // Numbers: KC_1 to KC_0
    // Modifiers: KC_LCTL, KC_LSFT, KC_LALT, KC_LGUI, KC_RCTL, KC_RSFT, KC_RALT, KC_RGUI
    // Special: KC_ESC, KC_CAPS, KC_TAB, KC_SPC, KC_ENT, KC_BSPC, KC_DEL, etc.
    // Arrows: KC_LEFT, KC_DOWN, KC_UP, KC_RGHT
    // F-keys: KC_F1 to KC_F12
    
    // ===== NEW ADDITIONS =====
    
    // Navigation (6 keys)
    KC_PGUP,
    KC_PGDN,
    KC_HOME,
    KC_END,
    KC_INS,
    KC_PSCR,  // Print Screen
    
    // Numpad (17 keys)
    KC_KP_0,
    KC_KP_1,
    KC_KP_2,
    KC_KP_3,
    KC_KP_4,
    KC_KP_5,
    KC_KP_6,
    KC_KP_7,
    KC_KP_8,
    KC_KP_9,
    KC_KP_SLASH,
    KC_KP_ASTERISK,
    KC_KP_MINUS,
    KC_KP_PLUS,
    KC_KP_ENTER,
    KC_KP_DOT,
    KC_NUM_LOCK,
    
    // Media Keys (8 keys)
    KC_MUTE,
    KC_VOL_UP,
    KC_VOL_DN,
    KC_MEDIA_PLAY_PAUSE,
    KC_MEDIA_STOP,
    KC_MEDIA_NEXT_TRACK,
    KC_MEDIA_PREV_TRACK,
    KC_MEDIA_SELECT,
    
    // System Keys (12 keys)
    KC_PWR,
    KC_SLEP,
    KC_WAKE,
    KC_CALC,
    KC_MY_COMP,
    KC_WWW_SEARCH,
    KC_WWW_HOME,
    KC_WWW_BACK,
    KC_WWW_FORWARD,
    KC_WWW_STOP,
    KC_WWW_REFRESH,
    KC_WWW_FAVORITES,
    
    // Locking Keys (2 keys)
    KC_SCRL,  // Scroll Lock
    KC_PAUS,  // Pause/Break
    
    // Extended F-keys (12 keys)
    KC_F13,
    KC_F14,
    KC_F15,
    KC_F16,
    KC_F17,
    KC_F18,
    KC_F19,
    KC_F20,
    KC_F21,
    KC_F22,
    KC_F23,
    KC_F24,
    
    // Application Keys (2 keys)
    KC_APP,
    KC_MENU,
    
    // Multimedia (7 keys)
    KC_BRIU,  // Brightness Up
    KC_BRID,  // Brightness Down
    KC_DISPLAY_OFF,
    KC_WLAN,
    KC_TOOLS,
    KC_BLUETOOTH,
    KC_KEYBOARD_LAYOUT,
    
    // International Keys (3 keys)
    KC_INTL_BACKSLASH,  // 102nd key on European keyboards
    KC_INTL_YEN,
    KC_INTL_RO,
}
```

**Total new keys:** ~70
**Total keys after expansion:** ~150

**Files to modify:**
- `src/config.rs` - Add all new KeyCode variants

#### 2.2 Expand evdev Conversions (`keymap.rs`)

**Add conversion functions for all new keys:**

```rust
pub const fn evdev_to_keycode(key: Key) -> Option<KeyCode> {
    match key {
        // ... existing conversions ...
        
        // NEW: Navigation
        Key::KEY_PAGEUP => Some(KeyCode::KC_PGUP),
        Key::KEY_PAGEDOWN => Some(KeyCode::KC_PGDN),
        Key::KEY_HOME => Some(KeyCode::KC_HOME),
        Key::KEY_END => Some(KeyCode::KC_END),
        Key::KEY_INSERT => Some(KeyCode::KC_INS),
        Key::KEY_SYSRQ => Some(KeyCode::KC_PSCR),
        
        // NEW: Numpad
        Key::KEY_KP0 => Some(KeyCode::KC_KP_0),
        Key::KEY_KP1 => Some(KeyCode::KC_KP_1),
        // ... all numpad keys ...
        Key::KEY_NUMLOCK => Some(KeyCode::KC_NUM_LOCK),
        
        // NEW: Media
        Key::KEY_MUTE => Some(KeyCode::KC_MUTE),
        Key::KEY_VOLUMEUP => Some(KeyCode::KC_VOL_UP),
        Key::KEY_VOLUMEDOWN => Some(KeyCode::KC_VOL_DN),
        Key::KEY_PLAYPAUSE => Some(KeyCode::KC_MEDIA_PLAY_PAUSE),
        // ... all media keys ...
        
        // NEW: System
        Key::KEY_POWER => Some(KeyCode::KC_PWR),
        Key::KEY_SLEEP => Some(KeyCode::KC_SLEP),
        // ... all system keys ...
        
        // NEW: F13-F24
        Key::KEY_F13 => Some(KeyCode::KC_F13),
        // ... F14-F24 ...
        
        _ => None,
    }
}

pub const fn keycode_to_evdev(keycode: KeyCode) -> Key {
    match keycode {
        // ... existing conversions ...
        
        // NEW: All new keys
        KeyCode::KC_PGUP => Key::KEY_PAGEUP,
        KeyCode::KC_PGDN => Key::KEY_PAGEDOWN,
        // ... all new keys ...
    }
}
```

**Files to modify:**
- `src/keymap.rs` - Add ~140 new match arms (70 keys × 2 directions)

---

### Phase 3: SOCD Generalization

**Goal:** Support arbitrary opposing key pairs, not just WASD

#### 3.1 Dynamic SOCD State (`keymap.rs`)

**Current (Hardcoded WASD):**
```rust
struct KeymapProcessor {
    socd_w_held: bool,
    socd_s_held: bool,
    socd_a_held: bool,
    socd_d_held: bool,
    socd_last_vertical: Option<KeyCode>,
    socd_last_horizontal: Option<KeyCode>,
    socd_active_keys: [Option<KeyCode>; 2],
}

const fn socd_handle_press(&mut self, keycode: KeyCode) -> [Option<KeyCode>; 2] {
    match keycode {
        KeyCode::KC_W => { self.socd_w_held = true; ... }
        KeyCode::KC_S => { self.socd_s_held = true; ... }
        KeyCode::KC_A => { self.socd_a_held = true; ... }
        KeyCode::KC_D => { self.socd_d_held = true; ... }
        _ => {}
    }
}
```

**Target (Generic):**
```rust
struct SocdPair {
    key1: KeyCode,
    key2: KeyCode,
    key1_held: bool,
    key2_held: bool,
    last_input: KeyCode,
    active_key: Option<KeyCode>,
}

struct KeymapProcessor {
    // Build from config at initialization
    socd_pairs: HashMap<KeyCode, SocdPair>,  // Key -> its pair info
    // Remove: socd_w_held, socd_s_held, socd_a_held, socd_d_held, etc.
}

impl KeymapProcessor {
    pub fn new(config: &Config) -> Self {
        let mut socd_pairs = HashMap::new();
        
        // Extract SOCD pairs from config
        for (keycode, action) in &config.remaps {
            if let Action::Socd { this_key, opposing_key } = action {
                // Create bidirectional mapping
                socd_pairs.insert(*keycode, SocdPair {
                    key1: *this_key,
                    key2: *opposing_key,
                    key1_held: false,
                    key2_held: false,
                    last_input: *this_key,
                    active_key: None,
                });
            }
        }
        
        // Also check game_mode and layers for SOCD
        // ...
        
        Self {
            socd_pairs,
            // ...
        }
    }
    
    fn socd_handle_press(&mut self, keycode: KeyCode) -> ProcessResult {
        if let Some(pair) = self.socd_pairs.get_mut(&keycode) {
            // Generic SOCD logic
            let old_active = pair.active_key;
            
            if keycode == pair.key1 {
                pair.key1_held = true;
                pair.last_input = pair.key1;
            } else if keycode == pair.key2 {
                pair.key2_held = true;
                pair.last_input = pair.key2;
            }
            
            // Compute new active key
            let new_active = if pair.key1_held && !pair.key2_held {
                Some(pair.key1)
            } else if pair.key2_held && !pair.key1_held {
                Some(pair.key2)
            } else if pair.key1_held && pair.key2_held {
                Some(pair.last_input)  // Last input priority
            } else {
                None
            };
            
            pair.active_key = new_active;
            
            // Generate transition events
            self.generate_socd_transition(old_active, new_active)
        } else {
            ProcessResult::None
        }
    }
}
```

**Files to modify:**
- `src/keymap.rs` - Complete SOCD logic rewrite (~200 lines)
- Remove: All WASD-specific fields and functions
- Add: Generic SocdPair struct and HashMap-based tracking

#### 3.2 Config Validation

**Add validation to ensure SOCD pairs are symmetric:**

```rust
impl Config {
    pub fn validate(&self) -> Result<()> {
        // Validate SOCD pairs
        let mut socd_map: HashMap<KeyCode, KeyCode> = HashMap::new();
        
        for (key, action) in &self.remaps {
            if let Action::Socd { this_key, opposing_key } = action {
                if *key != *this_key {
                    return Err(anyhow::anyhow!(
                        "SOCD key mismatch: {:?} maps to Socd{{this_key: {:?}, ...}}", 
                        key, this_key
                    ));
                }
                socd_map.insert(*this_key, *opposing_key);
            }
        }
        
        // Check symmetry
        for (key1, key2) in &socd_map {
            if let Some(reverse) = socd_map.get(key2) {
                if reverse != key1 {
                    return Err(anyhow::anyhow!(
                        "SOCD pair asymmetric: {:?} -> {:?}, but {:?} -> {:?}",
                        key1, key2, key2, reverse
                    ));
                }
            } else {
                return Err(anyhow::anyhow!(
                    "SOCD missing reverse pair: {:?} -> {:?}, but {:?} not defined",
                    key1, key2, key2
                ));
            }
        }
        
        Ok(())
    }
}
```

**Files to modify:**
- `src/config.rs` - Add validate() method

---

### Phase 4: HR Mod Generalization

**Goal:** Remove key restrictions, support HR mods on ANY key

#### 4.1 Remove Bit-Flag Optimization (`keymap.rs`)

**Current (Limited to 8 keys):**
```rust
struct KeymapProcessor {
    pending_hrm: u8,  // Bit flags for ASDF JKL;
    hrm_last_tap: [Option<Instant>; 8],  // Fixed array
}

const fn keycode_to_hrm_bit(keycode: KeyCode) -> Option<u8> {
    match keycode {
        KeyCode::KC_A => Some(0),
        KeyCode::KC_S => Some(1),
        KeyCode::KC_D => Some(2),
        KeyCode::KC_F => Some(3),
        KeyCode::KC_J => Some(4),
        KeyCode::KC_K => Some(5),
        KeyCode::KC_L => Some(6),
        KeyCode::KC_SCLN => Some(7),
        _ => None,  // ❌ Other keys can't be HR mods
    }
}
```

**Target (Generic):**
```rust
struct KeymapProcessor {
    pending_hrm: HashSet<KeyCode>,  // Any key can be pending
    hrm_last_tap: HashMap<KeyCode, Instant>,  // Any key can be double-tapped
}

impl KeymapProcessor {
    fn has_pending_hrm(&self) -> bool {
        !self.pending_hrm.is_empty()
    }
    
    fn set_hrm_pending(&mut self, keycode: KeyCode) {
        self.pending_hrm.insert(keycode);
    }
    
    fn clear_hrm_pending(&mut self, keycode: KeyCode) {
        self.pending_hrm.remove(&keycode);
    }
    
    fn is_double_tap(&self, keycode: KeyCode) -> bool {
        if let Some(last_tap) = self.hrm_last_tap.get(&keycode) {
            let elapsed = Instant::now().duration_since(*last_tap).as_millis() as u32;
            return elapsed < self.double_tap_window_ms;
        }
        false
    }
    
    fn set_hrm_last_tap(&mut self, keycode: KeyCode) {
        self.hrm_last_tap.insert(keycode, Instant::now());
    }
}
```

**Files to modify:**
- `src/keymap.rs` - Remove all bit-flag functions
- `src/keymap.rs` - Replace with HashMap-based tracking (~50 lines changed)

#### 4.2 Same for OVERLOAD

**Current:**
```rust
struct KeymapProcessor {
    overload_press_times: HashMap<KeyCode, Instant>,  // ✅ Already generic!
    pending_overload: HashSet<KeyCode>,                // ✅ Already generic!
}
```

**No changes needed** - OVERLOAD is already generic! ✅

---

---

### Phase 6: Directory Restructure

**Goal:** Better code organization in `event_processor/` subdirectory

#### 6.1 New Directory Structure

```
src/
├── event_processor/
│   ├── mod.rs              # Re-exports + main entry point
│   ├── processor.rs        # Main event loop (current event_processor.rs)
│   ├── keymap.rs           # Keymap processing (current keymap.rs)
│   ├── actions.rs          # Action processing logic
│   ├── homerow_mods.rs     # HR mod logic (extracted from keymap.rs)
│   ├── overload.rs         # OVERLOAD logic (extracted from keymap.rs)
│   ├── socd.rs             # SOCD logic (extracted from keymap.rs)
│   ├── command.rs          # Command execution logic
│   ├── conversions.rs      # evdev ↔ KeyCode conversions (800+ lines)
│   └── virtual_device.rs   # Virtual device creation and typing
├── config.rs               # Config types
├── config_manager.rs       # Config loading/hot-reload
├── daemon.rs               # Main daemon
├── keyboard_id.rs          # Keyboard identification
├── niri.rs                 # Niri integration
├── session_manager.rs      # Multi-user sessions
├── ipc.rs                  # IPC server
└── main.rs                 # CLI entry point
```

#### 6.2 Module Breakdown

**`event_processor/mod.rs`:**
```rust
mod processor;
mod keymap;
mod actions;
mod homerow_mods;
mod overload;
mod socd;
mod command;
mod conversions;
mod virtual_device;

pub use processor::start_event_processor;
pub use keymap::{KeymapProcessor, ProcessResult};
pub use conversions::{evdev_to_keycode, keycode_to_evdev};
```

**`event_processor/processor.rs`:**
- Current `event_processor.rs` content
- Main event loop
- Command handling (game mode, shutdown)

**`event_processor/keymap.rs`:**
- KeymapProcessor struct
- Core key processing logic
- Action lookup (game mode → layer → base)
- Delegates to specialized modules

**`event_processor/actions.rs`:**
- Simple Key() action processing
- TO() layer switching

**`event_processor/homerow_mods.rs`:**
- All HR mod logic
- Double-tap detection
- Permissive hold resolution
- Extracted from keymap.rs (~150 lines)

**`event_processor/overload.rs`:**
- All OVERLOAD logic
- Timing-based tap/hold
- Extracted from keymap.rs (~80 lines)

**`event_processor/socd.rs`:**
- SocdPair struct
- Generic SOCD resolution
- LIP algorithm
- Extracted from keymap.rs (~150 lines)

**`event_processor/command.rs`:**
```rust
pub struct CommandRunner;

impl CommandRunner {
    pub fn execute(command: &str) {
        std::thread::spawn(move || {
            let _ = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .spawn();
        });
    }
} 
        id: &str
    ) -> ProcessResult { ... }
}
```

**`event_processor/conversions.rs`:**
- Move all 800+ lines of evdev ↔ KeyCode conversion functions
- `evdev_to_keycode()`
- `keycode_to_evdev()`

**`event_processor/virtual_device.rs`:**
- Virtual device creation
- `type_string()` function
- `char_to_key()` function
- `release_all_keys()` function

**Files to create:**
- 9 new files in `event_processor/` directory

**Files to modify:**
- `src/event_processor.rs` → Move to `event_processor/processor.rs`
- `src/keymap.rs` → Split into multiple files in `event_processor/`

---

### Phase 7: Testing & Validation

#### 7.1 Unit Tests

**Add tests for:**
- Layer lookup resolution
- SOCD pair resolution for various inputs
- HR mod permissive hold
- Command execution
- KeyCode conversions (all 150 keys)

**Test files to create:**
```
tests/
├── layer_tests.rs
├── socd_tests.rs
├── homerow_mod_tests.rs
├── command_tests.rs
└── conversion_tests.rs
```

#### 7.2 Integration Tests

**Test scenarios:**
1. Load config with custom layers
2. Press keys in various layers
3. Test SOCD with arrow keys and multiple opposing keys
4. Test HR mods on non-home-row keys
5. Test command execution
6. Test all media/numpad keys

#### 7.3 Migration Guide

**Breaking changes for users:**
1. Layer names change from `L_NAV` to `"nav"`
2. SOCD syntax changes to stack-based: `SOCD(key, [opposing_keys...])`
3. Password action replaced with `CMD()` for arbitrary commands

**Create migration script:**
```bash
#!/bin/bash
# migrate_config.sh - Automatically migrate old configs to new format

sed -i 's/L_NAV/"nav"/g' config.ron
sed -i 's/L_NUM/"num"/g' config.ron
# ... etc
```

---

## Implementation Order

### Priority 1 (Core Functionality) - Do First

1. ✅ **Layer system refactor** - Most impactful for customization
2. ✅ **Action type updates** - Required for SOCD and CMD
3. ✅ **Command runner** - Execute arbitrary shell commands

### Priority 2 (Expand Capabilities) - Do Second

5. **KeyCode expansion** - Add 70 new keys
6. **evdev conversion expansion** - Support all new keys
7. **SOCD generalization** - Generic opposing pairs
8. **HR mod generalization** - Remove key restrictions

### Priority 3 (Polish) - Do Last

9. **Directory restructure** - Better organization
10. **Testing suite** - Comprehensive tests
11. **Migration guide** - Help users upgrade
12. **Update config.example.ron** - Showcase all features

---

## Implementation Details

## Key Design Decisions

### 1. Hybrid Sync/Async Architecture

**Hot Path (Synchronous)** - MAXIMUM SPEED
```
Physical Device → Event Processor (sync) → Virtual Device
         ↓                ↓                    ↓
   evdev::fetch()  Keymap Processing    evdev::emit()
   (non-blocking)   (zero alloc)       (instant)
```

**Cold Path (Asynchronous)** - ROBUST MANAGEMENT
```
Root Daemon (async) ←→ IPC Server ←→ Config Manager
      ↓                    ↓              ↓
Hotplug Events      User Commands    File Watcher
Session Changes     Status Queries   Hot Reload
```

### 2. Session-Based Keyboard Ownership

- **First-come-first-serve**: Keyboards assigned to first requesting active user
- **Automatic release**: Keyboards freed when user session ends
- **Priority system**: Admin/system users can override regular users
- **Multi-session support**: Multiple users can have different keyboards

### 3. Smart Hot-Reload

**Before (Destructive)**:
```
Config Change → Kill ALL Threads → Reload Config → Restart ALL Threads
```

**After (Smart)**:
```
Config Change → Analyze Changes → Restart ONLY Affected Threads
```

### 4. Thread Lifecycle Management

- **Graceful shutdown**: Threads receive signals and clean up properly
- **Device release**: Proper ungrab before thread exit
- **Zombie cleanup**: Automatic detection and cleanup of dead threads
- **Resource tracking**: Monitor thread health and device state

## Component Details

### Session Manager (`session_manager.rs`)

**Purpose**: Manage multi-user keyboard ownership

**Key Features**:
- Monitor active user sessions via `loginctl`
- Track keyboard ownership per user
- Handle session changes (login/logout)
- First-come-first-serve assignment

**API**:
```rust
pub async fn request_keyboard(&self, keyboard_id: KeyboardId, uid: u32) -> Result<bool>
pub async fn release_keyboard(&self, keyboard_id: &KeyboardId) -> Result<()>
pub async fn get_keyboard_owner(&self, keyboard_id: &KeyboardId) -> Option<u32>
pub async fn get_user_keyboards(&self, uid: u32) -> Vec<KeyboardId>
```

### Async Daemon (`async_daemon.rs`)

**Purpose**: Main orchestrator with async management layer

**Key Features**:
- `tokio::select!` for efficient event handling
- Background sync services (hotplug, IPC, niri, config watcher)
- Smart hot-reload without service interruption
- Thread pool management with proper cleanup

**Event Loop**:
```rust
tokio::select! {
    _ = tokio::time::sleep(Duration::from_millis(10)) => {
        // Check hotplug events
    }
    _ = tokio::time::sleep(Duration::from_millis(10)) => {
        // Check IPC commands  
    }
    _ = tokio::time::sleep(Duration::from_millis(10)) => {
        // Check niri events
    }
    _ = tokio::time::sleep(Duration::from_millis(100)) => {
        // Check config changes
    }
    _ = tokio::time::sleep(Duration::from_secs(30)) => {
        // Periodic cleanup
    }
}
```

### Event Processor (`event_processor.rs`)

**Purpose**: High-performance synchronous key processing

**Hot Path Optimizations**:
- **Synchronous only**: No async overhead in key processing
- **Zero allocation**: Reuse buffers and minimize allocations
- **Direct evdev**: Direct device access without abstractions
- **Non-blocking**: Check for commands without blocking

**Command Interface**:
```rust
enum ProcessorCommand {
    ReloadConfig(Config),
    SetGameMode(bool),
    Shutdown,
}
```

### Config Manager

**Purpose**: Smart configuration handling

**Features**:
- **Atomic reload**: Update specific sections without full restart
- **Validation**: Verify config before applying
- **Rollback**: Revert on invalid configurations
- **User-specific**: Per-keyboard overrides

## Performance Characteristics

### Target Metrics

- **Key latency**: < 1ms (hot path priority)
- **Hot-reload**: < 100ms without interruption
- **Device discovery**: < 50ms on hotplug
- **Session tracking**: < 10ms for ownership changes

### Memory Usage

- **Per thread**: Minimal stack + keymap state
- **Shared structures**: Arc<RwLock> for thread-safe access
- **Zero-copy**: Event passing without allocations where possible

## Error Handling Strategy

### Self-Healing Components

1. **Device hotplug**: Automatic rediscovery on connection loss
2. **Thread restart**: Dead threads automatically respawned
3. **Config recovery**: Invalid configs rejected with rollback
4. **Session recovery**: Reclaim keyboards on session restoration

### Graceful Degradation

- **Partial failures**: Single keyboard issues don't affect others
- **Fallback modes**: Continue with reduced functionality
- **User notification**: Clear error reporting and recovery steps

## Security Considerations

### User Isolation

- **Device permissions**: Only owner can control keyboard
- **IPC validation**: Verify user permissions for commands
- **Session boundaries**: Respect user session limits

### Input Validation

- **Config sanitization**: Prevent malicious configurations
- **Event filtering**: Drop malformed input events
- **Resource limits**: Prevent resource exhaustion

## Deployment

### systemd Integration

**Root Daemon Service**:
```ini
[Unit]
Description=Keyboard Middleware Root Daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/keyboard-middleware daemon
Restart=always
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target

# User daemon service
sudo cp target/release/keyboard-middleware /usr/local/bin/
sudo systemctl --user daemon-reload
sudo systemctl --user enable --now keyboard-middleware.service

# Root daemon (optional, for system-wide keyboards)
sudo systemctl daemon-reload
sudo systemctl enable --now keyboard-middleware.service

# Setup complete

**User Daemon Service**:
```ini
[Unit]
Description=Keyboard Middleware User Daemon
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/keyboard-middleware daemon --user %i
Restart=on-failure
User=%i

[Install]
WantedBy=default.target
```

## Migration Path

### Phase 1: Core Architecture
- Implement async daemon skeleton
- Add session manager
- Migrate existing event processor

### Phase 2: Smart Features  
- Smart hot-reload implementation
- Enhanced error recovery
- Performance optimizations

### Phase 3: Advanced Features
- Multi-user session support
- Advanced game mode detection
- Configuration validation

## Future Enhancements

### Performance
- **NUMA awareness**: Bind threads to CPU cores
- **Real-time scheduling**: High priority for key processing
- **Memory mapping**: Zero-copy event handling

### Features
- **Device groups**: Logical keyboard groupings
- **Dynamic layers**: Runtime layer creation
- **Macro recording**: User-defined key sequences

### Integration
- **Wayland native**: Direct compositor integration
- **PulseAudio sync**: Audio-visual feedback coordination
- **Network support**: Remote keyboard management

---

This architecture provides a solid foundation for high-performance, multi-user keyboard middleware with robust error handling and future extensibility.
