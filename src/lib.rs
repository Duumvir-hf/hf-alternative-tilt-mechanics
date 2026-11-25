//! HighFleet Steam 1.163 - **wish tilt**.
//!
//! The engines' own `m_mdl_at` stabiliser normally pulls the hull back towards 0° (level) whenever
//! it thrusts. This mod replaces that 0° with a persistent **wish tilt** you aim with its own keys
//! (Q/E are left alone - they still fly the ship exactly as vanilla):
//!
//! * Hold the left / right key (`Z` / `X` by default) - the target drifts at `degrees_per_second`.
//! * Hold both for `level_hold_seconds` - the target snaps back to level. Releasing either key then
//!   starts `level_debounce_seconds` in which the keys are ignored, so letting go of the combo
//!   cannot nudge the target.
//! * AI ships are untouched (they never press the keys).
//!
//! One patch: a 6-byte detour inside `creatureLogic` case 200 (the hull stabiliser), immediately
//! before its `Body_AddTorque` call, plus a readout in the ship panel. See `stabiliser.rs` for the
//! site, the register map and the arithmetic, `hud.rs` for the readout, and `docs/DESIGN.md` for
//! the engine findings this rests on.
//!
//! Config: `Modloader/config/wish_tilt.json`.

highfleet_mod_api::export_logger!();

mod config;
mod hud;
mod keys;
mod mem;
mod stabiliser;
mod wish;

use std::sync::atomic::{AtomicBool, Ordering};

use patchy::PatchSession;

const IMAGE_BASE: usize = 0x1_4000_0000;
const _: () = assert!(IMAGE_BASE == 0x140000000);

static SUPPORTED_VERSION: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Waits for `bytes` to appear at `address` (the Steam build unpacks itself during loader start).
fn wait_for(address: usize, bytes: &[u8]) -> Result<(), String> {
    let mut problem = String::from("not readable");
    for attempt in 0..=20 {
        if mem::readable(address, bytes.len()) {
            // SAFETY: just proved readable for exactly this length.
            let current = unsafe { std::slice::from_raw_parts(address as *const u8, bytes.len()) };
            if current == bytes {
                if attempt > 0 {
                    log::info!("hf_wish_tilt: {address:#x} matched after {attempt} retr(ies)");
                }
                return Ok(());
            }
            problem = format!("bytes {current:02x?} differ from the Steam 1.163 build");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(format!("{address:#x}: {problem}; not patched"))
}

unsafe fn install() -> Result<(), String> {
    wait_for(stabiliser::SITE, &stabiliser::STOLEN)?;

    let mut session = PatchSession::new();
    // SAFETY: the six bytes were verified above.
    match unsafe {
        session.detour_trampoline(
            stabiliser::SITE,
            stabiliser::STOLEN.len(),
            stabiliser::trampoline(),
        )
    } {
        Ok(_) => log::info!(
            "hf_wish_tilt: hull stabiliser {:#x} retargeted (resumes at {:#x})",
            stabiliser::SITE,
            stabiliser::CONTINUATION
        ),
        Err(error) => return Err(format!("hook not prepared: {error}")),
    }

    // The panel readout is a bonus: another mod owning that call must not cost the wish tilt.
    match wait_for(hud::SITE, &hud::STOLEN) {
        Ok(()) => {
            // SAFETY: the five bytes were verified above.
            match unsafe {
                session.detour_trampoline(hud::SITE, hud::STOLEN.len(), hud::trampoline())
            } {
                Ok(_) => log::info!(
                    "hf_wish_tilt: wish tilt readout hooked at {:#x} (resumes at {:#x})",
                    hud::SITE,
                    hud::CONTINUATION
                ),
                Err(error) => log::warn!("hf_wish_tilt: readout not prepared: {error}"),
            }
        }
        Err(error) => log::warn!(
            "hf_wish_tilt: readout not installed ({error}); the stabiliser still works"
        ),
    }

    // SAFETY: the sites were verified above; the session installs as one batch.
    match unsafe { session.install_permanently() } {
        Ok(()) => Ok(()),
        Err(error) => Err(format!("not installed: {error}")),
    }
}

#[no_mangle]
unsafe extern "C" fn version(version: *const std::ffi::c_char) -> bool {
    // SAFETY: the loader passes a live NUL-terminated version string.
    let supported = !version.is_null()
        && unsafe { std::ffi::CStr::from_ptr(version) }.to_bytes() == b"Steam 1.163";
    SUPPORTED_VERSION.store(supported, Ordering::Relaxed);
    supported
}

#[no_mangle]
unsafe extern "C" fn init() -> bool {
    // SAFETY: a null module name asks for the main module.
    let image_base =
        unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(std::ptr::null()) }
            as usize;
    let supported = SUPPORTED_VERSION.load(Ordering::Relaxed);
    log::info!(
        "hf_wish_tilt v{}: image base {image_base:#x}, version {}",
        env!("CARGO_PKG_VERSION"),
        if supported { "Steam 1.163" } else { "unsupported" }
    );
    if !supported {
        log::error!("hf_wish_tilt: needs Steam 1.163; not started");
        return false;
    }
    if image_base != IMAGE_BASE {
        log::error!(
            "hf_wish_tilt: unexpected image base {image_base:#x} (want {IMAGE_BASE:#x}); not started"
        );
        return false;
    }

    let config = config::load();
    for (key, _) in &config.unknown {
        log::warn!(
            "hf_wish_tilt: ignoring unknown config key '{key}' - valid keys: enabled, \
             degrees_per_second, left_key, right_key, invert, twr_reference, twr_zero, \
             twr_exponent, twr_min_degrees_per_second, twr_max_degrees_per_second, \
             m_mdl_at_multiplier, level_hold_seconds, level_debounce_seconds, verbose_logging"
        );
    }
    if !config.enabled {
        log::info!("hf_wish_tilt: disabled in {}", config::PATH);
        return true;
    }
    if INSTALLED.load(Ordering::Relaxed) {
        return true;
    }
    log::info!(
        "hf_wish_tilt: {:.1} deg/s, keys {} / {}, invert={}, level hold {:.2} s, \
         debounce {:.2} s, m_mdl_at x{}, verbose={}",
        config.degrees_per_second,
        config.left_key,
        config.right_key,
        config.invert,
        config.level_hold_seconds,
        config.level_debounce_seconds,
        config.m_mdl_at_multiplier,
        config.verbose_logging
    );
    config::install(config);

    match unsafe { install() } {
        Ok(()) => {
            INSTALLED.store(true, Ordering::Relaxed);
            log::info!("hf_wish_tilt: installed; the wish keys now aim the engine stabiliser");
            true
        }
        Err(error) => {
            log::error!("hf_wish_tilt: not installed: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_base_is_the_1163_one() {
        assert_eq!(IMAGE_BASE, 0x140000000);
    }

    #[test]
    fn the_patch_site_is_the_stabiliser_call() {
        assert_eq!(stabiliser::SITE, 0x1_4003_57c5);
        assert_eq!(stabiliser::STOLEN.len(), 6);
    }
}
