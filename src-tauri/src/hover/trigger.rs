//! Decides when the mouse position should be turned into a lookup.

use serde::{Deserialize, Serialize};

pub const DEFAULT_DWELL_MS: u64 = 300;
/// In hold mode: how long the cursor must rest, after moving with the key held, to look again.
pub const HOLD_SETTLE_MS: u64 = 150;
/// Movement (in pixels, per axis) that still counts as keeping the cursor still.
pub const MOVE_TOLERANCE: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerMode {
    /// Look up when the key is pressed, and again when the cursor settles with the key held.
    Hold,
    /// Look up when the cursor has been still for the dwell time.
    Auto,
}

/// The key that has to be held in [`TriggerMode::Hold`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerKey {
    Shift,
    Control,
    Alt,
}

/// One reading of the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerSample {
    pub x: i32,
    pub y: i32,
    /// Whether the configured trigger key is down.
    pub key_held: bool,
    pub time_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Anchor {
    x: i32,
    y: i32,
    since_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Trigger {
    mode: TriggerMode,
    dwell_ms: u64,
    anchor: Option<Anchor>,
    fired: bool,
    key_was_held: bool,
}

impl Trigger {
    pub fn new(mode: TriggerMode, dwell_ms: u64) -> Trigger {
        Trigger {
            mode,
            dwell_ms,
            anchor: None,
            fired: false,
            key_was_held: false,
        }
    }

    pub fn mode(&self) -> TriggerMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: TriggerMode) {
        if mode != self.mode {
            self.mode = mode;
            self.fired = false;
            self.anchor = None;
        }
    }

    /// Feeds a reading; returns the position to look up, if this reading triggers one.
    pub fn update(&mut self, sample: PointerSample) -> Option<(i32, i32)> {
        let moved = self.anchor.is_none_or(|anchor| {
            (sample.x - anchor.x).abs() > MOVE_TOLERANCE
                || (sample.y - anchor.y).abs() > MOVE_TOLERANCE
        });
        if moved {
            self.anchor = Some(Anchor {
                x: sample.x,
                y: sample.y,
                since_ms: sample.time_ms,
            });
            self.fired = false;
        }
        let anchor = self.anchor?;
        let still_for = sample.time_ms.saturating_sub(anchor.since_ms);
        let key_pressed_now = sample.key_held && !self.key_was_held;
        self.key_was_held = sample.key_held;

        let fire = match self.mode {
            TriggerMode::Hold => {
                sample.key_held && !self.fired && (key_pressed_now || still_for >= HOLD_SETTLE_MS)
            }
            TriggerMode::Auto => !self.fired && still_for >= self.dwell_ms,
        };
        if fire {
            self.fired = true;
            return Some((sample.x, sample.y));
        }
        if self.mode == TriggerMode::Hold && !sample.key_held {
            self.fired = false;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, key_held: bool, time_ms: u64) -> PointerSample {
        PointerSample {
            x,
            y,
            key_held,
            time_ms,
        }
    }

    #[test]
    fn hold_mode_fires_the_moment_the_key_is_pressed() {
        let mut trigger = Trigger::new(TriggerMode::Hold, DEFAULT_DWELL_MS);

        assert_eq!(trigger.update(at(500, 900, false, 0)), None);
        assert_eq!(trigger.update(at(500, 900, true, 20)), Some((500, 900)));
    }

    #[test]
    fn hold_mode_fires_once_while_the_key_stays_down_and_the_cursor_is_still() {
        let mut trigger = Trigger::new(TriggerMode::Hold, DEFAULT_DWELL_MS);
        trigger.update(at(500, 900, false, 0));
        assert!(trigger.update(at(500, 900, true, 20)).is_some());

        for time in (40..2_000).step_by(20) {
            assert_eq!(trigger.update(at(500, 900, true, time)), None);
        }
    }

    #[test]
    fn hold_mode_looks_again_after_moving_with_the_key_held_and_resting() {
        let mut trigger = Trigger::new(TriggerMode::Hold, DEFAULT_DWELL_MS);
        trigger.update(at(500, 900, false, 0));
        trigger.update(at(500, 900, true, 20));

        assert_eq!(trigger.update(at(620, 905, true, 100)), None);
        assert_eq!(trigger.update(at(620, 905, true, 200)), None);
        assert_eq!(trigger.update(at(620, 905, true, 250)), Some((620, 905)));
        assert_eq!(trigger.update(at(620, 905, true, 400)), None);
    }

    #[test]
    fn hold_mode_does_not_fire_while_the_cursor_keeps_moving() {
        let mut trigger = Trigger::new(TriggerMode::Hold, DEFAULT_DWELL_MS);
        trigger.update(at(500, 900, false, 0));
        trigger.update(at(500, 900, true, 20));

        for step in 1..20 {
            assert_eq!(
                trigger.update(at(500 + step * 30, 900, true, 20 + step as u64 * 20)),
                None
            );
        }
    }

    #[test]
    fn hold_mode_ignores_a_resting_cursor_without_the_key() {
        let mut trigger = Trigger::new(TriggerMode::Hold, DEFAULT_DWELL_MS);

        for time in (0..3_000).step_by(50) {
            assert_eq!(trigger.update(at(500, 900, false, time)), None);
        }
    }

    #[test]
    fn hold_mode_fires_again_after_the_key_is_released_and_pressed() {
        let mut trigger = Trigger::new(TriggerMode::Hold, DEFAULT_DWELL_MS);
        trigger.update(at(500, 900, false, 0));
        assert!(trigger.update(at(500, 900, true, 20)).is_some());
        trigger.update(at(500, 900, false, 100));

        assert_eq!(trigger.update(at(500, 900, true, 200)), Some((500, 900)));
    }

    #[test]
    fn hold_mode_fires_when_the_key_is_already_down_and_the_cursor_rests() {
        let mut trigger = Trigger::new(TriggerMode::Hold, DEFAULT_DWELL_MS);

        assert_eq!(trigger.update(at(500, 900, true, 0)), Some((500, 900)));
    }

    #[test]
    fn auto_mode_fires_after_the_cursor_rests_for_the_dwell_time() {
        let mut trigger = Trigger::new(TriggerMode::Auto, 300);

        assert_eq!(trigger.update(at(500, 900, false, 0)), None);
        assert_eq!(trigger.update(at(500, 900, false, 299)), None);
        assert_eq!(trigger.update(at(500, 900, false, 300)), Some((500, 900)));
    }

    #[test]
    fn auto_mode_fires_once_per_rest_and_again_after_moving() {
        let mut trigger = Trigger::new(TriggerMode::Auto, 300);
        trigger.update(at(500, 900, false, 0));
        assert!(trigger.update(at(500, 900, false, 300)).is_some());
        assert_eq!(trigger.update(at(500, 900, false, 600)), None);

        assert_eq!(trigger.update(at(700, 900, false, 700)), None);
        assert_eq!(trigger.update(at(700, 900, false, 1_000)), Some((700, 900)));
    }

    #[test]
    fn auto_mode_waits_while_the_cursor_keeps_moving() {
        let mut trigger = Trigger::new(TriggerMode::Auto, 300);

        for step in 0..50 {
            assert_eq!(
                trigger.update(at(100 + step * 20, 900, false, step as u64 * 100)),
                None
            );
        }
    }

    #[test]
    fn small_jitter_counts_as_keeping_the_cursor_still() {
        let mut trigger = Trigger::new(TriggerMode::Auto, 300);
        trigger.update(at(500, 900, false, 0));
        trigger.update(at(503, 898, false, 100));
        trigger.update(at(497, 902, false, 200));

        assert_eq!(trigger.update(at(501, 900, false, 300)), Some((501, 900)));
    }

    #[test]
    fn jitter_beyond_the_tolerance_restarts_the_wait() {
        let mut trigger = Trigger::new(TriggerMode::Auto, 300);
        trigger.update(at(500, 900, false, 0));
        trigger.update(at(500 + MOVE_TOLERANCE + 1, 900, false, 250));

        assert_eq!(trigger.update(at(505, 900, false, 400)), None);
        assert!(trigger.update(at(505, 900, false, 550)).is_some());
    }

    #[test]
    fn switching_the_mode_starts_clean() {
        let mut trigger = Trigger::new(TriggerMode::Auto, 300);
        trigger.update(at(500, 900, false, 0));
        assert!(trigger.update(at(500, 900, false, 300)).is_some());

        trigger.set_mode(TriggerMode::Hold);
        assert_eq!(trigger.mode(), TriggerMode::Hold);

        assert_eq!(trigger.update(at(500, 900, false, 400)), None);
        assert!(trigger.update(at(500, 900, true, 420)).is_some());
    }

    #[test]
    fn setting_the_same_mode_changes_nothing() {
        let mut trigger = Trigger::new(TriggerMode::Auto, 300);
        trigger.update(at(500, 900, false, 0));
        assert!(trigger.update(at(500, 900, false, 300)).is_some());

        trigger.set_mode(TriggerMode::Auto);

        assert_eq!(trigger.update(at(500, 900, false, 400)), None);
    }

    #[test]
    fn a_clock_that_goes_backwards_does_not_fire() {
        let mut trigger = Trigger::new(TriggerMode::Auto, 300);
        trigger.update(at(500, 900, false, 1_000));

        assert_eq!(trigger.update(at(500, 900, false, 500)), None);
    }

    #[test]
    fn modes_and_keys_serialize_in_camel_case() {
        assert_eq!(
            serde_json::to_value(TriggerMode::Hold).expect("mode"),
            serde_json::json!("hold")
        );
        assert_eq!(
            serde_json::to_value(TriggerMode::Auto).expect("mode"),
            serde_json::json!("auto")
        );
        assert_eq!(
            serde_json::to_value(TriggerKey::Shift).expect("key"),
            serde_json::json!("shift")
        );
        assert_eq!(
            serde_json::to_value(TriggerKey::Control).expect("key"),
            serde_json::json!("control")
        );
        assert_eq!(
            serde_json::to_value(TriggerKey::Alt).expect("key"),
            serde_json::json!("alt")
        );
        let parsed: TriggerMode = serde_json::from_value(serde_json::json!("auto")).expect("parse");
        assert_eq!(parsed, TriggerMode::Auto);
    }
}
