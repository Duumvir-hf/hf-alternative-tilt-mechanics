//! The wish-tilt state machine: the only piece of this mod that is not engine plumbing, and the
//! only piece with a life of its own, so it is pure and unit-tested.
//!
//! * `angle` is the target hull attitude in **world radians**, wrapped to `[-pi, pi]`, exactly the
//!   quantity the stabiliser compares the hull against. `0.0` is level, which is why a fresh ship
//!   and a released combo behave exactly like vanilla.
//! * The left key drives it negative, the right key positive (one `rate` per second; `invert` flips
//!   both).
//! * both keys together freeze it and, after `level_hold`, snap it back to `0.0`. Releasing either
//!   key then starts a `level_debounce` window in which the keys are ignored, so letting go of the
//!   second key of the combo cannot nudge the target.

/// The two wish-key flags, read from the configured keys (`left_key` / `right_key`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Keys {
    pub left: bool,
    pub right: bool,
}

impl Keys {
    pub(crate) const NONE: Keys = Keys { left: false, right: false };

    pub(crate) fn new(left: bool, right: bool) -> Self {
        Self { left, right }
    }
}

/// The config, in the units the state machine wants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Magnitude of the wish drift while one key is held, radians per second, **at or above
    /// `twr_reference`**.
    pub radians_per_second: f32,
    /// Swap which key goes which way.
    pub invert: bool,
    /// How long both keys must be held before the target snaps back to level.
    pub level_hold: f32,
    /// How long the keys are ignored after a completed level.
    pub level_debounce: f32,
    /// Battle thrust-to-weight ratio at which the full `radians_per_second` applies. Below it the
    /// rate follows a square between `twr_zero` and this reference; above it the rate is not
    /// boosted.
    pub twr_reference: f32,
    /// Battle TWR at or below which the curve reaches zero (the rate then sits on
    /// `twr_min_rate`). Moving this up makes the curve harsher on heavy ships.
    pub twr_zero: f32,
    /// Exponent of the normalised curve: `1.0` is linear, `2.0` is the square, `1.5` (default) is
    /// between them.
    pub twr_exponent: f32,
    /// Lower clamp for the scaled rate, radians per second (magnitude).
    pub twr_min_rate: f32,
    /// Upper clamp for the scaled rate, radians per second (magnitude).
    pub twr_max_rate: f32,
}

impl Settings {
    /// The wish drift rate for a ship with this battle TWR.
    ///
    /// Above `twr_reference` the base rate is unchanged. Between `twr_zero` and the reference the
    /// rate follows `((twr - twr_zero) / (twr_reference - twr_zero)) ^ twr_exponent`; at or below
    /// `twr_zero` it is zero. The magnitude is then clamped to `[twr_min_rate, twr_max_rate]`, so
    /// a base above the upper clamp keeps the output flat until the curve drops under it (a later,
    /// sharper visible knee). A negative base keeps its sign, so the config's "negative
    /// `degrees_per_second` swaps the keys" still works.
    pub(crate) fn rate_for_twr(&self, twr: f32) -> f32 {
        let base = self.radians_per_second;
        let reference = self.twr_reference;
        let zero = self.twr_zero;
        let exponent = self.twr_exponent;
        if base == 0.0
            || !base.is_finite()
            || !twr.is_finite()
            || !reference.is_finite()
            || !zero.is_finite()
            || !(reference > zero)
            || !(exponent > 0.0)
            || !exponent.is_finite()
        {
            return base;
        }
        if !self.twr_min_rate.is_finite() || !self.twr_max_rate.is_finite() {
            return base;
        }
        let ratio = if twr >= reference {
            1.0
        } else if twr <= zero {
            0.0
        } else {
            let scaled = (twr - zero) / (reference - zero);
            scaled.powf(exponent).min(1.0)
        };
        let (low, high) = if self.twr_min_rate <= self.twr_max_rate {
            (self.twr_min_rate, self.twr_max_rate)
        } else {
            (self.twr_max_rate, self.twr_min_rate)
        };
        (base.abs() * ratio).clamp(low, high).copysign(base)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            radians_per_second: 45.0_f32.to_radians(),
            invert: false,
            level_hold: 0.5,
            level_debounce: 0.5,
            twr_reference: 3.0,
            twr_zero: 1.25,
            twr_exponent: 1.5,
            twr_min_rate: 1.0_f32.to_radians(),
            twr_max_rate: 45.0_f32.to_radians(),
        }
    }
}

/// Wraps an angle into `[-pi, pi]`, the same fold the stabiliser applies to `Frame + 0x138`.
pub(crate) fn wrap_pi(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 0.0;
    }
    let turn = std::f32::consts::TAU;
    angle - (angle / turn).round() * turn
}

/// The persistent attitude target.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Wish {
    /// The ship this target belongs to; a new pointer (new battle, new ship) resets it.
    creature: usize,
    angle: f32,
    combo: f32,
    combo_done: bool,
    debounce: f32,
}

impl Wish {
    pub(crate) const fn new() -> Self {
        Self {
            creature: 0,
            angle: 0.0,
            combo: 0.0,
            combo_done: false,
            debounce: 0.0,
        }
    }

    /// The current target. Read by the tests and by the panel readout.
    pub(crate) fn angle(&self) -> f32 {
        self.angle
    }

    /// True while the post-combo debounce is running.
    #[allow(dead_code)]
    pub(crate) fn debouncing(&self) -> bool {
        self.debounce > 0.0
    }

    /// Forgets the target (a new ship, or the mod disabled).
    pub(crate) fn reset(&mut self, creature: usize) {
        *self = Self::new();
        self.creature = creature;
    }

    /// Advances by one frame. `dt` is 0 for every call after the first in the same frame, which
    /// makes this safe to call once per engine part.
    pub(crate) fn step(&mut self, creature: usize, dt: f32, keys: Keys, settings: &Settings) -> f32 {
        if self.creature != creature {
            self.reset(creature);
        }
        if !(dt > 0.0) || !dt.is_finite() {
            return self.angle;
        }

        if keys.left && keys.right {
            // The pair is the level command: hold both and the target snaps to 0.
            self.combo += dt;
            if self.combo >= settings.level_hold {
                self.angle = 0.0;
                self.combo_done = true;
            }
            return self.angle;
        }

        if self.combo_done {
            self.combo_done = false;
            self.debounce = settings.level_debounce;
        }
        self.combo = 0.0;

        if self.debounce > 0.0 {
            // Letting go of the combo must not start a drift: ignore the keys for a moment.
            self.debounce = (self.debounce - dt).max(0.0);
            return self.angle;
        }

        let direction = match (keys.left, keys.right) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };
        if direction == 0.0 {
            return self.angle;
        }
        let signed = if settings.invert { -direction } else { direction };
        self.angle = wrap_pi(self.angle + signed * settings.radians_per_second * dt);
        self.angle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 0.02;

    fn hold(wish: &mut Wish, seconds: f32, keys: Keys, settings: &Settings) -> f32 {
        let frames = (seconds / FRAME).round() as usize;
        let mut angle = wish.angle();
        for _ in 0..frames {
            angle = wish.step(1, FRAME, keys, settings);
        }
        angle
    }

    #[test]
    fn half_a_second_of_the_left_key_is_22_5_degrees_left() {
        let settings = Settings::default();
        let mut wish = Wish::new();
        let angle = hold(&mut wish, 0.5, Keys::new(true, false), &settings);
        assert!((angle.to_degrees() - -22.5).abs() < 0.01, "{angle}");
    }

    #[test]
    fn the_right_key_goes_the_other_way_and_invert_swaps_them() {
        let settings = Settings::default();
        let mut wish = Wish::new();
        let angle = hold(&mut wish, 1.0, Keys::new(false, true), &settings);
        assert!((angle.to_degrees() - 45.0).abs() < 0.01, "{angle}");

        let flipped = Settings {
            invert: true,
            ..settings
        };
        let mut wish = Wish::new();
        let angle = hold(&mut wish, 1.0, Keys::new(false, true), &flipped);
        assert!((angle.to_degrees() - -45.0).abs() < 0.01, "{angle}");
    }

    #[test]
    fn the_target_is_kept_when_the_keys_are_released() {
        let settings = Settings::default();
        let mut wish = Wish::new();
        hold(&mut wish, 0.5, Keys::new(true, false), &settings);
        let held = wish.angle();
        for _ in 0..250 {
            wish.step(1, FRAME, Keys::NONE, &settings);
        }
        assert_eq!(wish.angle(), held, "an idle frame must not move the target");
    }

    #[test]
    fn the_target_wraps_past_half_a_turn() {
        let settings = Settings::default();
        let mut wish = Wish::new();
        let angle = hold(&mut wish, 5.0, Keys::new(true, false), &settings);
        // 5 s * 45 deg/s = 225 deg left, i.e. +135 deg wrapped.
        assert!((angle.to_degrees() - 135.0).abs() < 0.01, "{angle}");
    }

    #[test]
    fn only_the_first_call_of_a_frame_moves_the_target() {
        let settings = Settings::default();
        let mut wish = Wish::new();
        wish.step(1, FRAME, Keys::new(true, false), &settings);
        let after_one = wish.angle();
        // The other three engines of the ship in the same frame.
        for _ in 0..3 {
            wish.step(1, 0.0, Keys::new(true, false), &settings);
        }
        assert_eq!(wish.angle(), after_one);
        assert!((after_one.to_degrees() - -0.9).abs() < 0.001);
    }

    #[test]
    fn both_keys_level_after_half_a_second_and_then_debounce() {
        let settings = Settings::default();
        let mut wish = Wish::new();
        hold(&mut wish, 1.0, Keys::new(true, false), &settings);
        assert!(wish.angle() < -0.7);

        // 0.4 s of the combo: not yet.
        hold(&mut wish, 0.4, Keys::new(true, true), &settings);
        assert!(wish.angle() < -0.7, "level must wait for the full hold");

        // 0.2 s more completes it.
        hold(&mut wish, 0.2, Keys::new(true, true), &settings);
        assert_eq!(wish.angle(), 0.0);

        // Releasing the right key while the left stays down must not drift for `level_debounce`.
        hold(&mut wish, 0.4, Keys::new(true, false), &settings);
        assert_eq!(wish.angle(), 0.0, "the debounce swallows the release");
        assert!(wish.debouncing());

        // After the debounce the keys work again.
        hold(&mut wish, 0.4, Keys::new(true, false), &settings);
        assert!(wish.angle() < -0.0, "the left key drifts again after the debounce");
    }

    #[test]
    fn a_new_ship_starts_level() {
        let settings = Settings::default();
        let mut wish = Wish::new();
        hold(&mut wish, 1.0, Keys::new(true, false), &settings);
        assert!(wish.angle() != 0.0);
        wish.step(99, FRAME, Keys::NONE, &settings);
        assert_eq!(wish.angle(), 0.0);
    }

    #[test]
    fn wrap_pi_keeps_the_canonical_range() {
        for degrees in [-720.0_f32, -359.0, -181.0, -180.0, -90.0, 0.0, 90.0, 179.0, 181.0, 360.0] {
            let wrapped = wrap_pi(degrees.to_radians());
            assert!(wrapped >= -std::f32::consts::PI && wrapped <= std::f32::consts::PI);
            let difference = wrap_pi(wrapped - degrees.to_radians());
            assert!(difference.abs() < 1e-4, "{degrees} -> {wrapped}");
        }
        assert_eq!(wrap_pi(f32::NAN), 0.0);
    }

    #[test]
    fn the_twr_scale_is_a_power_between_zero_and_the_reference() {
        let settings = Settings::default();
        let rate = |twr| settings.rate_for_twr(twr).to_degrees();
        assert!((rate(3.0) - 45.0).abs() < 1e-4);
        assert!((rate(6.0) - 45.0).abs() < 1e-4, "above the reference is not boosted");
        assert!((rate(2.5) - 27.17).abs() < 0.02);
        assert!((rate(2.0) - 12.63).abs() < 0.02);
        assert!((rate(1.5) - 2.43).abs() < 0.02);
        assert!((rate(1.37) - 1.0).abs() < 1e-4, "the Sevastopol-like point is on the floor");
        assert!((rate(1.25) - 1.0).abs() < 1e-4, "the curve reaches zero at 1.25");
        assert!((rate(1.0) - 1.0).abs() < 1e-4);
        assert!((rate(0.1) - 1.0).abs() < 1e-4, "lower clamp");
        assert!((rate(-2.0) - 1.0).abs() < 1e-4);
        assert!((rate(f32::NAN) - 45.0).abs() < 1e-4, "unknown TWR keeps the base rate");
    }

    #[test]
    fn the_exponent_selects_linear_or_square() {
        let linear = Settings {
            twr_exponent: 1.0,
            ..Settings::default()
        };
        assert!((linear.rate_for_twr(2.0).to_degrees() - 19.29).abs() < 0.02);
        let square = Settings {
            twr_exponent: 2.0,
            ..Settings::default()
        };
        assert!((square.rate_for_twr(2.0).to_degrees() - 8.27).abs() < 0.02);
    }

    #[test]
    fn a_base_above_the_upper_clamp_makes_a_later_sharper_knee() {
        let settings = Settings {
            radians_per_second: 90.0_f32.to_radians(),
            twr_max_rate: 45.0_f32.to_radians(),
            ..Settings::default()
        };
        let rate = |twr| settings.rate_for_twr(twr).to_degrees();
        assert!((rate(3.0) - 45.0).abs() < 1e-4);
        assert!((rate(2.5) - 45.0).abs() < 1e-4, "still saturated at 2.5");
        assert!((rate(2.0) - 25.25).abs() < 0.02, "falls steeply below the knee");
        assert!((rate(1.25) - 1.0).abs() < 1e-4, "the floor still wins at the bottom");
    }

    #[test]
    fn the_twr_scale_keeps_a_negative_base_and_sorts_the_clamps() {
        let mut settings = Settings::default();
        settings.radians_per_second = -45.0_f32.to_radians();
        assert!((settings.rate_for_twr(2.0).to_degrees() + 12.63).abs() < 0.02);
        assert!((settings.rate_for_twr(0.1).to_degrees() + 1.0).abs() < 1e-6);

        let swapped = Settings {
            twr_min_rate: 40.0_f32.to_radians(),
            twr_max_rate: 5.0_f32.to_radians(),
            ..Settings::default()
        };
        assert!((swapped.rate_for_twr(3.0).to_degrees() - 40.0).abs() < 1e-4, "clamps are sorted");
    }
}


