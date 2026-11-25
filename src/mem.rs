//! Reading the game's own state from inside the stabiliser hook.
//!
//! Everything that is dereferenced is proved readable first (`readable`), and the three game
//! getters that are called (`GetMdlAt`, `Body_GetInertiaAboutOrigin`, `Profile_IsPlayerShip`) are
//! the same ones the game itself calls with the same arguments.

use std::mem::{size_of, MaybeUninit};

use windows_sys::Win32::System::Memory::{
    VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_GUARD, PAGE_NOACCESS,
};

// Game globals.
pub(crate) const HGE_OBJECT: usize = 0x1_43a1_19e0;
pub(crate) const HGE_CLOCK: usize = 0xd68;

// Frame body.
pub(crate) const FRAME_ANGLE: usize = 0x138;
pub(crate) const FRAME_SPIN: usize = 0x13c;

// Creature.
pub(crate) const CREATURE_BANK_ONE: usize = 0x17fc;
pub(crate) const CREATURE_BANK_TWO: usize = 0x1800;
pub(crate) const CREATURE_GATE: usize = 0x180c;
/// Ship mass, the two bank thrust sums, and the two TWR scalars `Creature::CalculateStats`
/// stores: battle TWR `+0x1604` = `(left + right) / (mass * 9.82)`, and the shipbuilder/strategic
/// TWR `+0x160c` (map thrust, damage-scaled) that the game's TWR display reads.
pub(crate) const CREATURE_MASS: usize = 0x15b8;
pub(crate) const CREATURE_THRUST_LEFT: usize = 0x1664;
pub(crate) const CREATURE_THRUST_RIGHT: usize = 0x1668;
pub(crate) const CREATURE_TWR_BATTLE: usize = 0x1604;
pub(crate) const CREATURE_TWR_STRATEGIC: usize = 0x160c;

// Game functions.
pub(crate) const GET_MDL_AT: usize = 0x1_4004_8720;
pub(crate) const INERTIA_ABOUT_ORIGIN: usize = 0x1_4004_8440;
pub(crate) const GET_PROFILE: usize = 0x1_4029_2a90;
pub(crate) const IS_PLAYER_SHIP: usize = 0x1_4029_3920;

/// The game's per-frame time (`time_related_global_`): `0.02` at the vanilla 50 fps, `0.01` at
/// the 100 fps `CORE_FRAMERATE` the test install uses. The stabiliser applies its torque once per
/// frame and the call is **not** multiplied by this, so the torque has to be scaled by
/// `dt * 50` to mean the same thing per second at any frame rate. QOL's own anti-tilt fix applies
/// exactly this factor, at `ADD_TORQUE_ENTRY`; when that patch is present this mod leaves the
/// factor to it (see [`add_torque_detoured`]). The game itself skips the torque call when the
/// global is 0.
pub(crate) const FRAME_DT: usize = 0x1_403c_126c;

/// `Body_AddTorque` `0x140019be0` entry: `PUSH RDI; SUB RSP,0xC0`. QOL's frame-time scaling
/// detours these nine bytes, so a different first byte means another mod already scales the
/// torque that reaches this call.
pub(crate) const ADD_TORQUE_ENTRY: usize = 0x1_4001_9be0;
pub(crate) const ADD_TORQUE_STOLEN: [u8; 9] = [
    0x40, 0x57, // PUSH RDI
    0x48, 0x81, 0xec, 0xc0, 0x00, 0x00, 0x00, // SUB RSP,0xC0
];

/// The global `Stats *` vector (`StatListBase`/`StatListMax` in `GetStats` `0x140281e00`): 8-byte
/// entries loaded from `Libraries/OL.seria`. `m_mdl_at` is `Stats + 0x100`.
pub(crate) const STAT_LIST_BASE: usize = 0x1_43a3_3bf0;
pub(crate) const STAT_LIST_MAX: usize = 0x1_43a3_3bf8;
pub(crate) const STATS_MDL_AT: usize = 0x100;

const _: () = assert!(HGE_OBJECT == 0x143a119e0);
const _: () = assert!(GET_MDL_AT == 0x140048720);
const _: () = assert!(INERTIA_ABOUT_ORIGIN == 0x140048440);
const _: () = assert!(FRAME_DT == 0x1403c126c);
const _: () = assert!(ADD_TORQUE_ENTRY == 0x140019be0);
const _: () = assert!(STAT_LIST_BASE == 0x143a33bf0);
const _: () = assert!(STAT_LIST_MAX == 0x143a33bf8);

/// True when `size` bytes at `address` are committed, readable and guard-free.
pub(crate) fn readable(address: usize, size: usize) -> bool {
    if address < 0x1_0000 {
        return false;
    }
    let mut information = MaybeUninit::<MEMORY_BASIC_INFORMATION>::uninit();
    // SAFETY: VirtualQuery only inspects an address; the buffer is sized and aligned for the struct.
    let queried = unsafe {
        VirtualQuery(
            address as *const _,
            information.as_mut_ptr(),
            size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    if queried != size_of::<MEMORY_BASIC_INFORMATION>() {
        return false;
    }
    // SAFETY: VirtualQuery returned a complete structure.
    let information = unsafe { information.assume_init() };
    if information.State != MEM_COMMIT
        || information.Protect == PAGE_NOACCESS
        || information.Protect & PAGE_GUARD != 0
    {
        return false;
    }
    let region_offset = address.saturating_sub(information.BaseAddress as usize);
    size <= information.RegionSize.saturating_sub(region_offset)
}

/// Reads a `Copy` value out of the game's memory.
///
/// # Safety
///
/// `address` must be readable for `size_of::<T>()` bytes.
pub(crate) unsafe fn read<T: Copy>(address: usize) -> T {
    // SAFETY: the caller guarantees readability; unaligned because the game's fields are packed.
    unsafe { std::ptr::read_unaligned(address as *const T) }
}

/// The application clock (`HGE + 0xd68`), or `NaN` before the HGE object exists.
///
/// Pausing freezes it, which is exactly what the timers want.
pub(crate) fn clock() -> f32 {
    if !readable(HGE_OBJECT, 8) {
        return f32::NAN;
    }
    // SAFETY: the pointer slot is readable; the object pointer is checked before the field read.
    let hge: usize = unsafe { read(HGE_OBJECT) };
    if hge != 0 && readable(hge + HGE_CLOCK, 4) {
        // SAFETY: the HGE object and the field were just proved readable.
        unsafe { read(hge + HGE_CLOCK) }
    } else {
        f32::NAN
    }
}

/// The game's per-frame time in seconds (`FRAME_DT`), or `None` when it is unreadable or not a
/// positive step (a paused frame). At the vanilla 50 fps this is `0.02`.
pub(crate) fn frame_dt() -> Option<f32> {
    if !readable(FRAME_DT, 4) {
        return None;
    }
    // SAFETY: the global was just proved readable.
    let dt = unsafe { read::<f32>(FRAME_DT) };
    (dt.is_finite() && dt > 0.0).then_some(dt)
}

/// True when another mod has detoured `Body_AddTorque`'s entry, i.e. something is already scaling
/// the stabiliser torque by the frame time (QOL's `enable_antitilt`).
pub(crate) fn add_torque_detoured() -> bool {
    if !readable(ADD_TORQUE_ENTRY, ADD_TORQUE_STOLEN.len()) {
        // The image is always mapped; if this ever fails, scale ourselves rather than not at all.
        return false;
    }
    // SAFETY: the nine bytes were just proved readable.
    let current = unsafe {
        std::slice::from_raw_parts(ADD_TORQUE_ENTRY as *const u8, ADD_TORQUE_STOLEN.len())
    };
    current != ADD_TORQUE_STOLEN
}

/// Multiplies every `Stats` record's `m_mdl_at` (the stabiliser's torque budget) in place, which is
/// exactly what editing `m_mdl_at` in `Libraries/OL.seria` does: the engine case's per-part clamp
/// **and** the stat pass's gate (`+0x180c`, built from these same records) scale together, so the
/// per-engine share stays the same and only the torque headroom grows.
///
/// Returns the number of records changed (0 when the table is not loaded yet).
pub(crate) fn scale_mdl_at(factor: f32) -> usize {
    if !factor.is_finite() || factor <= 0.0 || (factor - 1.0).abs() < f32::EPSILON {
        return 0;
    }
    if !readable(STAT_LIST_BASE, 16) {
        return 0;
    }
    // SAFETY: the vector header was just proved readable.
    let (begin, end) = unsafe { (read::<usize>(STAT_LIST_BASE), read::<usize>(STAT_LIST_MAX)) };
    if begin == 0 || end <= begin {
        return 0;
    }
    let count = (end - begin) / 8;
    if count == 0 || count > 100_000 || !readable(begin, count * 8) {
        return 0;
    }
    let mut changed = 0;
    for index in 0..count {
        // SAFETY: the pointer array was proved readable for `count` entries.
        let stats = unsafe { read::<usize>(begin + index * 8) };
        if stats == 0 || !readable(stats + STATS_MDL_AT, 4) {
            continue;
        }
        // SAFETY: the record's `m_mdl_at` field was just proved readable.
        let current = unsafe { read::<f32>(stats + STATS_MDL_AT) };
        let scaled = current * factor;
        if current != 0.0 && current.is_finite() && scaled.is_finite() {
            // SAFETY: the same field, written with unaligned access.
            unsafe { std::ptr::write_unaligned((stats + STATS_MDL_AT) as *mut f32, scaled) };
            changed += 1;
        }
    }
    changed
}

/// The ship's **displayed** TWR: the shipbuilder/strategic scalar at `Creature + 0x160c`
/// (`m_tele_dynamic_map` in a design, the number the game's `TWR_STAT` display reads), i.e.
/// strategic thrust over weight. Falls back to the battle TWR `+0x1604`, then to
/// `(left + right) / (mass * 9.82)` recomputed from `+0x1664/+0x1668` and `+0x15b8`.
pub(crate) fn twr(creature: usize) -> f32 {
    // SAFETY: the caller proved the creature block readable for `CREATURE_GATE + 4` this frame,
    // which covers every field below.
    let (strategic, battle) = unsafe {
        (
            read::<f32>(creature + CREATURE_TWR_STRATEGIC),
            read::<f32>(creature + CREATURE_TWR_BATTLE),
        )
    };
    if strategic.is_finite() && strategic > 0.0 {
        return strategic;
    }
    if battle.is_finite() && battle > 0.0 {
        return battle;
    }
    let (mass, left, right) = unsafe {
        (
            read::<f32>(creature + CREATURE_MASS),
            read::<f32>(creature + CREATURE_THRUST_LEFT),
            read::<f32>(creature + CREATURE_THRUST_RIGHT),
        )
    };
    if !(mass > 0.0) || !mass.is_finite() || !left.is_finite() || !right.is_finite() {
        return 0.0;
    }
    (left + right) / (mass * 9.82)
}

/// `GetMdlAt(part)` = `Stats + 0x100` (`m_mdl_at`), 0 when the part has no stats record.
///
/// # Safety
///
/// `part` must be a live body pointer (the game's own part list supplies it).
pub(crate) unsafe fn mdl_at(part: usize) -> f32 {
    let get_mdl_at: unsafe extern "C" fn(usize) -> f32 = unsafe { std::mem::transmute(GET_MDL_AT) };
    // SAFETY: the same call the stabiliser itself makes with the same argument.
    unsafe { get_mdl_at(part) }
}

/// True when the creature is the player's ship: the shipped `Profile_IsPlayerShip`
/// (`0x140293920`) is `(class_byte & 0x2f) == 0x2f && creature+0xbd4 == 0`, i.e. a ship body whose
/// `ai_flag` is clear. It dereferences only `+0x28` and `+0xbd4`.
///
/// # Safety
///
/// `creature` must be a live `Creature *` (the stabiliser case's own `RBX`, which it has already
/// checked for null).
pub(crate) unsafe fn is_player_ship(creature: usize) -> bool {
    // SAFETY: both are plain game getters; the creature is the caller's live pointer.
    let get_profile: unsafe extern "C" fn() -> usize = unsafe { std::mem::transmute(GET_PROFILE) };
    let is_player: unsafe extern "C" fn(usize, usize) -> bool =
        unsafe { std::mem::transmute(IS_PLAYER_SHIP) };
    let profile = unsafe { get_profile() };
    // SAFETY: the same call the game itself makes.
    unsafe { is_player(profile, creature) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_are_the_1163_ones() {
        assert_eq!(FRAME_ANGLE, 0x138);
        assert_eq!(FRAME_SPIN, 0x13c);
        assert_eq!(CREATURE_BANK_ONE, 0x17fc);
        assert_eq!(CREATURE_BANK_TWO, 0x1800);
        assert_eq!(CREATURE_GATE, 0x180c);
        assert_eq!(CREATURE_MASS, 0x15b8);
        assert_eq!(CREATURE_THRUST_LEFT, 0x1664);
        assert_eq!(CREATURE_THRUST_RIGHT, 0x1668);
        assert_eq!(CREATURE_TWR_BATTLE, 0x1604);
        assert_eq!(CREATURE_TWR_STRATEGIC, 0x160c);
    }

    #[test]
    fn a_null_or_tiny_address_is_never_readable() {
        assert!(!readable(0, 4));
        assert!(!readable(0x1000, 4));
    }
}

