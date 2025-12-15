# Keyboard Middleware

A multi-keyboard middleware daemon for Linux that provides:

- **Home Row Mods**: A/S/D/F and J/K/L/; act as modifiers when held, letters when tapped
- **SOCD Cleaner**: Last-input-priority for WASD in game mode
- **Game Mode**: Automatic detection via niri window manager (gamescope = game mode)
- **Per-Window Overrides**: Press both shifts to override game mode for specific windows (tracked by PID)
- **Nav Layer**: Left Alt + HJKL for arrow keys, ASDF for modifiers
- **Multi-keyboard Support**: Handle multiple physical keyboards simultaneously
- **Hotplug Support**: Automatically detect keyboard connect/disconnect via udev
- **Per-keyboard Configuration**: Enable/disable individual keyboards

## Features

- Caps Lock → Escape
- Escape → Caps Lock
- 130ms tapping term for home row mods
- IPC-based daemon/client architecture
- Systemd user service integration
- **Safe Eject**: Press Equals (=) key to release all modifiers and shut down daemon

## Installation

### Quick Install (Recommended)

Run the installation script:

```bash
./install.sh
```

This will:
- Build the package with makepkg
- Install it with pacman
- Enable and start the systemd service
- Automatically clean up build artifacts
- Rollback on failure (atomic installation)

**Note**: Add yourself to the `input` group first:
```bash
sudo usermod -a -G input $USER
# Then log out and log back in
```

### Manual Installation

#### Build from source

```bash
cargo build --release
sudo cp target/release/keyboard-middleware /usr/bin/
```

#### Install as Arch Linux package

```bash
makepkg -si
```

This will install the binary to `/usr/bin/keyboard-middleware` and the systemd service to `/usr/lib/systemd/user/keyboard-middleware.service`.

#### Enable systemd service

```bash
# Add your user to the input group (required for device access)
sudo usermod -a -G input $USER

# Log out and log back in for group membership to take effect

# Enable and start the service
systemctl --user enable keyboard-middleware
systemctl --user start keyboard-middleware

# Check status
systemctl --user status keyboard-middleware
```

## Usage

### Daemon Commands

```bash
# Start daemon (usually done via systemd - default if no command specified)
keyboard-middleware
keyboard-middleware daemon
```

### Management Commands

```bash
# Set password for nav+backspace password typer (interactive prompt)
keyboard-middleware set-password
```

### Configuration

Configuration is stored at `~/.config/keyboard-middleware/config.toml`:

```toml
tapping_term_ms = 130
enable_game_mode_auto = true
enable_socd = true
password = "your-password-here"
```

**Password configuration**: Set the `password` field to enable the password typer (Nav + Backspace). The password can contain letters, numbers, and common symbols.

**Note**: After changing which keyboards are enabled via `toggle-keyboards`, restart the daemon for changes to take effect:

```bash
systemctl --user restart keyboard-middleware
```

## Home Row Mods Layout

### Left Hand
- A → Super/Meta (when held)
- S → Alt (when held)
- D → Ctrl (when held)
- F → Shift (when held)

### Right Hand
- J → Shift (when held)
- K → Ctrl (when held)
- L → Alt (when held)
- ; → Super/Meta (when held)

## Nav Layer

Hold Left Alt to activate the navigation layer:

### Arrow Keys
- H → Left
- J → Down
- K → Up
- L → Right

### Modifiers (when in nav layer)
- A → Super/Meta
- S → Alt
- D → Ctrl
- F → Shift

### Mouse Buttons (arrow key cluster)
- Arrow Up → Middle Click
- Arrow Left → Left Click
- Arrow Down → Middle Click
- Arrow Right → Right Click

### Password Typer
- Left Alt + Backspace (first press) → Type configured password
- Left Alt + Backspace (subsequent presses) → Press Enter
- State resets when leaving nav layer (releasing Left Alt)

## Game Mode

### Niri Window Manager Integration (Automatic Only)

Game mode is **only** controlled by the niri monitor watching window focus:

**Automatic behavior:**
- `gamescope` windows → Game mode **ON**
  - Left hand home row mods (ASDF) **disabled** → keys pass through for WASD gaming
  - Right hand home row mods (JKL;) **still active** → modifiers available while gaming
  - Nav layer (Left Alt) **disabled** → Alt passes through as regular key
  - SOCD cleaner **enabled** for WASD (last-input-priority)
- All other windows → Game mode **OFF**
  - All home row mods active
  - Nav layer active

**No manual controls:** Game mode cannot be toggled manually - it is purely automatic based on which window has focus.

## Safe Eject

**Emergency shutdown**: Press the **Equals (=)** key to immediately:
- Release all held modifiers
- Shut down the daemon cleanly
- Restore full keyboard control

Use this if:
- Keys get stuck
- Modifiers won't release
- The daemon is misbehaving
- You need to quickly regain normal keyboard control

After pressing equals, restart the daemon with:
```bash
systemctl --user restart keyboard-middleware
```

## Requirements

- Linux with evdev support
- User must be in the `input` group
- systemd (for service management)
- **Optional**: niri window manager (for automatic game mode detection)

## Troubleshooting

### "Device or resource busy" errors

Another process is already grabbing the keyboard devices. Check for:
- Other keyboard-middleware instances: `pkill -f keyboard-middleware`
- Other input remapping tools

### Daemon not starting

Check logs:
```bash
journalctl --user -u keyboard-middleware -f
```

### IPC connection failures

Ensure daemon is running:
```bash
keyboard-middleware ping
```

## License

MIT
