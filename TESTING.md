# Testing Guide

## Quick Start

The easiest way to test the middleware is to run it with sudo:

```bash
./run-debug.sh
```

Or manually:

```bash
sudo RUST_LOG=info cargo run
```

## What to Expect

When you run the middleware, you should see:

```
INFO keyboard_middleware: Starting keyboard middleware
INFO keyboard_middleware: Scanning for keyboard devices...
INFO keyboard_middleware: Device #1: /dev/input/event2 - Keychron Lemokey X1
  -> This is a keyboard!
INFO keyboard_middleware: Using keyboard: Keychron Lemokey X1 (/dev/input/event2)
INFO keyboard_middleware: Keyboard middleware ready
```

## Testing Home Row Mods

Once running, the middleware will intercept your keyboard. Test the home row mods:

### Basic Tap Test
- Press and quickly release `A` → should type `a`
- Press and quickly release `S` → should type `s`
- Press and quickly release `D` → should type `d`
- Press and quickly release `F` → should type `f`

### Modifier Test (Permissive Hold)
- Hold `A` and press `C` → should act as `Super+C` (GUI+C)
- Hold `S` and press `C` → should act as `Alt+C`
- Hold `D` and press `C` → should act as `Ctrl+C` (copy)
- Hold `F` and press `C` → should act as `Shift+C` (types `C`)

### Right Hand Home Row
- `J` → Shift when held, `j` when tapped
- `K` → Ctrl when held, `k` when tapped
- `L` → Alt when held, `l` when tapped
- `;` → GUI when held, `;` when tapped

## Testing SOCD Cleaner (Game Mode)

### Entering Game Mode
Rapidly alternate pressing WASD keys (like strafing in a game). After 8 key presses or hitting all 4 keys, you should see:

```
INFO keyboard_middleware: Auto-entering game mode due to WASD activity
INFO keyboard_middleware: Entering game mode
```

### Testing SOCD
Once in game mode:

1. **Vertical SOCD**:
   - Hold `W`, then press `S` → only `S` should be active (last input wins)
   - Release `S` → `W` becomes active again

2. **Horizontal SOCD**:
   - Hold `A`, then press `D` → only `D` should be active
   - Release `D` → `A` becomes active again

3. **Diagonal movement** (should work normally):
   - `W` + `A` → both active
   - `W` + `D` → both active

### Exiting Game Mode
Press any GUI key (Super/Meta) or Alt key. You should see:

```
INFO keyboard_middleware: Exiting game mode
```

Home row mods will be re-enabled.

## Troubleshooting

### "No keyboard device found"
- **Solution**: Run with sudo or add your user to the input group
  ```bash
  sudo usermod -a -G input $USER
  # Then log out and log back in
  ```

### "Failed to grab keyboard device"
- **Solution**: Another program might be grabbing the keyboard. Stop any other keyboard remapping tools.

### Home row mods feel sluggish
- Adjust `TAPPING_TERM_MS` in `src/main.rs` (default is 130ms)
- Try 100-120ms for faster response
- Rebuild after changes

### Keys are delayed or stuck
- Stop the middleware: Press `Ctrl+C` in the terminal
- The keyboard should return to normal immediately
- Check logs for errors

## Stopping the Middleware

Press `Ctrl+C` in the terminal where it's running. The keyboard will be ungrabbed and return to normal operation immediately.

## Debug Logging

To see detailed event processing:

```bash
sudo RUST_LOG=debug cargo run
```

You'll see every key press/release event being processed:

```
DEBUG keyboard_middleware: Event: Key(KEY_A) pressed=true released=false
DEBUG keyboard_middleware: Home row mod activated by other key: Key(KEY_LEFTMETA)
```

## Common Test Scenarios

### Test 1: Quick Typing (should work normally)
Type: "asdf" quickly → should output "asdf"

### Test 2: Shortcuts
- Hold `D` + press `C` → Copy (Ctrl+C)
- Hold `D` + press `V` → Paste (Ctrl+V)
- Hold `A` + press `T` → Terminal (Super+T, depends on your DE)

### Test 3: Gaming
1. Rapidly press W-A-S-D alternating
2. Game mode should activate
3. Try pressing W+S together → only last pressed should be active
4. Press Super key → exit game mode

## Performance

The middleware typically adds <1-2ms latency, which is imperceptible for normal use and gaming.

## Safety

- The middleware can be stopped instantly with Ctrl+C
- If the process crashes, the kernel will automatically ungrab the keyboard
- Your keyboard will always be usable even if the middleware fails
