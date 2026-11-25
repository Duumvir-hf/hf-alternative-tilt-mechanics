//! `Modloader/config/wish_tilt.json` - the four numbers this mod runs on.
//!
//! Values are taken as written (a negative `degrees_per_second` simply swaps the two keys, like
//! `invert` does); only a non-finite number falls back to its default, because NaN would silently
//! freeze the target.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::{keys, wish};

pub(crate) const PATH: &str = "Modloader/config/wish_tilt.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub enabled: bool,
    /// How fast the wish tilt drifts while a key is held, in degrees per second (at or above
    /// `twr_reference`).
    pub degrees_per_second: f32,
    /// Key that aims the wish left (default `Z`). Q/E are left alone for flying the ship.
    pub left_key: String,
    /// Key that aims the wish right (default `X`).
    pub right_key: String,
    /// Swap which key aims which way (also use it if the two keys feel mirrored).
    pub invert: bool,
    /// Battle thrust-to-weight ratio at which the full `degrees_per_second` applies (default 3.0).
    /// Below it the rate follows a square down to zero at `twr_zero`; above it is not boosted.
    pub twr_reference: f32,
    /// Battle TWR at or below which the curve reaches zero (default 1.25). Raise it to be harsher
    /// on heavy ships.
    pub twr_zero: f32,
    /// Exponent of the curve between `twr_zero` and `twr_reference` (default 1.5):
    /// `1.0` = linear, `2.0` = square, higher = a sharper drop.
    pub twr_exponent: f32,
    /// Slowest the wish may move while a key is held, degrees per second (default 1.0).
    pub twr_min_degrees_per_second: f32,
    /// Fastest the wish may move, degrees per second (default 45.0).
    pub twr_max_degrees_per_second: f32,
    /// Multiplies every engine's `m_mdl_at` (the stabiliser's torque budget) at load, exactly like
    /// editing `Libraries/OL.seria`. `1.0` leaves the game alone; `10.0` is the tested value that
    /// stops statics-heavy hulls from flipping. Affects **all** ships, AI included.
    pub m_mdl_at_multiplier: f32,
    /// How long both keys must be held together before the target snaps back to level (0°).
    pub level_hold_seconds: f32,
    /// How long the keys are ignored after a level, so releasing the combo cannot nudge it.
    pub level_debounce_seconds: f32,
    /// One TILT line per 0.5° of target movement.
    pub verbose_logging: bool,
    /// Keys the mod does not know.
    #[serde(flatten)]
    pub unknown: std::collections::HashMap<String, serde_json::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            degrees_per_second: 45.0,
            left_key: "Z".to_string(),
            right_key: "X".to_string(),
            invert: false,
            twr_reference: 3.0,
            twr_zero: 1.25,
            twr_exponent: 1.5,
            twr_min_degrees_per_second: 1.0,
            twr_max_degrees_per_second: 45.0,
            m_mdl_at_multiplier: 10.0,
            level_hold_seconds: 0.5,
            level_debounce_seconds: 0.5,
            verbose_logging: false,
            unknown: Default::default(),
        }
    }
}

impl Config {
    /// The config reduced to what the state machine needs, with non-finite numbers defaulted.
    pub(crate) fn settings(&self) -> wish::Settings {
        let defaults = Config::default();
        let finite = |value: f32, fallback: f32| {
            if value.is_finite() {
                value
            } else {
                fallback
            }
        };
        wish::Settings {
            radians_per_second: finite(self.degrees_per_second, defaults.degrees_per_second)
                .to_radians(),
            invert: self.invert,
            level_hold: finite(self.level_hold_seconds, defaults.level_hold_seconds),
            level_debounce: finite(
                self.level_debounce_seconds,
                defaults.level_debounce_seconds,
            ),
            twr_reference: finite(self.twr_reference, defaults.twr_reference),
            twr_zero: finite(self.twr_zero, defaults.twr_zero),
            twr_exponent: {
                let exponent = finite(self.twr_exponent, defaults.twr_exponent);
                if exponent > 0.0 {
                    exponent
                } else {
                    defaults.twr_exponent
                }
            },
            twr_min_rate: finite(
                self.twr_min_degrees_per_second,
                defaults.twr_min_degrees_per_second,
            )
            .to_radians(),
            twr_max_rate: finite(
                self.twr_max_degrees_per_second,
                defaults.twr_max_degrees_per_second,
            )
            .to_radians(),
        }
    }
}

static CURRENT: OnceLock<Config> = OnceLock::new();

/// The live config for the hook. Defaults before `install` is called.
pub(crate) fn get() -> &'static Config {
    CURRENT.get_or_init(Config::default)
}

/// The state-machine view of the live config.
pub(crate) fn settings() -> wish::Settings {
    get().settings()
}

/// The two wish keys as virtual-key codes. An unknown name was reported at load time and falls
/// back to its default here.
pub(crate) fn key_codes() -> (i32, i32) {
    let config = get();
    (
        keys::virtual_key(&config.left_key).unwrap_or(keys::DEFAULT_LEFT),
        keys::virtual_key(&config.right_key).unwrap_or(keys::DEFAULT_RIGHT),
    )
}

/// The `m_mdl_at` multiplier, or `1.0` when the config value is nonsense.
pub(crate) fn mdl_at_multiplier() -> f32 {
    let factor = get().m_mdl_at_multiplier;
    if factor.is_finite() && factor > 0.0 {
        factor
    } else {
        1.0
    }
}

/// Stores the config read by `init`, before any hook can run.
pub(crate) fn install(config: Config) {
    let _ = CURRENT.set(config);
}

pub(crate) fn load() -> Config {
    let config = match std::fs::read_to_string(PATH) {
        Ok(text) => {
            let text = text.trim_start_matches('\u{feff}');
            serde_json::from_str(text).unwrap_or_else(|error| {
                log::error!("hf_wish_tilt: bad {PATH}: {error}; using defaults");
                Config::default()
            })
        }
        Err(_) => {
            let config = Config::default();
            if let Ok(text) = serde_json::to_string_pretty(&config) {
                let _ = std::fs::write(PATH, text);
            }
            config
        }
    };
    for (name, value) in [("left_key", &config.left_key), ("right_key", &config.right_key)] {
        if keys::virtual_key(value).is_none() {
            log::warn!("hf_wish_tilt: unknown {name} '{value}' in {PATH}; using its default");
        }
    }
    if config.left_key.trim().eq_ignore_ascii_case(config.right_key.trim()) {
        log::warn!(
            "hf_wish_tilt: left_key and right_key are both '{}'; the level combo will always be held",
            config.left_key
        );
    }
    if !(config.twr_reference > 0.0) || !config.twr_reference.is_finite() {
        log::warn!(
            "hf_wish_tilt: twr_reference {} is not above 0; the TWR scaling is disabled",
            config.twr_reference
        );
    } else if config.twr_reference <= config.twr_zero {
        log::warn!(
            "hf_wish_tilt: twr_reference {} is not above twr_zero {}; the TWR scaling is disabled",
            config.twr_reference,
            config.twr_zero
        );
    }
    if !(config.twr_exponent > 0.0) || !config.twr_exponent.is_finite() {
        log::warn!(
            "hf_wish_tilt: twr_exponent {} is not above 0; using 1.5",
            config.twr_exponent
        );
    }
    if (config.m_mdl_at_multiplier - 1.0).abs() > f32::EPSILON {
        log::warn!(
            "hf_wish_tilt: m_mdl_at_multiplier {} scales every engine's stabiliser torque budget \
             for ALL ships (same as editing Libraries/OL.seria); set it to 1.0 if you already \
             edited OL.seria",
            config.m_mdl_at_multiplier
        );
    }
    if config.twr_min_degrees_per_second > config.twr_max_degrees_per_second
        && config.twr_min_degrees_per_second.is_finite()
        && config.twr_max_degrees_per_second.is_finite()
    {
        log::warn!(
            "hf_wish_tilt: twr_min_degrees_per_second {} is above twr_max_degrees_per_second {}; \
             they are sorted as a range",
            config.twr_min_degrees_per_second,
            config.twr_max_degrees_per_second
        );
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_agreed_ones() {
        let config = Config::default();
        assert!(config.enabled);
        assert_eq!(config.degrees_per_second, 45.0);
        assert_eq!(config.left_key, "Z");
        assert_eq!(config.right_key, "X");
        assert!(!config.invert);
        assert_eq!(config.twr_reference, 3.0);
        assert_eq!(config.twr_zero, 1.25);
        assert_eq!(config.twr_exponent, 1.5);
        assert_eq!(config.twr_min_degrees_per_second, 1.0);
        assert_eq!(config.twr_max_degrees_per_second, 45.0);
        assert_eq!(config.m_mdl_at_multiplier, 10.0);
        assert_eq!(config.level_hold_seconds, 0.5);
        assert_eq!(config.level_debounce_seconds, 0.5);
        assert!(!config.verbose_logging, "the release config is quiet");
        assert!(config.unknown.is_empty());
    }

    #[test]
    fn the_shipped_config_parses_and_matches_the_defaults() {
        let config: Config =
            serde_json::from_str(include_str!("../release/Modloader/config/wish_tilt.json"))
                .expect("the shipped config parses");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn settings_convert_to_radians_and_survive_nonsense() {
        let settings = Config::default().settings();
        assert!((settings.radians_per_second.to_degrees() - 45.0).abs() < 1e-4);
        assert_eq!(settings.level_hold, 0.5);
        assert_eq!(settings.twr_reference, 3.0);
        assert_eq!(settings.twr_zero, 1.25);
        assert_eq!(settings.twr_exponent, 1.5);
        assert!((settings.twr_min_rate.to_degrees() - 1.0).abs() < 1e-4);
        assert!((settings.twr_max_rate.to_degrees() - 45.0).abs() < 1e-4);

        let broken = Config {
            degrees_per_second: f32::NAN,
            level_hold_seconds: f32::INFINITY,
            twr_reference: f32::NAN,
            twr_zero: f32::NAN,
            twr_exponent: f32::NAN,
            twr_min_degrees_per_second: f32::NEG_INFINITY,
            twr_max_degrees_per_second: f32::NAN,
            ..Config::default()
        };
        let settings = broken.settings();
        assert!((settings.radians_per_second.to_degrees() - 45.0).abs() < 1e-4);
        assert_eq!(settings.level_hold, 0.5);
        assert_eq!(settings.twr_reference, 3.0);
        assert_eq!(settings.twr_zero, 1.25);
        assert_eq!(settings.twr_exponent, 1.5);
        assert!((settings.twr_min_rate.to_degrees() - 1.0).abs() < 1e-4);
        assert!((settings.twr_max_rate.to_degrees() - 45.0).abs() < 1e-4);
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let config: Config =
            serde_json::from_str(r#"{ "degrees_per_second": 90.0, "left_key": "C" }"#)
                .expect("parses");
        assert_eq!(config.degrees_per_second, 90.0);
        assert_eq!(config.left_key, "C");
        assert_eq!(config.right_key, "X", "a missing key keeps its default");
        assert!(!config.invert);
        assert_eq!(config.level_hold_seconds, 0.5);
        assert!(config.unknown.is_empty());
    }
}
