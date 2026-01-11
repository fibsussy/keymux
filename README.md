# keyboard-middleware ⌨️

**QMK-inspired keyboard customization for Linux** - A blazing-fast, zero-latency keyboard middleware daemon that brings advanced QMK features to any keyboard

## ✨ Features

### Core Functionality
- **Home Row Mods (HRM)**: Tap for letters, hold for modifiers with configurable tapping term
- **OVERLOAD Actions**: Tap/hold with permissive hold and double-tap support
- **Custom Layers**: Define unlimited layers (navigation, numpad, symbols, etc.)
- **Game Mode**: Automatic detection via Steam/Gamescope with SOCD support
- **SOCD Cleaner**: Last-input-priority for FPS games (eliminates W+S conflicts)
- **Command Runner**: Execute arbitrary shell commands on key press
- **Per-Keyboard Configs**: Different keymaps for different keyboards
- **Hot-Reload**: Automatic config reload on file save with desktop notifications

### Advanced Features
- **Zero Input Lag**: Direct evdev access with non-blocking I/O
- **Multi-Keyboard Support**: Handle multiple keyboards simultaneously
- **Hotplug Detection**: Automatically detect keyboard connect/disconnect
- **Hardware-Based IDs**: Keyboards identified by USB properties, not device paths
- **Shell Completions**: Bash, Zsh, Fish support built-in

### System Integration
- **Systemd Service**: Runs as user service with automatic startup
- **Desktop Notifications**: Config reload success/error notifications
- **IPC Architecture**: Manage keyboards without restarting daemon
- **RON Configuration**: Human-readable config with extensive comments

## 🔧 Installation

### One-Line Install (Arch Linux)

**Precompiled binary (default, fast):**
```bash
curl -fsSL https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/install.sh | bash
```

**Or build from source:**
```bash
curl -fsSL https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/install.sh | bash -s local
```

**Note:** For security, inspect the install script before running it. View it [here](https://github.com/fibsussy/keyboard-middleware/blob/main/install.sh).

### Manual Installation

#### Prerequisites

Add yourself to the `input` group:
```bash
sudo usermod -a -G input $USER
# Log out and log back in for changes to take effect
```

#### From Source

```bash
# Clone and build
git clone https://github.com/fibsussy/keyboard-middleware.git
cd keyboard-middleware
cargo build --release

# Install
sudo cp target/release/keyboard-middleware /usr/bin/
sudo cp keyboard-middleware.service /usr/lib/systemd/system/
sudo cp keyboard-middleware-niri.service /usr/lib/systemd/user/
sudo cp config.example.ron /usr/share/doc/keyboard-middleware/

# Enable and start root daemon
sudo systemctl enable --now keyboard-middleware.service
```

### Post-Installation Setup

1. **Copy the example config:**
```bash
mkdir -p ~/.config/keyboard-middleware
cp /usr/share/doc/keyboard-middleware/config.example.ron ~/.config/keyboard-middleware/config.ron
```

2. **Edit your config:**
```bash
$EDITOR ~/.config/keyboard-middleware/config.ron
```

3. **Select which keyboards to enable:**
```bash
keyboard-middleware toggle
```

4. **(Optional) Enable Niri watcher for automatic game mode:**
```bash
systemctl --user enable --now keyboard-middleware-niri.service
```

### Systemd Services

**Root daemon (required):** Manages keyboard devices
- Path: `/usr/lib/systemd/system/keyboard-middleware.service`
- Enable: `sudo systemctl enable --now keyboard-middleware.service`

**User service (optional):** Watches Niri windows for automatic game mode
- Path: `/usr/lib/systemd/user/keyboard-middleware-niri.service`
- Enable: `systemctl --user enable --now keyboard-middleware-niri.service`

## 📖 Configuration Guide

### Configuration File Location

`~/.config/keyboard-middleware/config.ron`

### Basic Structure

```ron
(
    tapping_term_ms: 130,
    double_tap_window_ms: None,
    enabled_keyboards: None,
    remaps: { /* base layer keymaps */ },
    layers: { /* additional layers */ },
    game_mode: ( remaps: { /* game mode keymaps */ } ),
    keyboard_overrides: { /* per-keyboard configs */ },
)
```

### Available Key Codes

**Letters:** `KC_A` through `KC_Z`
**Numbers:** `KC_1` through `KC_0`
**Modifiers:** `KC_LCTL`, `KC_LSFT`, `KC_LALT`, `KC_LGUI`, `KC_RCTL`, `KC_RSFT`, `KC_RALT`, `KC_RGUI`
**Special:** `KC_ESC`, `KC_CAPS`, `KC_TAB`, `KC_SPC`, `KC_ENT`, `KC_BSPC`, `KC_DEL`
**Function:** `KC_F1` through `KC_F12`
**Arrows:** `KC_LEFT`, `KC_DOWN`, `KC_UP`, `KC_RGHT`

### Available Actions

#### Key(KeyCode)
Direct key mapping.
```ron
KC_CAPS: Key(KC_ESC),  // Caps Lock becomes Escape
```

#### HR(tap_key, hold_key)
Home row mod with permissive hold logic.
```ron
KC_A: HR(KC_A, KC_LGUI),  // A = tap 'a', hold for Super/Win/Cmd
KC_S: HR(KC_S, KC_LALT),   // S = tap 's', hold for Alt
```

#### OVERLOAD(tap_key, hold_key)
Tap/hold with permissive hold and optional double-tap mode.
- **Tap**: Emits tap_key (base key)
- **Hold**: Emits hold_key (modifier)
- **Permissive Hold**: Pressing another key while pending resolves to hold
- **Double-Tap** (if `double_tap_window_ms` set): Press twice to hold tap_key instead of hold_key

```ron
KC_SPC: OVERLOAD(KC_SPC, KC_LCTL),  // Tap for Space, hold for Ctrl

# With double-tap enabled:
# Press Space = tap Space
# Hold Space = hold Ctrl (with permissive hold)
# Double-tap Space = hold Space (base key)
```

#### TO(Layer)
Switch to a different layer while held.
```ron
KC_LALT: TO("nav"),  // Hold Left Alt to activate navigation layer
```

#### Socd(key1, key2)
SOCD cleaner for gaming (last-input-priority).
```ron
KC_W: Socd(KC_W, KC_S),  // Pressing W then S = S, release S = W again
```

#### CMD (Command Runner)
Execute arbitrary shell commands on key press.
```ron
KC_F1: CMD("/usr/bin/notify-send 'Hello from keyboard!'"),
KC_F2: CMD("/usr/bin/playerctl play-pause"),
```

### Example Configurations

#### Minimal Config (Home Row Mods Only)

```ron
(
    tapping_term_ms: 130,
    double_tap_window_ms: None,
    enabled_keyboards: None,

    remaps: {
        // Escape/Caps swap
        KC_CAPS: Key(KC_ESC),
        KC_ESC: Key(KC_GRV),

        // Home row mods - left hand
        KC_A: HR(KC_A, KC_LGUI),
        KC_S: HR(KC_S, KC_LALT),
        KC_D: HR(KC_D, KC_LCTL),
        KC_F: HR(KC_F, KC_LSFT),

        // Home row mods - right hand
        KC_J: HR(KC_J, KC_RSFT),
        KC_K: HR(KC_K, KC_RCTL),
        KC_L: HR(KC_L, KC_RALT),
        KC_SCLN: HR(KC_SCLN, KC_RGUI),
    },

    layers: {},
    game_mode: (remaps: {}),
    keyboard_overrides: {},
)
```

#### Advanced Config (Layers + Game Mode)

```ron
(
    tapping_term_ms: 130,
    double_tap_window_ms: Some(300),
    enabled_keyboards: Some([
        "2e3c:c365:0110:0003",
    ]),

    remaps: {
        KC_CAPS: Key(KC_ESC),
        KC_ESC: Key(KC_GRV),
        KC_LALT: TO("nav"),

        KC_A: HR(KC_A, KC_LGUI),
        KC_S: HR(KC_S, KC_LALT),
        KC_D: HR(KC_D, KC_LCTL),
        KC_F: HR(KC_F, KC_LSFT),
        KC_J: HR(KC_J, KC_RSFT),
        KC_K: HR(KC_K, KC_RCTL),
        KC_L: HR(KC_L, KC_RALT),
        KC_SCLN: HR(KC_SCLN, KC_RGUI),
    },

    layers: {
        L_NAV: (
            remaps: {
                // Keep modifiers accessible
                KC_A: Key(KC_LGUI),
                KC_S: Key(KC_LALT),
                KC_D: Key(KC_LCTL),
                KC_F: Key(KC_LSFT),

                // Vim-style navigation
                KC_H: Key(KC_LEFT),
                KC_J: Key(KC_DOWN),
                KC_K: Key(KC_UP),
                KC_L: Key(KC_RGHT),
            },
        ),
    },

    game_mode: (
        remaps: {
            KC_CAPS: Key(KC_ESC),
            KC_ESC: Key(KC_GRV),

            // SOCD for FPS gaming
            KC_W: Socd(KC_W, KC_S),
            KC_S: Socd(KC_S, KC_W),
            KC_A: Socd(KC_A, KC_D),
            KC_D: Socd(KC_D, KC_A),
        },
    ),

    keyboard_overrides: {},
)
```

### Timing Configuration

- **tapping_term_ms**: Time to hold before tap becomes hold (130-200ms recommended)
- **double_tap_window_ms**: Window for detecting double-taps (None or Some(300))

Lower tapping term = more sensitive to holds, higher = more sensitive to taps.

### Game Mode Detection

Game mode activates automatically when:
1. **Steam games**: Process tree contains `steam` + game executable
2. **Gamescope**: Window manager reports gamescope app ID
3. **IS_GAME env var**: Process has `IS_GAME=1` environment variable

Manual toggle: `keyboard-middleware gamemode [on|off]`

## 🎮 Usage

### Daemon Management

```bash
# Start daemon (automatically started by systemd)
keyboard-middleware daemon

# Check status
systemctl status keyboard-middleware

# View live logs
journalctl -u keyboard-middleware -f

# Restart daemon
systemctl restart keyboard-middleware
```

### Keyboard Management

```bash
# List all detected keyboards
keyboard-middleware list

# Toggle which keyboards are enabled (interactive)
keyboard-middleware toggle

# Validate your config
keyboard-middleware validate

# Reload config (automatic on file save, but manual trigger available)
keyboard-middleware reload

# Toggle game mode manually
keyboard-middleware gamemode on
keyboard-middleware gamemode off

# Debug mode (show all keyboard events in real-time)
keyboard-middleware debug
```

### Shell Completions

```bash
# Bash
keyboard-middleware completion bash | sudo tee /usr/share/bash-completion/completions/keyboard-middleware

# Zsh
keyboard-middleware completion zsh | sudo tee /usr/share/zsh/site-functions/_keyboard-middleware

# Fish
keyboard-middleware completion fish > ~/.config/fish/completions/keyboard-middleware.fish
```

## 🐛 Troubleshooting

### "Permission denied" errors

Add yourself to the `input` group:
```bash
sudo usermod -a -G input $USER
```
Then log out and back in.

### "Device or resource busy"

Another process is grabbing your keyboard. Check for:
```bash
# Kill any existing instances
pkill -f keyboard-middleware

# Check for other remapping tools
ps aux | grep -E "kmonad|keyd|xremap"
```

### Config errors

Watch the logs when editing config:
```bash
journalctl -u keyboard-middleware -f
```

Config errors show desktop notifications and keep the previous working config.

### Hot-reload not working

Ensure the daemon is running:
```bash
systemctl status keyboard-middleware
```

Check file watcher is working (should see "Config reloaded" in logs when you save).

### "HRM triggers hold too fast/slow"

Adjust `tapping_term_ms`:
- **100-130ms**: More sensitive to holds
- **150-200ms**: More sensitive to taps
- **Recommended**: 130 for home row mods, 150+ for laptops

### "W+S both pressed in game, not moving"

Use SOCD in game_mode (see configuration examples above).

## 📚 Related Projects

Alternative keyboard remapping tools with different approaches:

- **[kmonad](https://github.com/kmonad/kmonad)** - Haskell-based, cross-platform, mature codebase, S-expression config
- **[keyd](https://github.com/rvaiya/keyd)** - C-based, key remapping via config files, active development
- **[kanata](https://github.com/jtroo/kanata)** - Rust-based, cross-platform, programmable key remapper
- **[xremap](https://github.com/xremap/xremap)** - Python-based, X11 only, older project

Each has different strengths - choose based on your platform, performance needs, and configuration preferences.

## 📚 Further Reading

- [QMK Documentation](https://docs.qmk.fm/) - Inspiration for this project
- [Home Row Mods Guide](https://precondition.github.io/home-row-mods) - Deep dive into HRM techniques
- [Colemak Mod-DH](https://colemakmods.github.io/mod-dh/) - Alternative keyboard layout with better HRM placement

## 🤝 Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## 📄 License

MIT License - See [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- Inspired by [QMK Firmware](https://qmk.fm/)
- Built with Rust and [evdev](https://github.com/emberian/evdev)
