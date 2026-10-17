//! Mouse cursor position and modifier keys, behind a trait so the X11 backend can be replaced
//! by a Wayland one later (ADR-003).

#[cfg(windows)]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod x11;

use thiserror::Error;

use crate::capture::Region;
use crate::hover::trigger::TriggerKey;

#[cfg(windows)]
pub use windows::WindowsPointer;
#[cfg(target_os = "linux")]
pub use x11::X11Pointer;

/// The pointer backend of the system the app runs on.
#[cfg(windows)]
pub type SystemPointer = WindowsPointer;
/// The pointer backend of the system the app runs on.
#[cfg(target_os = "linux")]
pub type SystemPointer = X11Pointer;

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

/// Which application a window belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowIdentity {
    /// The window class (`WM_CLASS`), e.g. `mpv` or `Navigator`.
    pub app_name: Option<String>,
    pub title: Option<String>,
}

pub trait PointerTracker {
    fn read(&mut self) -> Result<PointerReading, PointerError>;

    /// The top-level window under the pointer (frame included), in screen pixels; `None` over
    /// the bare desktop or when the backend cannot tell.
    fn window_under_pointer(&mut self) -> Result<Option<Region>, PointerError> {
        Ok(None)
    }

    /// The application and title of the window under the pointer; empty when unknown.
    fn identify_window_under_pointer(&mut self) -> Result<WindowIdentity, PointerError> {
        Ok(WindowIdentity::default())
    }
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
