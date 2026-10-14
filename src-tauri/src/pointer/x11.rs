use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};
use x11rb::rust_connection::RustConnection;

use super::{PointerError, PointerReading, PointerTracker};

/// Reads the pointer of an X11 display (works without any special permission).
pub struct X11Pointer {
    connection: RustConnection,
    root: u32,
}

impl X11Pointer {
    /// Connects to the display named by `$DISPLAY`.
    pub fn connect() -> Result<X11Pointer, PointerError> {
        let (connection, screen_number) =
            x11rb::connect(None).map_err(|error| PointerError::Connection(error.to_string()))?;
        let root = connection
            .setup()
            .roots
            .get(screen_number)
            .ok_or_else(|| PointerError::Connection("the screen does not exist".to_string()))?
            .root;
        Ok(X11Pointer { connection, root })
    }
}

impl PointerTracker for X11Pointer {
    fn read(&mut self) -> Result<PointerReading, PointerError> {
        let reply = self
            .connection
            .query_pointer(self.root)
            .map_err(|error| PointerError::Failed(error.to_string()))?
            .reply()
            .map_err(|error| PointerError::Failed(error.to_string()))?;
        Ok(reading_from(
            i32::from(reply.root_x),
            i32::from(reply.root_y),
            reply.mask,
        ))
    }
}

fn reading_from(x: i32, y: i32, mask: KeyButMask) -> PointerReading {
    PointerReading {
        x,
        y,
        shift: mask.contains(KeyButMask::SHIFT),
        control: mask.contains(KeyButMask::CONTROL),
        alt: mask.contains(KeyButMask::MOD1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_masks_are_translated() {
        assert_eq!(
            reading_from(10, 20, KeyButMask::SHIFT),
            PointerReading {
                x: 10,
                y: 20,
                shift: true,
                control: false,
                alt: false
            }
        );
        assert_eq!(
            reading_from(0, 0, KeyButMask::CONTROL | KeyButMask::MOD1),
            PointerReading {
                x: 0,
                y: 0,
                shift: false,
                control: true,
                alt: true
            }
        );
        assert_eq!(
            reading_from(5, 6, KeyButMask::from(0_u16)),
            PointerReading {
                x: 5,
                y: 6,
                ..Default::default()
            }
        );
    }

    #[test]
    fn mouse_buttons_are_not_mistaken_for_modifiers() {
        let reading = reading_from(1, 1, KeyButMask::BUTTON1 | KeyButMask::LOCK);

        assert!(!reading.shift && !reading.control && !reading.alt);
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn reads_the_position_the_cursor_was_moved_to() {
        let (connection, screen_number) = x11rb::connect(None).expect("display");
        let root = connection.setup().roots[screen_number].root;
        connection
            .warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, 321, 234)
            .expect("warp")
            .check()
            .expect("warp reply");
        let mut pointer = X11Pointer::connect().expect("connect");

        let reading = pointer.read().expect("read");

        assert_eq!((reading.x, reading.y), (321, 234));
        assert!(!reading.shift && !reading.control && !reading.alt);
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn connecting_fails_clearly_when_the_display_does_not_exist() {
        let original = std::env::var("DISPLAY").ok();
        std::env::set_var("DISPLAY", ":987");

        let result = X11Pointer::connect();

        match original {
            Some(value) => std::env::set_var("DISPLAY", value),
            None => std::env::remove_var("DISPLAY"),
        }
        assert!(matches!(result, Err(PointerError::Connection(_))));
    }
}
