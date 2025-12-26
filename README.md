# keyboard-middleware ⌨️

**QMK-inspired keyboard customization for Linux** - A blazing-fast, zero-latency keyboard middleware daemon that brings advanced QMK features to any keyboard

## ✨ Features

### Core Functionality
- **Home Row Mods (HRM)**: Tap for letters, hold for modifiers with configurable tapping term
- **OVERLOAD Actions**: Simpler tap/hold without permissive hold logic
- **Custom Layers**: Define unlimited layers (navigation, numpad, symbols, etc.)
- **Game Mode**: Automatic detection via Steam/Gamescope with SOCD support
- **SOCD Cleaner**: Last-input-priority for FPS games (eliminates W+S conflicts)
- **Password Typer**: Securely type passwords with a dedicated key (double-tap adds Enter)
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

## 📊 Comparison with kmonad

| Feature | keyboard-middleware | kmonad | Winner |
|---------|-------------------|--------|--------|
| **Performance** | Native Rust, zero-copy evdev | Haskell runtime | ⚡ keyboard-middleware |
| **Input Latency** | <1ms (direct evdev) | ~2-3ms | ⚡ keyboard-middleware |
| **Memory Usage** | ~2-5MB per keyboard | ~50-80MB | ⚡ keyboard-middleware |
| **Hot-Reload** | Automatic on file save | Manual restart required | ⚡ keyboard-middleware |
| **Multi-Keyboard** | Native support | Single keyboard focus | ⚡ keyboard-middleware |
| **Config Format** | RON (Rust Object Notation) | Custom S-expressions | 🤝 Preference |
| **Layer System** | QMK-style enum layers | S-expression layers | 🤝 Preference |
| **Home Row Mods** | Permissive hold + OVERLOAD | Tap-hold with delays | ⚡ keyboard-middleware |
| **Game Mode** | Automatic Steam/Gamescope detection | Manual toggle | ⚡ keyboard-middleware |
| **SOCD** | Built-in last-input-priority | Not available | ⚡ keyboard-middleware |
| **Password Manager** | Encrypted password typer | Not available | ⚡ keyboard-middleware |
| **Per-Key Timing** | Per-action timing possible | Global timing | ⚡ keyboard-middleware |
| **Shell Completions** | Built-in (bash/zsh/fish) | Manual setup | ⚡ keyboard-middleware |
| **Community** | New project | Mature, large community | ✨ kmonad |
| **Documentation** | Growing | Extensive | ✨ kmonad |
| **Cross-Platform** | Linux only | Linux, Windows, macOS | ✨ kmonad |
| **Established** | New (2025) | Mature (2018+) | ✨ kmonad |

### Why keyboard-middleware?

**Performance-Critical Users:**
- Gamers who need zero input lag and SOCD
- Vim users who need instant mode switches
- Anyone who types fast and values responsiveness

**QMK Users:**
- If you're familiar with QMK's Action system, this will feel natural
- Enum-based layers instead of symbolic expressions
- Direct action mappings like `HR(KC_A, KC_LGUI)`

**Multi-Keyboard Setups:**
- Different configs for laptop keyboard vs external keyboard
- Automatic keyboard detection and config switching
- Enable/disable keyboards without daemon restart

**Convenience Features:**
- Auto hot-reload (edit config, save, done)
- Automatic game mode detection
- Desktop notifications for config errors
- Built-in password typer

### Why kmonad?

- You need cross-platform support (Windows, macOS)
- You prefer S-expression configuration
- You value a mature, battle-tested codebase
- You need features specific to kmonad's ecosystem

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

**Local development:** When run from within the cloned repo, `./install.sh` defaults to building from source. Use `./install.sh bin` for precompiled binary.

### Manual Installation

#### Prerequisites

Add yourself to the `input` group:
```bash
sudo usermod -a -G input $USER
# Log out and log back in for changes to take effect
```

#### From Precompiled Binary (Recommended)

```bash
# Download latest release
VERSION="0.1.0"  # Check https://github.com/fibsussy/keyboard-middleware/releases/latest
ARCH="x86_64"    # or "aarch64"
curl -fsSL -O "https://github.com/fibsussy/keyboard-middleware/releases/download/v${VERSION}/keyboard-middleware-linux-${ARCH}.tar.gz"

# Extract and install
tar -xzf keyboard-middleware-linux-${ARCH}.tar.gz
sudo install -Dm755 keyboard-middleware /usr/bin/keyboard-middleware

# Install systemd service
sudo curl -fsSL -o /usr/lib/systemd/user/keyboard-middleware.service \
    "https://raw.githubusercontent.com/fibsussy/keyboard-middleware/main/keyboard-middleware.service"

# Enable and start
systemctl --user enable --now keyboard-middleware
```

#### From Source

```bash
# Clone and build
git clone https://github.com/fibsussy/keyboard-middleware.git
cd keyboard-middleware
cargo build --release

# Install
sudo cp target/release/keyboard-middleware /usr/bin/
sudo cp keyboard-middleware.service /usr/lib/systemd/user/

# Enable and start
systemctl --user enable --now keyboard-middleware
```

#### Using PKGBUILD (Arch Linux)

```bash
# Clone and build from source
git clone https://github.com/fibsussy/keyboard-middleware.git
cd keyboard-middleware
makepkg -si              # Build from source

# Or use precompiled binary
makepkg -si -p PKGBUILD.bin
```

### Post-Installation Setup

1. **Copy the example config:**
```bash
mkdir -p ~/.config/keyboard-middleware
cp /usr/share/doc/keyboard-middleware/config.example.ron ~/.config/keyboard-middleware/config.ron
# Or from source:
cp config.example.ron ~/.config/keyboard-middleware/config.ron
```

2. **Edit the config:**
```bash
$EDITOR ~/.config/keyboard-middleware/config.ron
```

3. **Select which keyboards to enable:**
```bash
keyboard-middleware toggle
```

4. **(Optional) Set up password typer:**
```bash
# Create password file (plain text, one line)
echo "YourSecurePassword123" > ~/.config/keyboard-middleware/password.txt
chmod 600 ~/.config/keyboard-middleware/password.txt

# Then configure a key to use Action::Password in config.ron
```

5. **(Optional) Shell completions:**
```bash
# Bash
keyboard-middleware completion bash | sudo tee /usr/share/bash-completion/completions/keyboard-middleware

# Zsh
keyboard-middleware completion zsh | sudo tee /usr/share/zsh/site-functions/_keyboard-middleware

# Fish
keyboard-middleware completion fish > ~/.config/fish/completions/keyboard-middleware.fish
```

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

Letters: `KC_A` through `KC_Z`
Numbers: `KC_1` through `KC_0`
Modifiers: `KC_LCTL`, `KC_LSFT`, `KC_LALT`, `KC_LGUI`, `KC_RCTL`, `KC_RSFT`, `KC_RALT`, `KC_RGUI`
Special: `KC_ESC`, `KC_CAPS`, `KC_TAB`, `KC_SPC`, `KC_ENT`, `KC_BSPC`, `KC_DEL`
Function: `KC_F1` through `KC_F12`
Arrows: `KC_LEFT`, `KC_DOWN`, `KC_UP`, `KC_RGHT`

### Available Actions

#### Key(KeyCode)
Direct key mapping.
```ron
KC_CAPS: Key(KC_ESC),  // Caps Lock becomes Escape
```

#### HR(tap_key, hold_key)
Home row mod with permissive hold logic.
```ron
KC_A: HR(KC_A, KC_LGUI),  // Tap for 'a', hold for Super/Win/Cmd
KC_S: HR(KC_S, KC_LALT),   // Tap for 's', hold for Alt
```

#### OVERLOAD(tap_key, hold_key)
Simpler tap/hold without permissive hold.
```ron
KC_SPC: OVERLOAD(KC_SPC, KC_LCTL),  // Tap for Space, hold for Ctrl
```

#### TO(Layer)
Switch to a different layer while held.
```ron
KC_LALT: TO(L_NAV),  // Hold Left Alt to activate navigation layer
```

#### Socd(key1, key2)
SOCD cleaner for gaming (last-input-priority).
```ron
KC_W: Socd(KC_W, KC_S),  // Pressing W then S = S, release S = W again
```

#### Password
Type password from `~/.config/keyboard-middleware/password.txt`.
```ron
KC_BSPC: Password,  // First press types password, second press adds Enter
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
        "2e3c:c365:0110:0003:usb-0000:08:00.3-1/input0",
    ]),

    remaps: {
        KC_CAPS: Key(KC_ESC),
        KC_ESC: Key(KC_GRV),
        KC_LALT: TO(L_NAV),

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

                // Password typer
                KC_BSPC: Password,
            },
        ),
    },

    game_mode: (
        remaps: {
            // Disable home row mods for left hand (WASD gaming)
            // Keep essential remaps
            KC_CAPS: Key(KC_ESC),
            KC_ESC: Key(KC_GRV),

            // SOCD for competitive FPS
            KC_W: Socd(KC_W, KC_S),
            KC_S: Socd(KC_S, KC_W),
            KC_A: Socd(KC_A, KC_D),
            KC_D: Socd(KC_D, KC_A),
        },
    ),

    keyboard_overrides: {},
)
```

#### Per-Keyboard Overrides

```ron
(
    tapping_term_ms: 130,
    double_tap_window_ms: None,
    enabled_keyboards: None,

    remaps: { /* your default keymaps */ },
    layers: {},
    game_mode: (remaps: {}),

    keyboard_overrides: {
        // Different config for laptop keyboard
        "1234:5678:0100:0003:usb-0000:00:14.0-1/input0": (
            settings: (
                tapping_term_ms: 150,  // Laptop keys need longer hold time
                double_tap_window_ms: 400,
            ),
            keymap: (
                base_remaps: {
                    KC_CAPS: Key(KC_LCTL),  // Laptop uses Caps as Ctrl
                },
            ),
        ),
    },
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
systemctl --user status keyboard-middleware

# View live logs
journalctl --user -u keyboard-middleware -f

# Restart daemon
systemctl --user restart keyboard-middleware
```

### Keyboard Management

```bash
# List all detected keyboards
keyboard-middleware list

# Toggle which keyboards are enabled (interactive)
keyboard-middleware toggle

# Config hot-reloads automatically on save - no restart needed!
```

### Password Management

```bash
# Set password interactively (stored in password.txt)
keyboard-middleware set-password
```

### Shell Completions

```bash
# Generate completions
keyboard-middleware completion bash
keyboard-middleware completion zsh
keyboard-middleware completion fish
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
journalctl --user -u keyboard-middleware -f
```

Config errors show desktop notifications and keep the previous working config.

### Hot-reload not working

Ensure the daemon is running:
```bash
systemctl --user status keyboard-middleware
```

Check file watcher is working (should see "Config reloaded" in logs when you save).

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
- Compared with [kmonad](https://github.com/kmonad/kmonad)
- Built with Rust and [evdev](https://github.com/emberian/evdev)
