# hf_wish_tilt

**HighFleet Steam 1.163** — the engines' hull stabiliser aims at a **wish tilt** you set with your
own keys, instead of always at level. Q/E and every other flight control stay vanilla.

Version **1.0.0** — MIT — [changelog](CHANGELOG.md) — [design notes and engine reference](docs/DESIGN.md)

---

Every engine's `m_mdl_at` stabiliser normally pulls the hull back to **0°**, harder the further
away it is. That 0° is hard-coded. This mod replaces it with a target you can move:

* hold **Z** / **X** (configurable) — the wish tilt moves left / right at **45°/s** (0.5 s = 22.5°);
* hold **Z + X** for **0.5 s** — the wish tilt snaps back to level;
* after that combo the keys are ignored for **0.5 s**, so letting go cannot nudge it;
* heavy ships move the wish **slower** than light ones (battle TWR scaling, below);
* a **readout** in the ship panel shows the current target.

Q/E still fly and spin the ship exactly as stock. The mod only changes *where the stabiliser aims*.
Tap Z for half a second, then when the engines spool up they pull the hull to 22.5° off level and
hold it there — and the readout shows the number, so the hull's own spinning no longer hides where
the target is.

## Install

```
Modloader/mods/hf_wish_tilt.dll
Modloader/config/wish_tilt.json
```

Requires logdot's Modloader. The DLL refuses to start on anything but `Steam 1.163`. If the config
is missing it is created with the defaults below.

## Wish tilt readout

The empty bottom-left corner of your ship panel (the 350 × 300 panel with the TADS timer, the
ASM/AAM/GPB counts and FUEL/EVAC/FSS) prints the target:

```
TILT -22.5
```

in the panel's own `vcr_30` font at panel pixel (30, 260). Signed degrees: the left key aims
negative, the right key positive, `0.0` = level. The enemy/target panel on the right never shows it.
If another mod owns the panel's draw call, the readout is skipped (the log says so) and the
stabiliser keeps working.

## Heavy ships aim slower (TWR scaling)

The target's rate is scaled by the ship's **displayed TWR** — the shipbuilder/strategic value at
`Creature + 0x160c` (`m_tele_dynamic_map` in a design), the number the editor's TWR stat shows
(falls back to the battle TWR, then to recomputed thrust/mass):

```
ratio = ((TWR − twr_zero) / (twr_reference − twr_zero)) ^ twr_exponent   (0 below twr_zero, 1 above the reference)
rate  = clamp(degrees_per_second × ratio, twr_min…, twr_max…)
```

At the defaults (reference 3.0, zero 1.25, exponent 1.5, base 45, clamps 1…45): TWR **3.0 and
above stay at 45°/s** (not boosted), 2.5 → 27, 2.0 → 13, 1.5 → 2.4, and at/below 1.25 sits on the
1°/s floor (a Sevastopol-like 1.37 is on the floor). `twr_exponent` is the softener: `1.0` linear,
`2.0` square. The **upper clamp is also a knee shifter**: with a base above it (e.g. 90 and 45) the
output stays flat at 45 until the curve drops under the clamp, then falls steeply. The `TILT` log
prints the ship's `twr` and the effective `rate`.

## The `m_mdl_at` multiplier — mostly redundant, read this

```json
"m_mdl_at_multiplier": 10.0
```

This multiplies every engine's **`m_mdl_at`** — the stabiliser's per-engine torque budget — in
memory at the first battle. It is **exactly the same thing** as changing the same numbers in
`Libraries/OL.seria`, just done for you:

* If you are comfortable editing a text file, **set this to `1.0` and edit `OL.seria` instead** —
  the result is identical and survives without the mod. It is mostly redundant for that reason.
* It exists because statics-heavy hulls (Sevastopol and friends) can **backflip** when a wish tilt
  above roughly 4° is held while flying with WASD. At a few degrees of tilt the statics' `cos`
  gate switches one side's engines off while the mirror side keeps firing, and the game's default
  stabiliser budget is too small to hold the twist. ×10 gives it the headroom (the log's `m_at`
  goes from 7.0e+07 to 7.0e+08).
* It applies to **every ship in the game, AI included** — the stats records are shared.
* **Do not combine it with an `OL.seria` edit.** If you already multiplied `m_mdl_at` there, set
  this to `1.0`, or you get ×100.
* `1.0` leaves the game's stabiliser exactly vanilla.

The nine numbers in `Libraries/OL.seria` (under `m_oid=…`), if you prefer the file edit:

| `m_oid` | default `m_mdl_at` | ×10 |
|---|---|---|
| `MDL_ENGINE_01` | `7e+07` | `7e+08` |
| `MDL_ENGINE_02` | `7e+07` | `7e+08` |
| `MDL_ENGINE_02_OLD` | `7e+07` | `7e+08` |
| `MDL_ENGINE_03` | `1e+07` | `1e+08` |
| `MDL_ENGINE_03_OLD` | `1e+07` | `1e+08` |
| `MDL_ENGINE_04` | `1e+07` | `1e+08` |
| `MDL_ENGINE_05` | `1.5e+07` | `1.5e+08` |
| `MDL_ENGINE_05_OLD` | `1.5e+07` | `1.5e+08` |
| `MDL_ENGINE_06` | `4e+06` | `4e+07` |

## Config — `Modloader/config/wish_tilt.json`

| key | default | meaning |
|---|---|---|
| `enabled` | `true` | false = not installed at all |
| `degrees_per_second` | `45.0` | rate at/above `twr_reference` (0.5 s = 22.5°) |
| `left_key` | `"Z"` | key that aims the wish left |
| `right_key` | `"X"` | key that aims the wish right |
| `invert` | `false` | swap which key aims which way |
| `twr_reference` | `3.0` | displayed TWR at which the full rate applies |
| `twr_zero` | `1.25` | displayed TWR where the curve reaches zero |
| `twr_exponent` | `1.5` | curve exponent: `1.0` linear, `2.0` square, higher is sharper |
| `twr_min_degrees_per_second` | `1.0` | lower clamp for the scaled rate |
| `twr_max_degrees_per_second` | `45.0` | upper clamp; below the base it flattens the curve and sharpens the knee |
| `m_mdl_at_multiplier` | `10.0` | see the section above; `1.0` = leave the game alone |
| `level_hold_seconds` | `0.5` | how long both keys must be held to snap to level |
| `level_debounce_seconds` | `0.5` | how long the keys are ignored after that |
| `verbose_logging` | `true` | one `TILT` line per 0.5° of target movement |

Numbers are taken as written; a negative `degrees_per_second` just swaps the two keys, and a
negative `m_mdl_at_multiplier` is ignored. Only a non-finite number falls back to its default.
Unknown keys are reported and ignored.

## Compatibility

* **QOL's anti-tilt scaling** composes with this mod. QOL applies the `dt × 50` factor at
  `Body_AddTorque`'s entry, after this mod has retargeted the torque; this mod detects that patch
  and leaves the factor to QOL, so the torque is scaled exactly once (`x1.00` in the log when QOL
  owns it, `x0.50` at 100 fps when this mod does).
* **Other mods hooking `0x1400357c5`** (the stabiliser case) will collide. If the bytes differ,
  this mod logs why and backs out cleanly; nothing is patched.
* **`m_mdl_at_multiplier`** changes the same data other engine/tuning mods may touch. Set it to
  `1.0` if another mod edits `OL.seria` for you.

## What it does not do

* **Q/E are not changed.** They still fly the ship. The wish keys do not steer it.
* **Nothing happens without thrust.** Vanilla only applies the tilt torque while the engines push
  (and for ~0.6 s after), so a coasting ship keeps drifting — the same rule as vanilla's
  "return to level", with a new target.
* **Some ships never get the tilt torque at all.** Vanilla drops the angle term on any tick where
  one of the two thrust banks made no force (a statics-only design has a bank that never fires),
  and it is 0 in the arcade ruleset. Those ticks are left exactly as vanilla made them.
* **AI ships keep vanilla level-seeking.** They never press the wish keys.
* **A new hull starts level.** The target is forgotten when the ship you fly changes.

## Log

```
hf_wish_tilt: player ship at 0000021A...; the wish keys move the wish tilt
hf_wish_tilt: m_mdl_at x10 applied to 412 stats records (the same as editing Libraries/OL.seria); every ship and the AI see it
TILT t=412.320 wish=-22.5deg hull=41.7deg l=1 r=0 banks=1/1 torque -3.1e+06 -> 4.4e+06 (m_at 7.0e+08, x1.00, twr 1.37, rate 1.0deg/s)
```

`wish` is the target, `hull` the current attitude, `l`/`r` the wish keys, `banks` whether each
thrust bank fired this tick, `torque` before/after the retarget, `m_at` the engine's budget, `x` the
frame-time factor, `twr` the displayed TWR and `rate` the wish speed it produced.

If the loader logs `bytes ... differ from the Steam 1.163 build`, another mod owns
`0x1400357c5` and this one backed out cleanly. The `TILT` lines only appear once both thrust banks
fire, i.e. while thrusting — that is expected.

## Design notes

The hook, the byte evidence, the stabiliser arithmetic, the readout and the TWR/multiplier details:
[docs/DESIGN.md](docs/DESIGN.md).

## Licence

MIT © 2026 Duumvir-hf — see [LICENSE](LICENSE). `hf_wish_tilt` is a standalone mod; it is not
affiliated with the HighFleet developers or with logdot's Modloader, which it depends on.
