# Changelog

## 1.0.1 — 2026-11-25

Release config fix: `verbose_logging` now defaults to **`false`** and the shipped
`wish_tilt.json` sets it to `false`, so a fresh install does not write a `TILT` line every 0.5° of
movement. No behaviour change; set it to `true` for the diagnostics described in the README.

Artifacts (built 2026-11-25 21:41):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (357 376 B) | `3CDF72175116537E382E82336D68F55DF8E64F1A03947D33AADA9276AEE7E926` |
| `Modloader/config/wish_tilt.json` | `9FAF79A82858E9316785D1FB7AB46B2501BF4262E774C315181CC002DB149F8A` |

31 unit tests.

## 1.0.0 — 2026-11-25

First public release, MIT © 2026 Duumvir-hf. Feature set as 0.5.0:

* the wish keys (Z/X by default) move a persistent target for the engine hull stabiliser; holding
  both levels it after 0.5 s, with a 0.5 s debounce;
* the wish's rate scales with the displayed battle TWR (heavy hulls aim slower; 3.0+ unchanged);
* the player's ship panel prints `TILT -22.5` in its empty bottom-left corner;
* `m_mdl_at_multiplier` (default 10) gives statics-heavy hulls the torque budget to hold a tilt
  instead of backflipping — **mostly redundant**, the same effect is an `OL.seria` edit (README);
* QOL's anti-tilt frame-time scaling composes; the torque is scaled exactly once.

Artifacts (built 2026-11-25 21:37):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (357 376 B) | `6C0CC658C6A31C3F803EF7A756B85546F57D28B70BE2EF3B4BC3FECAA5C75CEC` |
| `Modloader/config/wish_tilt.json` | `F6E83185C1E28CEC4017D360F62F0509FF045C24B07A55485ACC71A5F5062352` |

31 unit tests. Installed in `release\Modloader\` and `163 Highfleet\Modloader\`.

## 0.5.0 — 2026-11-25

**`m_mdl_at` multiplier and release prep.** New `m_mdl_at_multiplier` (default **10.0**): on the
first battle frame the mod multiplies every `Stats.m_mdl_at` in memory, which is exactly what
editing `Libraries/OL.seria` does — the per-engine clamp and the gate scale together, so the share
is unchanged and only the torque budget grows. It is what stops statics-heavy hulls (Sevastopol)
backflipping above roughly 4° of held tilt while flying with WASD; `10.0` is the tested value,
`1.0` leaves the game's stabiliser vanilla, and it must **not** be combined with an `OL.seria` edit
(that would be ×100). It applies to every ship, AI included.

Release prep: README rewritten for players (with the multiplier's redundancy and the equivalent
`OL.seria` edit stressed), DESIGN gained the multiplier section and the play-check results, and the
test install's `Libraries/OL.seria` was restored to vanilla after the ×10 experiment.

Artifacts (built 2026-11-25 21:26):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (357 376 B) | `99504A1C8DC8C5737CBF60FF6EFDFCD2D16710417912EC030094FDE67335CCFC` |
| `Modloader/config/wish_tilt.json` | `F6E83185C1E28CEC4017D360F62F0509FF045C24B07A55485ACC71A5F5062352` |

31 unit tests. Installed in `release\Modloader\` and `163 Highfleet\Modloader\`.

## 0.4.2 — 2026-11-25

**Fixed which TWR the scaling uses.** The mod scaled by the **battle** TWR (`Creature + 0x1604`,
a design's `m_tele_dynamic`), but the TWR the shipbuilder displays — and the one the request refers
to — is the **strategic/displayed** scalar at `+0x160c` (`m_tele_dynamic_map`). A Sevastopol
logged `twr 2.34` and 17.5 deg/s; it now reads `twr 1.37` and sits on the 1 deg/s floor. Fallbacks
are the battle TWR, then the recomputed `(thrust)/(mass*9.82)`.

The softener exponent is now configurable: `twr_exponent` (default **1.5**; `1.0` = linear,
`2.0` = square, higher = sharper). With the defaults a 2.5 TWR ship gets 27 deg/s, 2.0 gets 13,
1.5 gets 2.4.

Artifacts (built 2026-11-25 20:20):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (356 864 B) | `5C84039A611C1DE3BAE8BB61B18D1D8D9501A9CEBC269BFC209271804057E7F5` |
| `Modloader/config/wish_tilt.json` | `A7B51FFE150BCC305877FF54DEA4C7515599661C18C205903F72443B4A0809A9` |

31 unit tests. Installed in `release\Modloader\` and `163 Highfleet\Modloader\`.

## 0.4.1 — 2026-11-25

**Harsher TWR curve and a configurable bottom.** The square now spans `twr_zero` .. `twr_reference`
instead of starting at 0: `ratio = ((TWR - twr_zero) / (reference - twr_zero))^2`, zero at/below
`twr_zero` (new key, default **1.25**), 1 at the reference, no boost above it. With the defaults a
2.5 TWR ship gets 23 deg/s, 2.0 gets 8, and 1.5/below sit on the 1 deg/s floor.

The upper clamp is kept and documented as a **knee shifter**: with `degrees_per_second` above
`twr_max_degrees_per_second` (e.g. 90 and 45) the output stays flat at 45 down to about 2.5 TWR,
then falls steeply, instead of starting to fall at the reference.

Artifacts (built 2026-11-25 20:10):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (355 328 B) | `B2C6507A325BB736B4D25948C680E7BC4C658DF816BA3C1F15BE997281DD4CE4` |
| `Modloader/config/wish_tilt.json` | `55BA541DE813DAA55506E9E3F5E20828DEA639563F356B40CF73A50972968EB4` |

30 unit tests. Installed in `release\Modloader\` and `163 Highfleet\Modloader\`.

## 0.4.0 — 2026-11-25

**Heavy ships aim their wish tilt slower.** The target's movement rate is now scaled by the ship's
battle TWR, `(left thrust + right thrust) / (mass * 9.82)` (the game's own `Creature + 0x1604`,
recomputed from `+0x1664/+0x1668` and `+0x15b8` so an empty bank does not zero it):

```text
rate = degrees_per_second * min(1, (TWR / twr_reference)^2)
rate = clamp(rate, twr_min_degrees_per_second, twr_max_degrees_per_second)
```

At the defaults (`twr_reference` 3.0, base 45, clamps 1..45): TWR 3.0 and above stay at 45 deg/s
(not boosted), 2.5 -> 31, 2.0 -> 20, 1.5 -> 11, 1.0 -> 5, and anything below ~0.45 sits on the
1 deg/s floor. A negative `degrees_per_second` keeps its sign. The `TILT` log now carries `twr` and
the effective `rate`.

Artifacts (built 2026-11-25 20:02):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (353 792 B) | `4EA4CDEBC76B848337675ABBAE8E37017FF17DF906D52E1EE006D0C7FDFB71CE` |
| `Modloader/config/wish_tilt.json` | `78E36236902C13886AB46AC43551DF2FA00F503C2A643C87903CA82764F9C6F2` |

29 unit tests. Installed in `release\Modloader\` and `163 Highfleet\Modloader\`.

## 0.3.0 — 2026-11-25

**The wish tilt has its own keys; Q/E go back to being pure vanilla flight controls.** Holding
Q/E to aim the wish meant a light ship's own spin hid the target, and limiting that spin would
have fought the engine physics. The wish is now edited by `left_key` / `right_key` in
`wish_tilt.json` (defaults **Z** / **X**), read once per frame with `GetAsyncKeyState` from the
stabiliser hook; the game's input record is never written. `invert` still swaps the two
directions, and holding both keys still levels the target after `level_hold_seconds`. The readout
(labelled `TILT`) and everything else in 0.2.0 are unchanged; the `TILT` log prints `l=`/`r=`
instead of `q=`/`e=`.

Artifacts (built 2026-11-25 19:37):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (348 672 B) | `9011897D91FF1C0A6004C43CC1686AA6AE58F476C1C6A380CEC4A5E45D8EFF1E` |
| `Modloader/config/wish_tilt.json` | `50B28BBEE4DB966672722CFDA31F8EED73E5096F340517C517AFA17B1D23C942` |

27 unit tests. Installed in `release\Modloader\` and `163 Highfleet\Modloader\` (the game had to be
closed first; the 0.2.0 DLL was still loaded).

## 0.2.0 — 2026-11-25

**Wish tilt readout in the ship panel.** The player's 350 x 300 battle ship panel (TADS timer,
ASM/AAM/GPB, FUEL/EVAC/FSS) now prints the target in its empty bottom-left corner, at panel pixel
(30, 260), in the panel's own `vcr_30` font: `TILT -22.5` (signed degrees, the left key aims
negative). The enemy/target panel is skipped. It is drawn inside `DrawBattleShipPanel` right before
its `DrawNoiseHudOverlay` pass, so it gets the panel's camera and scanline treatment. If another
mod owns that call, the readout is skipped and the stabiliser still installs.

Artifacts (built 2026-11-25 19:24):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (342 016 B) | `82390317FDD700216ED327B3A211FCB55E448F3875F38E2A6C9FD671458F9E0C` |
| `Modloader/config/wish_tilt.json` (unchanged) | `757B47E691166D9B60734BAB224EE9A320D1767E6033E52A064DFFCD5B7CEAD5` |

25 unit tests.

## 0.1.2 — 2026-11-25

**The target moved at 45°/s × the number of engine parts.** `retarget` latched the frame's `dt`
correctly, but every later engine of the same frame then called `wish::step` with that *same*
nonzero `dt` again, so a 4-engine ship moved the wish at 180°/s. The step is now **taken** by the
first engine of the frame (`Session::take_dt`, leaving `0.0`), which is the contract
`wish::Wish::step` documents. Regression test added (22 tests).

## 0.1.1 — 2026-11-25

Fix found while reviewing 0.1.0 before its first play check: the once-per-frame latch sat
*before* the player-ship test, and the game runs the stabiliser case for every creature in one
loop. An AI ship's engine visited before the player's consumed the frame's `dt`, so in a battle
the player's own engines would integrate the wish with `dt = 0` on those frames (the target would
barely move while the wish keys are held). The player test now comes first, and a new hull starts
on a fresh frame (`clock = NaN`). No behaviour change outside a battle; the state machine, config
and patch site are untouched.

Second fix: the trampoline wrapped `patchy::preserved_call` in `SUB RSP,8` / `ADD RSP,8`, copied
from the function-entry alignment rule. This site is **mid-function** (`RSP` = 0 mod 16) and
`preserved_call` preserves alignment (192 B), so the padding called the Rust hook 8 bytes off the
Windows x64 alignment (`movaps` spills would fault). Padding removed; the unit test now asserts
it stays out.

Frame-time scaling: the stabiliser torque is applied once per frame and is not multiplied by the
frame time, so at 100 fps it is twice as stiff per second as at the vanilla 50 fps. The torque is
now multiplied by `dt * 50` (`0x1403c126c`, 1.0 at 50 fps). QOL's `enable_antitilt` applies the same
factor; its patch moved from this hook site to `Body_AddTorque`'s entry so the two compose. This mod
checks those nine entry bytes on its first retarget and leaves the factor to QOL when the patch is
present, so the torque is scaled exactly once either way (the choice is logged once, and `TILT`
lines print `x{scale}`).

Artifacts (built 2026-11-25 11:38):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (338 432 B) | `EBED90470009CBA3F0AE96EE633328BD42B75F47A65698C4E433EE0E9098808C` |
| `Modloader/config/wish_tilt.json` (unchanged) | `757B47E691166D9B60734BAB224EE9A320D1767E6033E52A064DFFCD5B7CEAD5` |

21 unit tests.

## 0.1.0 — 2026-11-25

First build. The wish keys move a persistent **wish tilt** at 45°/s and the engine `m_mdl_at`
stabiliser aims at it instead of at 0°. Holding both keys for 0.5 s levels the target; 0.5 s of
the keys is then ignored so releasing the combo cannot nudge it. Q/E still fly the ship exactly as
vanilla; AI ships and the arcade ruleset are untouched.

* one 6-byte inline detour at `0x1400357c5` (`creatureLogic` case 200, immediately before
  `Body_AddTorque`), continuation `0x1400357cb`; the stolen `MOV RAX,[RDI]` / `MOV RCX,RDI` are
  replayed, and only the preserved `XMM1` slot is written.
* `Body_GetInertiaAboutOrigin` `0x140048440`, `GetMdlAt` `0x140048720` and `Profile_IsPlayerShip`
  `0x140293920` are called exactly as the game calls them; `kp` is read from the game's own saved
  `XMM2`; `frac` from `creature+0x180c`.
* config `Modloader/config/wish_tilt.json`: `degrees_per_second` 45, `invert` false,
  `level_hold_seconds` 0.5, `level_debounce_seconds` 0.5, `verbose_logging` true.
* 19 unit tests (state machine, config, protocol), `cargo test` green, no warnings.

Artifacts (built 2026-11-25):

| file | sha256 |
|---|---|
| `Modloader/mods/hf_wish_tilt.dll` (334 848 B) | `714F3F8D10B640D5384ABB00FEE26984EB0540A6BAD40E79080F8C93DAA332D9` |
| `Modloader/config/wish_tilt.json` | `757B47E691166D9B60734BAB224EE9A320D1767E6033E52A064DFFCD5B7CEAD5` |
