use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, KeyButMask, Window};
use x11rb::rust_connection::RustConnection;
use x11rb::NONE;

use super::{PointerError, PointerReading, PointerTracker, WindowIdentity};
use crate::capture::Region;

/// Reads the pointer of an X11 display (works without any special permission).
pub struct X11Pointer {
    connection: RustConnection,
    root: u32,
}

impl X11Pointer {
    /// Connects to the display named by `$DISPLAY`.
    pub fn connect() -> Result<X11Pointer, PointerError> {
        X11Pointer::connect_to(None)
    }

    /// Connects to the named display (`None`: the one in `$DISPLAY`).
    pub fn connect_to(display: Option<&str>) -> Result<X11Pointer, PointerError> {
        let (connection, screen_number) =
            x11rb::connect(display).map_err(|error| PointerError::Connection(error.to_string()))?;
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

    fn window_under_pointer(&mut self) -> Result<Option<Region>, PointerError> {
        let failed = |error: &dyn std::fmt::Display| PointerError::Failed(error.to_string());
        let pointer = self
            .connection
            .query_pointer(self.root)
            .map_err(|error| failed(&error))?
            .reply()
            .map_err(|error| failed(&error))?;
        if pointer.child == NONE {
            return Ok(None);
        }
        let geometry = self
            .connection
            .get_geometry(pointer.child)
            .map_err(|error| failed(&error))?
            .reply()
            .map_err(|error| failed(&error))?;
        Ok(Some(frame_region(
            geometry.x,
            geometry.y,
            geometry.width,
            geometry.height,
            geometry.border_width,
        )))
    }

    fn identify_window_under_pointer(&mut self) -> Result<WindowIdentity, PointerError> {
        let pointer = self
            .connection
            .query_pointer(self.root)
            .map_err(|error| PointerError::Failed(error.to_string()))?
            .reply()
            .map_err(|error| PointerError::Failed(error.to_string()))?;
        if pointer.child == NONE {
            return Ok(WindowIdentity::default());
        }
        let Some((client, class)) = self.client_window(pointer.child) else {
            return Ok(WindowIdentity::default());
        };
        Ok(WindowIdentity {
            app_name: class_from_wm_class(&class),
            title: self.window_title(client),
        })
    }
}

impl X11Pointer {
    fn text_property(&self, window: Window, property: AtomEnum) -> Option<Vec<u8>> {
        let reply = self
            .connection
            .get_property(false, window, property, AtomEnum::STRING, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        if reply.value.is_empty() {
            return None;
        }
        Some(reply.value)
    }

    /// The `_NET_WM_NAME` (UTF-8) of the window, or its old-style `WM_NAME`.
    fn window_title(&self, window: Window) -> Option<String> {
        let atom = self
            .connection
            .intern_atom(false, b"_NET_WM_NAME")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let utf8 = self
            .connection
            .intern_atom(false, b"UTF8_STRING")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let modern = self
            .connection
            .get_property(false, window, atom, utf8, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        if !modern.value.is_empty() {
            return Some(String::from_utf8_lossy(&modern.value).into_owned());
        }
        let old = self.text_property(window, AtomEnum::WM_NAME)?;
        Some(String::from_utf8_lossy(&old).into_owned())
    }

    /// The application's window (the one with a `WM_CLASS`) inside a top-level frame: the frame
    /// itself or the first window below it that has one.
    fn client_window(&self, frame: Window) -> Option<(Window, Vec<u8>)> {
        let mut pending = vec![frame];
        let mut visited = 0;
        while let Some(window) = pending.pop() {
            visited += 1;
            if visited > 64 {
                return None;
            }
            if let Some(class) = self.text_property(window, AtomEnum::WM_CLASS) {
                return Some((window, class));
            }
            if let Some(tree) = self
                .connection
                .query_tree(window)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
            {
                pending.extend(tree.children.iter().rev());
            }
        }
        None
    }
}

/// The class part of a `WM_CLASS` value (`instance\0class\0`).
fn class_from_wm_class(value: &[u8]) -> Option<String> {
    let mut names = value
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty());
    let instance = names.next()?;
    let class = names.next().unwrap_or(instance);
    Some(String::from_utf8_lossy(class).into_owned())
}

/// The screen rectangle of a window including its border.
fn frame_region(x: i16, y: i16, width: u16, height: u16, border: u16) -> Region {
    Region {
        x: i32::from(x),
        y: i32::from(y),
        width: u32::from(width) + 2 * u32::from(border),
        height: u32::from(height) + 2 * u32::from(border),
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
    fn the_frame_includes_the_border_on_every_side() {
        assert_eq!(
            frame_region(100, 50, 800, 600, 0),
            Region {
                x: 100,
                y: 50,
                width: 800,
                height: 600
            }
        );
        assert_eq!(
            frame_region(100, 50, 800, 600, 3),
            Region {
                x: 100,
                y: 50,
                width: 806,
                height: 606
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
        let _display = crate::x11_test_lock();
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
    fn finds_the_window_under_the_pointer_and_nothing_over_the_bare_desktop() {
        let _display = crate::x11_test_lock();
        use x11rb::protocol::xproto::{CreateWindowAux, WindowClass};
        use x11rb::COPY_DEPTH_FROM_PARENT;

        let (connection, screen_number) = x11rb::connect(None).expect("display");
        let root = connection.setup().roots[screen_number].root;
        let window = connection.generate_id().expect("id");
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                root,
                200,
                150,
                300,
                120,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().override_redirect(1),
            )
            .expect("create");
        connection.map_window(window).expect("map");
        connection
            .get_input_focus()
            .expect("sync")
            .reply()
            .expect("sync reply");
        let mut pointer = X11Pointer::connect().expect("connect");

        connection
            .warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, 250, 180)
            .expect("warp")
            .check()
            .expect("warp reply");
        let inside = pointer.window_under_pointer().expect("inside");
        connection
            .warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, 900, 700)
            .expect("warp")
            .check()
            .expect("warp reply");
        let outside = pointer.window_under_pointer().expect("outside");

        assert_eq!(
            inside,
            Some(Region {
                x: 200,
                y: 150,
                width: 300,
                height: 120
            })
        );
        assert_eq!(outside, None);
    }

    #[test]
    fn the_class_is_the_second_name_of_wm_class() {
        assert_eq!(
            class_from_wm_class(b"navigator\0Firefox\0"),
            Some("Firefox".to_string())
        );
        assert_eq!(class_from_wm_class(b"mpv\0mpv\0"), Some("mpv".to_string()));
        assert_eq!(class_from_wm_class(b"solo\0"), Some("solo".to_string()));
        assert_eq!(class_from_wm_class(b"\0\0"), None);
        assert_eq!(class_from_wm_class(b""), None);
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn identifies_the_application_and_title_of_the_window_under_the_pointer() {
        use x11rb::protocol::xproto::{CreateWindowAux, PropMode, WindowClass};
        use x11rb::wrapper::ConnectionExt as _;
        use x11rb::COPY_DEPTH_FROM_PARENT;

        let _display = crate::x11_test_lock();
        let (connection, screen_number) = x11rb::connect(None).expect("display");
        let root = connection.setup().roots[screen_number].root;
        let window = connection.generate_id().expect("id");
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                root,
                300,
                300,
                200,
                100,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().override_redirect(1),
            )
            .expect("create");
        connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"mpv\0mpv\0",
            )
            .expect("class");
        connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                b"episode 1",
            )
            .expect("title");
        connection.map_window(window).expect("map");
        connection
            .get_input_focus()
            .expect("sync")
            .reply()
            .expect("sync reply");
        let mut pointer = X11Pointer::connect().expect("connect");

        connection
            .warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, 350, 340)
            .expect("warp")
            .check()
            .expect("warp reply");
        let inside = pointer.identify_window_under_pointer().expect("inside");
        connection
            .warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, 1200, 900)
            .expect("warp")
            .check()
            .expect("warp reply");
        let outside = pointer.identify_window_under_pointer().expect("outside");

        assert_eq!(
            inside,
            WindowIdentity {
                app_name: Some("mpv".to_string()),
                title: Some("episode 1".to_string())
            }
        );
        assert_eq!(outside, WindowIdentity::default());
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn connecting_fails_clearly_when_the_display_does_not_exist() {
        let _display = crate::x11_test_lock();
        let result = X11Pointer::connect_to(Some(":987"));

        assert!(matches!(result, Err(PointerError::Connection(_))));
    }
}
