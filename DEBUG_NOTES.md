# Implementation Notes

## Goal
Replicate the QMK layout from `../lemokey-x1-fibs/keymaps/fibs/keymap.c` exactly.

## QMK Layout Structure
- **WIN_BASE**: Normal keyboard
- **WIN_HRM**: Home row mods overlay (HM_A, HM_S, HM_D, HM_F, HM_J, HM_K, HM_L, HM_SCL)
- **WIN_NAV**: Navigation layer (activated by MO(WIN_NAV) on left Alt)
- **WIN_GAME**: Gaming mode with SOCD cleaning
- **WIN_FN**: Function layer

## Current Implementation Status

### ✅ Working
- SOCD cleaner (last-input-priority for WASD)
- Game mode auto-detection
- Basic structure

### ❌ Not Working / Needs Fix
- **Home row mod TAPPING**: Tapping A/S/D/F/J/K/L/; should emit the letter, currently does nothing
- Nav layer not implemented
- Function layer not implemented
- Tap dance not implemented
- Mouse keys not implemented

## Key Definitions from QMK
```c
#define HM_A   MT(MOD_LGUI, KC_A)   // Tap: A, Hold: GUI/Super
#define HM_S   MT(MOD_LALT, KC_S)   // Tap: S, Hold: Alt
#define HM_D   MT(MOD_LCTL, KC_D)   // Tap: D, Hold: Ctrl
#define HM_F   MT(MOD_LSFT, KC_F)   // Tap: F, Hold: Shift

#define HM_J   MT(MOD_RSFT, KC_J)   // Tap: J, Hold: Shift
#define HM_K   MT(MOD_RCTL, KC_K)   // Tap: K, Hold: Ctrl
#define HM_L   MT(MOD_RALT, KC_L)   // Tap: L, Hold: Alt
#define HM_SCL MT(MOD_RGUI, KC_SCLN) // Tap: ;, Hold: GUI
```

## Next Steps
1. Fix home row mod tapping behavior - PRIORITY
2. Implement NAV layer with MO(WIN_NAV) on left Alt
3. Test thoroughly
