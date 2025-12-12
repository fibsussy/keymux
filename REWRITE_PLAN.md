# Home Row Mod Rewrite Plan

## The Problem
Current implementation decides too early if a key is mod or tap, causing issues when:
- Holding one home row mod and tapping another (e.g., K hold + A tap for Ctrl+A)
- Fast typing where timing is ambiguous

## How QMK Does It
QMK uses **deferred decision making**:

1. **On home row key press**: Buffer it, don't emit yet
2. **On another key press while buffered**:
   - Emit the buffered key as MODIFIER
   - Emit the new key normally
3. **On timeout (130ms) without other press**:
   - Emit as tap
4. **On release before timeout**:
   - Emit as tap

## Implementation Strategy

### Option 1: Event Queue (Complex but correct)
- Maintain queue of pending events
- Process queue with timing logic
- Can retroactively fix decisions

### Option 2: State Machine (Simpler)
For each home row key track:
- `Idle` - not pressed
- `Pending` - pressed, waiting to decide (< 130ms, no other key)
- `Decided(Mod)` - decided it's a modifier
- `Decided(Tap)` - decided it's a tap

State transitions:
- `Idle` → press → `Pending`
- `Pending` → another key pressed → `Decided(Mod)` + emit modifier
- `Pending` → timeout (130ms) → stay `Pending` (it's now a hold)
- `Pending` → release (< 130ms, no other key) → `Decided(Tap)` + emit tap
- `Pending` → release (>= 130ms, no other key) → `Decided(Tap)` + emit tap
- `Decided(Mod)` → release → emit modifier release, go to `Idle`

## Key Insight for Home Row + Home Row
When pressing home row key B while home row key A is in `Pending`:
1. A transitions to `Decided(Mod)` immediately
2. Emit A's modifier
3. Put B into `Pending` state
4. If B is tapped quickly, emit B's base key (with A's modifier still held)
5. Result: Modifier-A + tap-B = correct combo!

## Implementation
Need to check state of ALL held home row mods when ANY key is pressed.
