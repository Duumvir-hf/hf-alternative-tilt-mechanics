//! The engine stabiliser hook: swap the "pull towards 0" angle term for "pull towards the wish".
//!
//! ## The site
//!
//! `creatureLogic` case 200 (`hull-stabiliser-engine-logic.md`) runs once per engine part per
//! physics step and ends with:
//!
//! ```text
//! 0x1400357c5  48 8b 07           MOV RAX,[RDI]          ; the Frame's vtable
//! 0x1400357c8  48 8b cf           MOV RCX,RDI            ; RCX = the Frame body
//! 0x1400357cb  ff 90 a0 01 00 00  CALL [RAX+0x1A0]       ; Body_AddTorque(T)
//! ```
//!
//! with `XMM1` = the clamped torque `T = clamp(Tw + Ta, ±m_at)`, `XMM2` = `kp`, `RDI` = the Frame,
//! `RBX` = the creature and `RSI` = the engine part. Those are the two instructions the detour
//! steals (6 bytes, no branch, no RIP-relative operand, so they relocate unchanged); the hook runs
//! between them and the call.
//!
//! ## The arithmetic
//!
//! Vanilla: `Tw = -w*I*kd*frac`, `Ta = -a*I*kp*frac`, `T = clamp(Tw + Ta, ±m_at)`, where
//! `a = wrap(Frame.angle)`. Only the angle term changes:
//!
//! ```text
//! T' = clamp(T - Ta + Ta', ±m_at)        Ta' = -wrap(a - wish)*I*kp*frac
//! ```
//!
//! `Ta' - Ta` needs `I*kp*frac`, so the hook calls the game's own `GetMdlAt` and
//! `Body_GetInertiaAboutOrigin`, and reads `kp` out of the preserved `XMM2` slot. Everything the
//! game uses is reused; nothing is re-derived from constants.
//!
//! Vanilla drops `Ta` entirely on a tick where one of the two thrust banks applied no force
//! (`creature+0x17fc` / `+0x1800` are zero), and in the arcade ruleset `kp` is 0. Both ticks are
//! left exactly as vanilla made them: there is no angle term to retarget.

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};

use patchy::{ReturnType, Trampoline};

use crate::{config, keys, mem, wish};

/// `creatureLogic` case 200, immediately before `Body_AddTorque`.
pub(crate) const SITE: usize = 0x1_4003_57c5;
pub(crate) const STOLEN: [u8; 6] = [0x48, 0x8b, 0x07, 0x48, 0x8b, 0xcf];
pub(crate) const CONTINUATION: usize = 0x1_4003_57cb;

const _: () = assert!(SITE == 0x1400357c5);
const _: () = assert!(CONTINUATION == 0x1400357cb);

/// `patchy::preserved_call` frame slots (see `arch.rs`): `XMM0..XMM5` live at `0x20..0x70`.
const FRAME_XMM1: usize = 0x30;
const FRAME_XMM2: usize = 0x40;

static APPLIED: AtomicU64 = AtomicU64::new(0);

/// The `m_mdl_at` multiplier is applied to the shared `Stats` records once, on the first call of
/// the first battle (the table is loaded by then).
static MDL_AT_DONE: AtomicBool = AtomicBool::new(false);

/// Multiplies `Stats.m_mdl_at` in place, which is exactly what editing `Libraries/OL.seria` does:
/// the per-engine clamp and the gate both scale, so the per-engine share is unchanged and only the
/// torque budget grows. Retried each frame until the table is readable.
fn apply_mdl_at_multiplier() {
    if MDL_AT_DONE.load(Ordering::Relaxed) {
        return;
    }
    let factor = config::mdl_at_multiplier();
    if (factor - 1.0).abs() < f32::EPSILON {
        MDL_AT_DONE.store(true, Ordering::Relaxed);
        return;
    }
    let changed = mem::scale_mdl_at(factor);
    if changed == 0 {
        return;
    }
    MDL_AT_DONE.store(true, Ordering::Relaxed);
    log::info!(
        "hf_wish_tilt: m_mdl_at x{factor} applied to {changed} stats records (the same as \
         editing Libraries/OL.seria); every ship and the AI see it"
    );
}

/// Logged at most once: an engine whose `m_mdl_at` did not resolve.
static WARNED_PART: Mutex<usize> = Mutex::new(0);

#[derive(Clone, Copy)]
struct Session {
    /// The clock value the per-frame work was last done for.
    clock: f32,
    /// Time since the previous frame; 0 for the engines after the first in the same frame.
    dt: f32,
    keys: wish::Keys,
    /// The creature this session belongs to (the player's ship).
    player: usize,
    frame_body: usize,
    /// The creature and the Frame were proved readable for this frame.
    guarded: bool,
    wish: wish::Wish,
    /// The target the last verbose line reported, so a line only prints when it moved.
    logged: f32,
}

impl Session {
    const fn new() -> Self {
        Self {
            clock: f32::NAN,
            dt: 0.0,
            keys: wish::Keys::NONE,
            player: 0,
            frame_body: 0,
            guarded: false,
            wish: wish::Wish::new(),
            logged: 0.0,
        }
    }

    /// The frame's step, handed to the **first** engine of the player's ship and zero afterwards.
    /// `wish::Wish::step` treats a zero step as "same frame, do not integrate", so every other
    /// engine of the frame reuses the target instead of advancing it again.
    fn take_dt(&mut self) -> f32 {
        std::mem::take(&mut self.dt)
    }
}

static SESSION: Mutex<Session> = Mutex::new(Session::new());

/// Replays the stolen instructions, calls the hook, then continues into `Body_AddTorque`.
///
/// The hook's arguments are the preserved-call frame in `RCX` (set by the trampoline), then
/// `RDX = RDI` (Frame body), `R8 = RBX` (creature) and `R9 = RSI` (engine part). All three are
/// volatile, so the setup is allowed to overwrite them; `patchy` restores them afterwards.
pub(crate) fn trampoline() -> Trampoline {
    let mut trampoline = Trampoline::new();
    trampoline.bytes(&STOLEN);
    // No stack padding: `patchy`'s preserved call (7 pushes + 0x88 = 192 bytes, 0 mod 16) keeps
    // the incoming alignment, and this is a mid-function site where the game's own body holds
    // RSP 0 mod 16 (5 pushes + SUB RSP,0xc50). Adding `SUB RSP,8` here - as a function-entry hook
    // needs - would call the Rust hook 8 bytes off alignment.
    trampoline.preserved_call(
        retarget_hook as *const (),
        &[
            0x48, 0x89, 0xe1, // MOV RCX,RSP   (frame)
            0x48, 0x89, 0xfa, // MOV RDX,RDI   (Frame body)
            0x49, 0x89, 0xd8, // MOV R8,RBX    (creature)
            0x49, 0x89, 0xf1, // MOV R9,RSI    (engine part)
        ],
        &[],
        ReturnType::None,
    );
    trampoline.relative_jump(CONTINUATION);
    trampoline
}

fn frame_f32(frame: *const u8, offset: usize) -> f32 {
    // SAFETY: the frame is the preserved-call frame the trampoline built.
    unsafe { std::ptr::read_unaligned(frame.add(offset) as *const f32) }
}

unsafe extern "C" fn retarget_hook(
    frame: *const u8,
    frame_body: usize,
    creature: usize,
    part: usize,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the trampoline supplies the live register values of the case.
        unsafe { retarget(frame, frame_body, creature, part) }
    }));
}

/// # Safety
///
/// `frame_body` is the `RDI` the stabiliser case is using, `creature` its `RBX` and `part` its
/// `RSI`; all three are live game objects.
unsafe fn retarget(frame: *const u8, frame_body: usize, creature: usize, part: usize) {
    if frame.is_null() || frame_body == 0 || creature == 0 || part == 0 {
        return;
    }
    let now = mem::clock();
    if !now.is_finite() {
        return;
    }

    let mut session = match SESSION.lock() {
        Ok(session) => session,
        Err(poisoned) => poisoned.into_inner(),
    };

    apply_mdl_at_multiplier();

    // Who is this? Only the local player gets a wish tilt; AI ships keep vanilla's level-seeking.
    // The test is the game's own `Profile_IsPlayerShip`: a class-byte compare plus `ai_flag == 0`.
    //
    // This has to happen *before* the frame latch below. The game runs this same case for every
    // creature in one loop per frame, so if an AI ship's engine is visited before the player's,
    // latching the clock on that call would leave the player's own engine with `dt = 0` for the
    // whole frame - and then the wish would never integrate in a battle.
    if creature != session.player {
        // SAFETY: `creature` is the case's own creature pointer (a live Creature).
        if !unsafe { mem::is_player_ship(creature) } {
            return;
        }
        session.player = creature;
        session.frame_body = frame_body;
        session.wish.reset(creature);
        session.logged = 0.0;
        session.guarded = false;
        // A new hull starts on a fresh frame: never take the previous ship's frame gap.
        session.clock = f32::NAN;
        log::info!("hf_wish_tilt: player ship at {creature:#x}; the wish keys move the wish tilt");
    } else if frame_body != session.frame_body {
        session.frame_body = frame_body;
        session.guarded = false;
    }

    // One pass of the per-frame work for the player's own ship, then every later engine of the
    // same frame reuses it.
    let fresh = now != session.clock;
    if fresh {
        let previous = session.clock;
        session.clock = now;
        session.dt = if previous.is_finite() {
            (now - previous).clamp(0.0, 0.1)
        } else {
            0.0
        };
        let (left, right) = config::key_codes();
        session.keys = wish::Keys::new(keys::down(left), keys::down(right));
        session.guarded = false;
    }

    if !session.guarded {
        session.guarded = mem::readable(creature, mem::CREATURE_GATE + 4)
            && mem::readable(frame_body, mem::FRAME_SPIN + 4);
        if !session.guarded {
            return;
        }
    }

    let settings = config::settings();
    // Battle TWR scales the target's movement rate: a heavy ship aims its wish slower than a light
    // one, and above the reference TWR the rate is unchanged (see `wish::Settings::rate_for_twr`).
    // SAFETY: the creature block was proved readable for `CREATURE_GATE + 4` above, which covers
    // the mass and both thrust sums.
    let twr = mem::twr(creature);
    let rate = settings.rate_for_twr(twr);
    let settings = wish::Settings {
        radians_per_second: rate,
        ..settings
    };
    let dt = session.take_dt();
    let keys = session.keys;
    let wish = session.wish.step(creature, dt, keys, &settings);
    if !wish.is_finite() || wish == 0.0 {
        return;
    }

    // SAFETY: the creature block was proved readable this frame.
    let (gate, bank_one, bank_two) = unsafe {
        (
            mem::read::<f32>(creature + mem::CREATURE_GATE),
            mem::read::<f32>(creature + mem::CREATURE_BANK_ONE),
            mem::read::<f32>(creature + mem::CREATURE_BANK_TWO),
        )
    };
    // Vanilla drops the angle term on a tick where a bank applied no force, and in the arcade
    // ruleset `kp` is 0. Nothing to retarget there.
    if !(gate > 0.0) || bank_one == 0.0 || bank_two == 0.0 {
        return;
    }
    // SAFETY: `part` is the case's own engine part.
    let m_at = unsafe { mem::mdl_at(part) };
    if !(m_at > 0.0) || !m_at.is_finite() {
        if let Ok(mut warned) = WARNED_PART.lock() {
            if *warned != part {
                *warned = part;
                log::warn!(
                    "hf_wish_tilt: no m_mdl_at for engine part {part:#x}; its share stays vanilla"
                );
            }
        }
        return;
    }
    let kp = frame_f32(frame, FRAME_XMM2);
    if !(kp > 0.0) || !kp.is_finite() {
        return;
    }
    let inertia = inertia_about_origin(frame_body);
    if !(inertia > 0.0) || !inertia.is_finite() {
        return;
    }
    // SAFETY: the Frame was proved readable this frame.
    let raw = unsafe { mem::read::<f32>(frame_body + mem::FRAME_ANGLE) };
    let old = frame_f32(frame, FRAME_XMM1);
    if !raw.is_finite() || !old.is_finite() {
        return;
    }

    let hull = wish::wrap_pi(raw);
    let stiffness = inertia * kp * (m_at / gate);
    let delta = stiffness * (hull - wish::wrap_pi(hull - wish));
    // The vanilla torque is applied once per frame and not multiplied by the frame time, so at
    // 100 fps it works twice as hard per second. The factor (`dt * 50`, 1.0 at 50 fps) is applied
    // exactly once for the whole torque: QOL's anti-tilt fix watches for it at the torque call,
    // and when that patch is installed this mod hands the factor over instead of doubling it.
    let scale = if own_scaling_needed() {
        match frame_scale(mem::frame_dt()) {
            Some(scale) => scale,
            None => return,
        }
    } else {
        1.0
    };
    let torque = (old + delta).clamp(-m_at, m_at) * scale;
    if !torque.is_finite() {
        return;
    }
    // SAFETY: the slot is inside the preserved-call frame; writing it changes the restored XMM1.
    unsafe { std::ptr::write_unaligned(frame.add(FRAME_XMM1) as *mut f32, torque) };

    if config::get().verbose_logging {
        let applied = APPLIED.fetch_add(1, Ordering::Relaxed) + 1;
        if applied == 1 || (wish - session.logged).abs() >= 0.5_f32.to_radians() {
            session.logged = wish;
            log::info!(
                "TILT t={now:.3} wish={:.1}deg hull={:.1}deg l={} r={} banks={}/{} \
                 torque {:.3e} -> {:.3e} (m_at {:.3e}, x{scale:.2}, twr {twr:.2}, \
                 rate {:.1}deg/s)",
                wish.to_degrees(),
                hull.to_degrees(),
                u8::from(session.keys.left),
                u8::from(session.keys.right),
                u8::from(bank_one != 0.0),
                u8::from(bank_two != 0.0),
                old,
                torque,
                m_at,
                rate.to_degrees()
            );
        }
    }
}

/// `dt * 50`, the factor that turns a once-per-frame torque into a per-second one: `1.0` at the
/// vanilla 50 fps, `0.5` at 100 fps. `None` when the game reports no step.
fn frame_scale(dt: Option<f32>) -> Option<f32> {
    let scale = dt? * 50.0;
    (scale.is_finite() && scale > 0.0).then_some(scale)
}

/// `0` = not decided yet, `1` = this mod applies `frame_scale`, `2` = another mod does.
static SCALING: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Whether this mod has to apply the frame-time factor itself. QOL's anti-tilt fix detours the
/// torque call's entry, which the game reaches from the stabiliser at `0x1400357d1`; if that detour
/// is present it scales the torque *after* this hook has retargeted it, so the factor must not be
/// applied here as well.
fn own_scaling_needed() -> bool {
    match SCALING.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let needed = !mem::add_torque_detoured();
            SCALING.store(if needed { 1 } else { 2 }, Ordering::Relaxed);
            if needed {
                log::info!(
                    "hf_wish_tilt: no other frame-time scaling at {:#x}; this mod applies dt*50",
                    mem::ADD_TORQUE_ENTRY
                );
            } else {
                log::info!(
                    "hf_wish_tilt: {:#x} is already detoured (QOL anti-tilt scaling); the \
                     frame-time factor stays with that mod",
                    mem::ADD_TORQUE_ENTRY
                );
            }
            needed
        }
    }
}

/// The target the stabiliser is aiming at, in world radians (`0.0` = level). Read by the panel
/// readout in [`crate::hud`]; the hook updates it once per frame.
pub(crate) fn current_wish() -> f32 {
    match SESSION.lock() {
        Ok(session) => session.wish.angle(),
        Err(poisoned) => poisoned.into_inner().wish.angle(),
    }
}

/// `Body_GetInertiaAboutOrigin(frame)` in game units, 0 when the body has no b2 body.
fn inertia_about_origin(frame_body: usize) -> f32 {
    let get: unsafe extern "C" fn(usize) -> f32 =
        unsafe { std::mem::transmute(mem::INERTIA_ABOUT_ORIGIN) };
    // SAFETY: the same call `creatureLogic` case 200 itself makes on the same body.
    unsafe { get(frame_body) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_site_is_the_stabiliser_call() {
        assert_eq!(SITE, 0x1400357c5);
        assert_eq!(CONTINUATION, 0x1400357cb);
        assert_eq!(CONTINUATION, SITE + STOLEN.len());
    }

    #[test]
    fn the_stolen_bytes_are_whole_instructions() {
        // MOV RAX,[RDI] ; MOV RCX,RDI
        assert_eq!(STOLEN, [0x48, 0x8b, 0x07, 0x48, 0x8b, 0xcf]);
    }

    #[test]
    fn the_trampoline_replays_the_stolen_bytes_and_stays_aligned() {
        const BASE: usize = 0x1_4004_0000;
        let code = trampoline().build(BASE).expect("builds");
        assert!(code.starts_with(&STOLEN));
        // `patchy`'s preserved call preserves the caller's alignment (192 bytes, 0 mod 16) and
        // this site is already 16-byte aligned, so any SUB RSP,8 / ADD RSP,8 pair here would put
        // the Rust hook 8 bytes off the Windows x64 call alignment.
        assert_eq!(
            code.windows(4)
                .filter(|window| *window == [0x48, 0x83, 0xec, 0x08])
                .count(),
            0,
            "no SUB RSP,8 at a mid-function site"
        );
        assert_eq!(
            code.windows(4)
                .filter(|window| *window == [0x48, 0x83, 0xc4, 0x08])
                .count(),
            0,
            "no matching ADD RSP,8"
        );
        assert!(code.windows(3).any(|window| window == [0x48, 0x89, 0xe1]));
        assert!(code.windows(3).any(|window| window == [0x48, 0x89, 0xfa]));
        assert!(code.windows(3).any(|window| window == [0x49, 0x89, 0xd8]));
        assert!(code.windows(3).any(|window| window == [0x49, 0x89, 0xf1]));

        let jump_at = code.len() - 5;
        assert_eq!(code[jump_at], 0xe9, "the trampoline ends in a relative jump");
        let displacement = i32::from_le_bytes(code[jump_at + 1..].try_into().unwrap());
        let next = (BASE + jump_at + 5) as isize;
        assert_eq!(next + displacement as isize, CONTINUATION as isize);
    }

    #[test]
    fn the_frame_step_is_handed_to_the_first_engine_only() {
        let mut session = Session::new();
        session.dt = 0.01;
        assert_eq!(session.take_dt(), 0.01);
        // Every other engine of the same frame must not move the target again.
        assert_eq!(session.take_dt(), 0.0);
        assert_eq!(session.take_dt(), 0.0);
    }

    #[test]
    fn the_frame_time_factor_is_one_at_fifty_fps() {
        assert_eq!(frame_scale(Some(0.02)), Some(1.0));
        assert_eq!(frame_scale(Some(0.01)), Some(0.5));
        assert_eq!(frame_scale(Some(0.04)), Some(2.0));
        assert_eq!(frame_scale(Some(0.0)), None, "a paused frame is not a step");
        assert_eq!(frame_scale(Some(-0.02)), None);
        assert_eq!(frame_scale(Some(f32::NAN)), None);
        assert_eq!(frame_scale(None), None, "an unreadable step must not break the hook");
    }

    #[test]
    fn the_torque_entry_bytes_are_the_ones_qol_detours() {
        // PUSH RDI ; SUB RSP,0xC0 - read from the 1.163 image at 0x140019be0.
        assert_eq!(mem::ADD_TORQUE_ENTRY, 0x140019be0);
        assert_eq!(
            mem::ADD_TORQUE_STOLEN,
            [0x40, 0x57, 0x48, 0x81, 0xec, 0xc0, 0x00, 0x00, 0x00]
        );
    }
}
