//! Mouse cursor position and modifier keys, behind a trait so the X11 backend can be replaced
//! by a Wayland one later (ADR-003).

pub mod x11;

use thiserror::Error;

use crate::hover::trigger::TriggerKey;

pub use x11::X11Pointer;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PointerError {
    #[error("cannot connect to the display: {0}")]
    Connection(String),
    #[error("cannot read the pointer: {0}")]
    Failed(String),
}

/// Where the cursor is (screen pixels) and which modifier keys are down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointerReading {
    pub x: i32,
    pub y: i32,
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
}

impl PointerReading {
    pub fn is_held(&self, key: TriggerKey) -> bool {
        match key {
            TriggerKey::Shift => self.shift,
            TriggerKey::Control => self.control,
            TriggerKey::Alt => self.alt,
        }
    }
}

pub trait PointerTracker {
    fn read(&mut self) -> Result<PointerReading, PointerError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_held_checks_only_the_requested_key() {
        let reading = PointerReading {
            x: 1,
            y: 2,
            shift: true,
            control: false,
            alt: true,
        };

        assert!(reading.is_held(TriggerKey::Shift));
        assert!(!reading.is_held(TriggerKey::Control));
        assert!(reading.is_held(TriggerKey::Alt));
    }

    #[test]
    fn no_key_is_held_by_default() {
        let reading = PointerReading::default();

        assert!(!reading.is_held(TriggerKey::Shift));
        assert!(!reading.is_held(TriggerKey::Control));
        assert!(!reading.is_held(TriggerKey::Alt));
        assert_eq!((reading.x, reading.y), (0, 0));
    }

    #[test]
    fn errors_have_descriptive_messages() {
        assert_eq!(
            PointerError::Connection("no display".to_string()).to_string(),
            "cannot connect to the display: no display"
        );
        assert_eq!(
            PointerError::Failed("closed".to_string()).to_string(),
            "cannot read the pointer: closed"
        );
    }
}
