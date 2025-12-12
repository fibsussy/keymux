# Keyboard Middleware

A Rust userspace keyboard middleware that intercepts keyboard events and re-emits them through uinput, providing QMK-like functionality for any keyboard without flashing firmware.

## Features

- **Home Row Mods**: Convert home row keys (ASDF/JKL;) into modifiers when held, or regular keys when tapped
  - A → GUI/Super, S → Alt, D → Ctrl, F → Shift (left hand)
  - J → Shift, K → Ctrl, L → Alt, ; → GUI/Super (right hand)
  - Configurable tapping term (default 130ms)
  - Permissive hold behavior for quick typing

- **SOCD Cleaner**: Simultaneous Opposite Cardinal Direction cleaning for gaming
  - Last-input-priority for W/S and A/D
  - Prevents impossible inputs in competitive games
  - Automatically resolves conflicting directional inputs

- **Game Mode Auto-Detection**: Automatically enters game mode when rapid WASD alternation is detected
  - Exits on GUI/Alt key press
  - Disables home row mods in game mode
  - Enables SOCD cleaning for WASD keys

- **Layer System**: Multiple keyboard layers (currently supporting Base, HomeRowMod, Game)

## Requirements

- Linux with evdev and uinput support
- Root/sudo access (required to grab keyboard device and create virtual device)
- Rust 1.70+ (for building)

## Building

```bash
cargo build --release
```

The binary will be in `target/release/keyboard-middleware`.

## Installation

### Permissions Setup (Important!)

The middleware needs access to `/dev/input/*` devices. You have two options:

**Option A: Add user to input group** (recommended for development):
```bash
sudo usermod -a -G input $USER
# Then log out and log back in
```

**Option B: Run as root** (easier for testing):
```bash
sudo ./target/release/keyboard-middleware
```

### Build and Install

1. Build the project:
```bash
cargo build --release
```

2. Copy the binary to a system location:
```bash
sudo cp target/release/keyboard-middleware /usr/local/bin/
```

3. Create a systemd service (optional but recommended):
```bash
sudo tee /etc/systemd/system/keyboard-middleware.service << EOF
[Unit]
Description=Keyboard Middleware
After=multi-user.target

[Service]
Type=simple
ExecStart=/usr/local/bin/keyboard-middleware
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
```

4. Enable and start the service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable keyboard-middleware
sudo systemctl start keyboard-middleware
```

## Usage

### Running Manually

Run with sudo/root privileges:

```bash
sudo ./target/release/keyboard-middleware
```

The middleware will:
1. Find your keyboard device automatically
2. Grab the device (intercept all events)
3. Create a virtual keyboard
4. Start processing events

### Checking Status

View logs:
```bash
sudo journalctl -u keyboard-middleware -f
```

### Configuration

Currently, configuration is hardcoded but can be modified in `src/main.rs`:

- `TAPPING_TERM_MS`: Time in milliseconds to distinguish tap from hold (default 130ms)
- Home row mod mappings: In `KeyboardState::init_home_row_mods()`
- Game mode detection threshold: In `KeyboardState::check_game_mode_entry()`

## How It Works

1. **Event Capture**: The middleware grabs your physical keyboard using evdev, intercepting all key events

2. **Event Processing**:
   - Home row keys are held in a pending state for the tapping term
   - If another key is pressed (permissive hold), the home row key becomes a modifier
   - If released quickly, it's emitted as a regular keypress
   - In game mode, WASD keys go through SOCD cleaning

3. **Event Emission**: Processed events are emitted through a uinput virtual keyboard that the system sees as a normal keyboard

## Architecture

```
Physical Keyboard → evdev (grabbed) → Middleware Processing → uinput Virtual Keyboard → System
                                           ↓
                                    ┌──────┴──────┐
                                    │  Processing  │
                                    ├─────────────┤
                                    │ Home Row Mod │
                                    │ SOCD Cleaner │
                                    │ Game Mode    │
                                    │ Layer System │
                                    └─────────────┘
```

## Differences from QMK Firmware

**Advantages:**
- Works on any keyboard without flashing
- Easy to modify and test (just recompile Rust code)
- Can use system debugging tools
- Cross-device compatible

**Limitations:**
- Requires running as root/sudo
- Slightly higher latency than firmware (typically <1-2ms, imperceptible)
- Can't work in BIOS/bootloader
- Requires the OS to be running

## Troubleshooting

**"Failed to grab keyboard device"**
- Make sure you're running as root: `sudo ./keyboard-middleware`
- Check that no other program is grabbing the keyboard

**"No keyboard device found"**
- List your input devices: `ls -la /dev/input/by-id/`
- Modify the device detection logic in `find_keyboard_device()`

**Home row mods feel sluggish**
- Adjust `TAPPING_TERM_MS` to a lower value (try 100-120ms)
- Recompile after changes

**SOCD not working**
- Make sure game mode is active (rapid WASD alternation)
- Check logs: `sudo journalctl -u keyboard-middleware -f`

## Similar Projects

- [kmonad](https://github.com/kmonad/kmonad) - Haskell-based keyboard remapper
- [kanata](https://github.com/jtroo/kanata) - Rust keyboard remapper inspired by QMK
- [keyd](https://github.com/rvaiya/keyd) - Linux key remapping daemon

## License

MIT

## Contributing

This is a personal project migrated from a QMK keymap. Feel free to fork and adapt to your needs!

## Credits

Based on the QMK keymap from `lemokey-x1-fibs` with features including:
- Home row mods with permissive hold
- SOCD cleaning with last-input priority
- Automatic game mode detection
- Raw HID communication (not yet implemented in userspace version)
