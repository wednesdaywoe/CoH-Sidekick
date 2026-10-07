# Enhancement Icon Construction

How set-IO / ATO / event enhancement icons are built for the planner.

A finished enhancement icon is **three stacked layers**:

1. **Symbol** (foreground) — the art unique to the set.
2. **POG** (background) — a colored disc keyed to the set's primary enhancement
   aspect. The color *is* the meaning (blue = endurance, green = heal, etc.).
3. **Frame** (outer ring) — the rarity / attunement border.

You build layers **1 + 2 by hand** and save the flattened result. **The planner
adds layer 3 automatically at render time — never bake a frame into the saved
icon.**

```
  symbol.png        E_POG_*.png         saved as s<Name>.png       (planner adds)
 ┌─────────┐       ┌─────────┐         ┌─────────┐               ┌─────────┐
 │  ╱◇╲   │   +   │ ███████ │    =    │ ●▓◇▓●  │   ──render──▶│ ◍▓◇▓◍│
 │  ╲◇╱   │       │ ███████ │         │ ●▓▓▓●   │   + frame     │ ◍▓▓▓◍ │
 └─────────┘       └─────────┘         └─────────┘               └─────────┘
  Components/        Components/         IO Sets/ (etc.)           Overlay/e_frame_*
  IO Set Icons/      POGS/                                         (dynamic)
```

## This folder (`Components/`) — build-time source material only

Nothing in the app references these at runtime; they exist so icons can be
(re)composited later.

- **`IO Set Icons/`** — bare symbols on a transparent background (~46×46),
  extracted from the Homecoming game data. Foreground layer.
- **`POGS/`** — background discs, one per enhancement aspect (`E_POG_*.png`,
  46×46). Background layer.

## Building an icon

1. Take the symbol from `IO Set Icons/<set>.png`.
2. Pick the POG for the set's **primary enhancement aspect** from `POGS/`.
   **If you're not sure which POG color a set should use, ask — don't guess.**
   The aspect↔color choice is a judgment call and a wrong background looks wrong.
3. Composite: center the symbol on the POG (both 46×46), preserving the symbol's
   alpha. Flatten.
4. Save as **`s<CamelCaseSetName>.png`** into the correct output folder (routing
   below). **No frame ring.**
5. Wire it into the data (see "Wiring", below).

### POG → aspect quick reference

The `POGS/` filenames are self-describing; common picks:

| Set's primary aspect            | POG file |
|---------------------------------|----------|
| Endurance Mod / Recovery        | `E_POG_RECOVERY`, `E_POG_END_DISCOUNT`, `E_POG_END_DRAIN` |
| Healing                         | `E_POG_HEAL` |
| Damage                          | `E_POG_DAMAGE` |
| Damage Resistance               | `E_POG_DAMAGE_RESIST` |
| Defense buff / To-Hit / Accuracy| `E_POG_BUFF_DEFENSE`, `E_POG_BUFF_TO_HIT`, `E_POG_ACCURACY` |
| Recharge / Range / Radius       | `E_POG_RECHARGE_TIME`, `E_POG_RANGE_INCREASE`, `E_POG_RADIUS` |
| Mez (Hold/Stun/Sleep/…)         | `E_POG_HOLD_DURATION`, `E_POG_STUN_DURATION`, `E_POG_SLEEP_DURATION`, … |
| Knockback / Slow                | `E_POG_KNOCKBACK_DISTANCE`, `E_POG_SLOW_MOVEMENT` |
| Travel (Run/Fly/Jump)           | `E_POG_RUN_SPEED`, `E_POG_FLY_SPEED`, `E_POG_JUMP_DISTANCE`, `E_POG_LEAP_HEIGHT` |

(Full list = the files in `POGS/`.)

## Output folder routing (by filename prefix)

`getIOSetFolder()` in `src/components/enhancements/EnhancementIcon.tsx` routes the
saved icon to a subfolder under `public/img/Enhancements/` by prefix:

| Prefix              | Folder        | Use |
|---------------------|---------------|-----|
| `AO_` / `SAO_`      | `Archetype/`  | Archetype Origin / Superior ATO |
| `EO_` / `SEO_`      | `Event/`      | Event sets / Superior |
| `UD_`               | `Universal/`  | Universal Damage |
| anything else (`s…`)| `IO Sets/`    | Normal invention sets |

## Frame overlay (the planner handles this — keep your icon frame-less)

At render, `EnhancementIcon` overlays `Overlay/e_frame_*.png` chosen by the set's
rarity and attunement: `uncommon` / `rare` / `superior` (purple) / `pvp`, each
with an `attuned` variant; ATO and event sets force the attuned-rare/superior
frame. The base icon and the 64×64 frame are scaled into a common box by CSS, so
your icon doesn't need to match the frame's pixel size — 46×46 (POG size) is the
standard.

## Wiring a new or changed icon into the data

Set icons are **not** stored in the game binary, so two edits make the planner
use yours and keep it through a data regen:

1. Set the set's `icon` field in `hand-data/homecoming/io-sets-raw.json` to
   `s<Name>.png`. This is the hand-curated library the extractor reads; it lived at
   `src/data/datasets/homecoming/io-sets-raw.ts` until 2026-09-24.
2. Add the same mapping to `ICON_OVERRIDES` in
   `scripts/extract-rebirth-io-sets-v2.py` (consulted first during regen).

Edit 1 reaches Homecoming and every fork that shares the set; edit 2 reaches every
dataset unconditionally, which is why both are wanted. The generated
`src/data/datasets/<dataset>/io-sets-raw.ts` files are frozen and no longer
regenerated — the TypeScript app still imports them, so an icon has to be set there
too to show up in that app, but nothing in the pipeline reads them.

Worked example — **Preemptive Optimization** (Endurance Mod set):
`IO Set Icons/preemptive_optimization.png` composited on
`POGS/E_POG_RECOVERY.png` → `IO Sets/sPreemptiveOptimization.png`, with both
data edits above.

## Re-extracting a bare symbol from the game data

The symbols in `IO Set Icons/` came from the Homecoming `texture_gui.pigg`:
`texture_library/gui/icons/enhancements/e_icon_<set>.texture`. **Only the newer
sets have a bare-symbol texture there** — most older sets do not, which is why
this folder holds only ~23 symbols.

```python
import sys, io
sys.path.insert(0, 'tools/pigg-wrangler')
from pigg_wrangler.pigg import PiggArchive
from pigg_wrangler.texture import texture_to_png, get_texture_info
from PIL import Image

# assets dir: see the "HC bins Linux path" note (texture_gui.pigg lives there)
p = PiggArchive('<assets>/texture_gui.pigg')
raw = p.extract('texture_library/gui/icons/enhancements/e_icon_<set>.texture')
info = get_texture_info(raw)                                  # logical size, e.g. 46x46
im = Image.open(io.BytesIO(texture_to_png(raw))).convert('RGBA')
im = im.crop((0, 0, info['width'], info['height']))          # drop power-of-two padding
im.save('IO Set Icons/<set>.png')
```
