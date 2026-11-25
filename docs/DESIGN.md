# Design — how the wish tilt is wired into the engine stabiliser

Steam 1.163 (`Highfleetcracked.exe`, image base `0x140000000`). Everything below was read out of
the shipped binary on 2026-11-25, in addition to the existing notes
`hf_notes/docs/findings/physics/hull-stabiliser-engine-logic.md` and
`player-thrust-keys-and-engine-heat.md`.

## 1. Where the "return to level" lives

`creatureLogic` case 200 (`0x140035652`) runs once per engine part per physics step and ends with:

```text
0x1400357c5  48 8b 07           MOV RAX,[RDI]              ; Frame vtable
0x1400357c8  48 8b cf           MOV RCX,RDI                ; RCX = Frame body
0x1400357cb  ff 90 a0 01 00 00  CALL [RAX+0x1A0]           ; Body_AddTorque(T)
```

Register state at `0x1400357c5` (from the earlier register-level logging in `hf_notes`):

| register | meaning |
|---|---|
| `RDI` | the Frame body (the hull) |
| `RBX` | the creature |
| `RSI` | the engine part |
| `XMM1` | `T`, already clamped to `±m_at` |
| `XMM2` | `kp` (0.05, or 0 in the arcade ruleset) |
| `XMM7` | `Tw` after the idle-bank/rules adjustments |
| `XMM8` | `frac = m_at / gate` |
| `XMM10` | `I` (inertia about the Frame origin) |

The vanilla computation is

```text
Tw = -w * I * kd * frac          kd = 0.05
Ta = -a * I * kp * frac          a  = wrap(Frame + 0x138), kp = 0.05
T  = clamp(Tw + Ta, -m_at, +m_at)
```

with two escape hatches that this mod respects:

* if `creature+0x17fc == 0` **or** `creature+0x1800 == 0` (one thrust bank applied no force this
  tick) then `Ta = 0` and `Tw *= 0.5`;
* `a` is the **wrapped world angle**, so the target is absolute, not ship-relative.

## 2. The patch

One 6-byte inline detour at `0x1400357c5`, continuation `0x1400357cb`. The stolen pair
(`MOV RAX,[RDI]` / `MOV RCX,RDI`) is replayed at the head of the trampoline, so the `CALL` still
sees the vtable in `RAX` and the Frame in `RCX`.

```text
stolen bytes                   ; MOV RAX,[RDI] ; MOV RCX,RDI
preserved_call(hook,           ; saves RAX,RCX,RDX,R8..R11 and XMM0..XMM5 into a frame
    MOV RCX,RSP                ;   frame pointer
    MOV RDX,RDI                ;   Frame body
    MOV R8,RBX                 ;   creature
    MOV R9,RSI)                ;   engine part
JMP 0x1400357cb
```

**No stack padding.** `patchy::preserved_call` is 7 pushes + `0x88`, i.e. 192 bytes (
0 mod 16), so it hands the call the alignment it was entered with. At a *function entry* the
game's caller has already pushed a return address (`RSP` = 8 mod 16) and a `SUB RSP,8` is
needed; at this mid-function site the function body holds `RSP` = 0 mod 16 (5 pushes +
`SUB RSP,0xc50`), so padding it would call the Rust hook 8 bytes off the Windows x64
alignment. The trampoline therefore has no `SUB`/`ADD RSP,8` (the 0.1.0 build had one and was
fixed before the play check).

`patchy::preserved_call` restores `XMM1` from frame slot `+0x30` **after** the hook returns, so
writing that slot is how the torque is changed; `XMM2` (`kp`) is read from slot `+0x40`.

Nothing else in the game is written. The mod never touches `Frame + 0x138`, the thrust sums or the
gate.

## 3. The arithmetic

Only the angle term is replaced:

```text
Ta' = -wrap(a - wish) * I * kp * frac
T'  = clamp(T - Ta + Ta', ±m_at)
```

`T - Ta + Ta'` needs `I * kp * frac`, which is rebuilt from the game's own values:

* `kp` — read from the saved `XMM2` (so the arcade rule is inherited, not re-derived);
* `m_at` — `GetMdlAt(part)` (`0x140048720`), the same call the case uses. Its disassembly confirms
  it is `Stats + 0x100` with the index at `part + 0x188`;
* `gate` — `creature + 0x180c`, so `frac = m_at / gate` exactly as vanilla computes it;
* `I` — `Body_GetInertiaAboutOrigin(frame)` (`0x140048440`). Disassembly: `MOV RAX,[RCX+0x178]`,
  then `(mass*|localCenter|² + I_com)` divided by `0.05^5` five times — i.e. exactly the documented
  `(m*|lc|² + I)/0.05^5`;
* `a` — `wrap(Frame + 0x138)` recomputed here with the same fold (round-half-away-from-zero).

Because `a - wrap(a - wish) = wish - 2πk`, the applied delta is

```text
extra torque = I * kp * (m_at/gate) * (wish - 2πk)
```

so no cancellation, and the `±m_at` clamp is re-applied afterwards.

Finally the torque is multiplied by the **frame-time factor** `dt * 50` (`dt` = `time_related_global_`
at `0x1403c126c`, `0.02` at the vanilla 50 fps, so the factor is 1.0 there and 0.5 at 100 fps):
`Body_AddTorque` is applied once per frame and is not multiplied by `dt`, so without this the trim
would be twice as stiff per second at 100 fps. Only the angle term is retargeted; the factor covers
the whole torque, exactly like QOL's anti-tilt fix.

### Who applies the frame-time factor

QOL's own anti-tilt patch (`enable_antitilt`, on by default in `qol.json`) applies the same factor.
It originally hooked the same six bytes as this mod, which made the two mutually exclusive. It now
hooks `Body_AddTorque`'s entry (`0x140019be0`, `PUSH RDI; SUB RSP,0xC0`) and scales `XMM1` only when
`[RSP] == 0x1400357d1` — i.e. only for the stabiliser's own `CALL [RAX+0x1A0]` at `0x1400357cb`.
That call is still the vanilla one after this mod's detour (the trampoline jumps back to it), so
QOL scales the torque *after* this mod has retargeted it, and the factor is applied once.

This mod detects that patch on its first retarget by comparing the nine entry bytes with the vanilla
ones (`mem::add_torque_detoured`). If they differ, QOL is scaling and this mod uses `scale = 1.0`;
otherwise it applies `dt * 50` itself. The decision is logged once.

## 4. The target itself

`src/wish.rs` is a pure state machine, driven once per frame:

* The two wish keys (`Z`/`X` by default, `left_key`/`right_key` in the config) are read once per
  frame with `GetAsyncKeyState` (`src/keys.rs`). They are **not** the game's Q/E: Q/E stay vanilla
  flight controls, and the game's input record only carries its own six held flags anyway, so an
  arbitrary letter has to be polled from the OS. The mod never rewrites the game's input.
* Time comes from the HGE clock (`HGE + 0xd68`), i.e. paused time does not count and the mod never
  uses wall-clock time.
* The hook runs per engine, so the frame is latched by the clock value: the first engine call in a
  frame integrates, the rest get `dt = 0`.
* The target is stored wrapped to `[-π, π]` and is reset whenever the player's creature pointer
  changes.

## 5. What was verified, and how

| claim | evidence |
|---|---|
| site bytes | `48 8b 07 48 8b cf ff 90 a0 01 00 00 e9 …` at RVA `0x357c5` of `Highfleetcracked.exe` (read from the file's `.text` on 2026-11-25) |
| `Profile_IsPlayerShip` is `(class&0x2f)==0x2f && +0xbd4==0` | disassembly at `0x140293920` |
| `GetMdlAt` = `Stats+0x100` via `part+0x188` | disassembly at `0x140048720` |
| `Body_GetInertiaAboutOrigin` = `(m*|lc|²+I)/0.05^5` | disassembly at `0x140048440` |
| frame slots `XMM1`/`XMM2` at `+0x30`/`+0x40` | `patchy-rs` `arch.rs` (`MOVDQU [RSP+0x30],XMM1`, restored after the call) |
| `Body_AddTorque` entry is `PUSH RDI; SUB RSP,0xC0` | read from the file at RVA `0x19be0` on 2026-11-25 |
| the stabiliser's `CALL [RAX+0x1A0]` returns to `0x1400357d1` | call at `0x1400357cb`, 6 bytes |
| frame `dt` global `0x1403c126c` = 0.02 at 50 fps | `hf_notes` (`time_related_global_`) |
| the stabiliser formula | `hf_notes/docs/findings/physics/hull-stabiliser-engine-logic.md` (register logging, ratio 1.00000) |

Verified in play (2026-11-25): the retargeted torque holds the wish on real hulls; the wish keys aim
the expected way; the TWR scaling produces the intended slow/fast split; the panel readout appears
on the player's panel only. Known limitation: statics-heavy hulls can backflip above roughly 4° of
held tilt while translating — see §9.

## 6. Why the wish is not applied on some ticks

`Ta` is dropped when a bank is idle, `kp` is 0 in the arcade ruleset, and `frac` is 0 when the gate
is 0 (no engine of a bank counted, e.g. an engine whose cone is blocked drops out of the gate).
On all of those ticks the mod returns without writing `XMM1`, so behaviour is bit-identical to
vanilla.

## 7. The panel readout

`DrawBattleShipPanel` `0x1402fde80` draws the 350 x 300 ship panel into an offscreen target and ends
with `DrawNoiseHudOverlay(0, 0, 350, 300)` at `0x1402f7f00` (its `CALL` is `0x14030033c`,
`e8 bf 7b ff ff`). That call is still inside the panel's camera with its target bound, so the
readout hooks it:

```text
preserved_call(overlay, MOV RCX,RBP)   ; frame pointer -> panel mode
absolute_call(0x1402f7f00)             ; the noise pass the detour replaced
JMP 0x140300341
```

The panel's third argument is its mode (`2` own ship, left; `3` target, right). Its prologue does
`MOV [RSP+0x58],R8D` and `RSP = RBP - 0x100` in the body, so the hook reads the mode at
`[RBP - 0xA8]` and draws only when it is not 3.

The readout uses the panel's own grid and font: `Font_SetupByName("vcr_30", 0.5f, 2, 0xffffffff)`
`0x1401a8710` (the size is an XMM float; the decompiler mistypes it as a pointer), then
`displayLableRotation_(font, 30.0, 260.0, 0, text, 0, 0, 0.0, 0.0)` `0x14016eb40` - the primitive
that `Text_DrawSingleLineFormat` `0x14016f290` wraps. The fourth argument is the alignment (0 =
left), not the colour; the colour is the font's, set white by the setup call. `(30, 260)` is the
same row as `FSS` on the right and empty on the left (`battle-ship-panels.md`: TADS at (30,15),
counters at x = 270, y = 15/30/45, FUEL/EVAC/FSS at y = 230/245/260).

The text is `TILT %+.1f` in degrees (`TILT -22.5` for a Q-side target). The hook runs before the
noise pass, so the readout gets the same scanlines as the panel's own labels; the panel is
composited one frame later, like everything else in it.

The readout is optional: if the bytes at `0x14030033c` do not match, the install logs a warning and
keeps the stabiliser patch.

## 8. Heavy ships aim slower (TWR scaling)

The target's rate is scaled by the ship's **displayed TWR**, the shipbuilder/strategic scalar
`Creature + 0x160c` (the `TWR_STAT` display reads it; a design stores it as `m_tele_dynamic_map`,
e.g. Sevastopol 1.37 against battle TWR 2.34). `mem::twr` falls back to the battle TWR `+0x1604`,
then to `(left + right) / (mass * 9.82)` recomputed from `+0x1664/+0x1668` and `+0x15b8` (the
stored battle value is 0 when a bank is empty). Sources: `hf_notes`
`findings/ships/ship-profiles.md`.

```text
ratio = ((TWR - twr_zero) / (twr_reference - twr_zero)) ^ twr_exponent
rate  = clamp(base * ratio, min_rate, max_rate)     # magnitudes; base keeps its sign
```

`wish::Settings::rate_for_twr` is pure and unit-tested. At the defaults (reference 3.0, zero 1.25,
exponent 1.5, base 45, clamps 1.0/45.0) a 3.0 TWR ship gets 45 deg/s, 2.5 -> 27.17, 2.0 -> 12.63,
1.5 -> 2.43, and 1.25 and below are on the 1 deg/s floor (a Sevastopol-like 1.37 also lands there).
The exponent makes the drop tunable: 1.0 is linear, 2.0 the square, higher is sharper. Above the
reference the ratio is exactly 1 (no boost). The upper clamp is a **knee shifter** as well as a
guard: with a base above it (e.g. 90 and 45) the output stays flat at 45 until the curve drops
under the clamp and then falls steeply, which is the "interruption" the config exposes. The
stabiliser hook computes it once per frame (the first engine) and feeds the scaled rate into the
state machine; the `TILT` line prints `twr` and `rate`.

## 9. The `m_mdl_at` multiplier

`GetStats` `0x140281e00` returns `StatListBase[i]` from the global `Stats *` vector
`0x143a33bf0..0x143a33bf8` (8-byte entries loaded from `Libraries/OL.seria`), and `m_mdl_at` is
`Stats + 0x100` (`GetMdlAt` `0x140048720`). `Creature_AccumulatePartStats` `0x140071380` calls
`GetStats` too, for the gate `+0x180c` and the per-engine shares, so the same field feeds both the
clamp and the gate.

`mem::scale_mdl_at` multiplies every record's `+0x100` in place once, on the first battle frame
(retried until the vector is readable). That is bit-for-bit the `OL.seria` edit: clamp and gate
scale together, `frac = m_at / gate` is unchanged and only the torque headroom grows. It applies to
every ship and the AI because the records are shared. `m_mdl_at_multiplier: 1.0` skips it; combining
it with an already-edited `OL.seria` multiplies twice, hence the README warning.

In play (2026-11-25) `1.0` reproduced the statics-heavy backflip above roughly 4° of held tilt while
flying with WASD, and `10.0` removed it; the TWR scaling and the readout were confirmed in the same
session.
