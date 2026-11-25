//! The wish-tilt readout in the bottom-left corner of the player's battle ship panel.
//!
//! `DrawBattleShipPanel` (`0x1402fde80`) renders the 350 x 300 panel into an offscreen target:
//! TADS timer top-left, ASM/AAM/GPB top-right, FUEL/EVAC/FSS bottom-right, and nothing in the
//! bottom-left (`hf_notes/docs/rendering/battle-ship-panels.md`). It ends with
//! `DrawNoiseHudOverlay(0, 0, 350, 300)` (`CALL` at `0x14030033c`), still inside the panel's
//! camera with its target bound. The detour draws one extra line there, before the noise pass, so
//! it uses the panel's font, pixels and scanline treatment for free.
//!
//! The panel mode is the third argument (`2` = the player's own ship, left; `3` = the target ship,
//! right). Its prologue stores `R8D` at `[RBP - 0xA8]`, which is where the readout checks it: the
//! enemy panel must not show the player's wish tilt.

use std::ffi::{c_char, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use patchy::{ReturnType, Trampoline};

use crate::stabiliser;

/// `CALL DrawNoiseHudOverlay` inside `DrawBattleShipPanel`: 5 bytes, no RIP-relative operand.
pub(crate) const SITE: usize = 0x1_4030_033c;
pub(crate) const STOLEN: [u8; 5] = [0xe8, 0xbf, 0x7b, 0xff, 0xff];
pub(crate) const CONTINUATION: usize = 0x1_4030_0341;

const NOISE: usize = 0x1_402f_7f00;
const FONT_SETUP: usize = 0x1_401a_8710;
const DRAW_LABEL: usize = 0x1_4016_eb40;

/// `DrawBattleShipPanel`'s `param_3` (mode) home, relative to its `RBP`; `3` = target panel.
const MODE_AT_RBP: usize = 0xa8;
const TARGET_PANEL: i32 = 3;

/// The panel's own text grid: `TADS` at (30,15), counters at x=270, bottom labels at y=260.
const X: f32 = 30.0;
const Y: f32 = 260.0;
const FONT: &[u8] = b"vcr_30\0";
const FONT_SCALE: f32 = 0.5;
const WHITE: u32 = 0xffff_ffff;

type SetupFont = unsafe extern "C" fn(*const c_char, f32, i32, u32) -> usize;
type DrawLabel =
    unsafe extern "C" fn(usize, f32, f32, u32, *const u8, usize, usize, f32, f32) -> i32;

/// Draws the readout, then repeats the `DrawNoiseHudOverlay` call the detour replaced.
pub(crate) fn trampoline() -> Trampoline {
    let mut trampoline = Trampoline::new();
    // MOV RCX,RBP - the panel's frame, where the mode lives.
    trampoline.preserved_call(overlay as *const (), &[0x48, 0x8b, 0xcd], &[], ReturnType::None);
    trampoline.absolute_call(NOISE as *const ());
    trampoline.relative_jump(CONTINUATION);
    trampoline
}

unsafe extern "C" fn overlay(rbp: usize) {
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe { draw(rbp) }));
}

/// # Safety
///
/// `rbp` is the `RBP` of the live `DrawBattleShipPanel` invocation.
unsafe fn draw(rbp: usize) {
    if rbp < 0x1_0000 {
        return;
    }
    // SAFETY: the panel's prologue stored its mode argument there and it is still live.
    let mode = unsafe { std::ptr::read_unaligned((rbp - MODE_AT_RBP) as *const i32) };
    if mode == TARGET_PANEL {
        return;
    }
    let Ok(text) = CString::new(label(stabiliser::current_wish())) else {
        return;
    };
    let setup: SetupFont = unsafe { std::mem::transmute(FONT_SETUP) };
    // SAFETY: the call the panel itself makes, with the same font name, scale and white colour.
    let font = unsafe { setup(FONT.as_ptr() as *const c_char, FONT_SCALE, 2, WHITE) };
    if font == 0 {
        return;
    }
    let draw: DrawLabel = unsafe { std::mem::transmute(DRAW_LABEL) };
    // SAFETY: the panel's target and camera are still bound, and `text` is NUL-terminated ASCII.
    unsafe { draw(font, X, Y, 0, text.as_ptr() as *const u8, 0, 0, 0.0, 0.0) };
}

/// `TILT -22.5` - the target in degrees, signed (Q aims negative). One line, ASCII only: the
/// `vcr_30` font has no degree sign.
fn label(radians: f32) -> String {
    let degrees = radians.to_degrees();
    if !degrees.is_finite() || degrees.abs() < 0.05 {
        return "TILT   0.0".to_string();
    }
    format!("TILT {degrees:+.1}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_site_is_the_panel_noise_call() {
        assert_eq!(SITE, 0x14030033c);
        assert_eq!(CONTINUATION, SITE + STOLEN.len());
        assert_eq!(STOLEN, [0xe8, 0xbf, 0x7b, 0xff, 0xff]);
    }

    #[test]
    fn the_trampoline_draws_then_repeats_the_noise_call() {
        const BASE: usize = 0x1_4004_0000;
        let code = trampoline().build(BASE).expect("builds");
        assert!(code.windows(3).any(|window| window == [0x48, 0x8b, 0xcd]), "MOV RCX,RBP");
        let noise = (NOISE as u64).to_le_bytes();
        assert!(code.windows(8).any(|window| window == noise), "absolute call to the noise pass");
        // The detour replaces a mid-function CALL: RSP is 0 mod 16 there, so no stack padding.
        assert!(!code.windows(4).any(|window| window == [0x48, 0x83, 0xec, 0x08]));

        let jump_at = code.len() - 5;
        assert_eq!(code[jump_at], 0xe9);
        let displacement = i32::from_le_bytes(code[jump_at + 1..].try_into().unwrap());
        let next = (BASE + jump_at + 5) as isize;
        assert_eq!(next + displacement as isize, CONTINUATION as isize);
    }

    #[test]
    fn the_label_is_signed_degrees() {
        assert_eq!(label(0.0), "TILT   0.0");
        assert_eq!(label((-22.5_f32).to_radians()), "TILT -22.5");
        assert_eq!(label((45.0_f32).to_radians()), "TILT +45.0");
        assert_eq!(label(3.0), "TILT +171.9", "wrapped values are printed as stored");
        assert_eq!(label(f32::NAN), "TILT   0.0");
    }
}
