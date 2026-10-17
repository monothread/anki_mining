//! The mouse pointer, the modifier keys and the window under the pointer on Windows.

use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::{CloseHandle, POINT, RECT};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetCursorPos, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    WindowFromPoint, GA_ROOT,
};

use super::{PointerError, PointerReading, PointerTracker, WindowIdentity};
use crate::capture::windows::{make_dpi_aware, virtual_screen};
use crate::capture::Region;

/// Reads the pointer of the Windows desktop (needs no permission). Positions are measured from
/// the top-left corner of the area that holds all the monitors, like the screenshots.
pub struct WindowsPointer {
    origin: (i32, i32),
}

impl WindowsPointer {
    pub fn connect() -> Result<WindowsPointer, PointerError> {
        make_dpi_aware();
        Ok(WindowsPointer {
            origin: virtual_screen().origin,
        })
    }

    fn cursor(&self) -> Result<POINT, PointerError> {
        let mut point = POINT { x: 0, y: 0 };
        // SAFETY: `point` is a valid, writable POINT.
        let read = unsafe { GetCursorPos(&mut point) };
        if read == 0 {
            return Err(PointerError::Failed(
                "GetCursorPos failed (is the desktop locked?)".to_string(),
            ));
        }
        Ok(point)
    }

    /// The top-level window under the pointer.
    fn root_window_under_pointer(&self) -> Result<Option<*mut core::ffi::c_void>, PointerError> {
        let point = self.cursor()?;
        // SAFETY: both calls only look at the handle they are given.
        let window = unsafe { WindowFromPoint(point) };
        if window.is_null() {
            return Ok(None);
        }
        let root = unsafe { GetAncestor(window, GA_ROOT) };
        Ok(Some(if root.is_null() { window } else { root }))
    }
}

fn key_is_down(key: u16) -> bool {
    // SAFETY: GetAsyncKeyState has no preconditions; the high bit tells the key is down.
    unsafe { GetAsyncKeyState(i32::from(key)) < 0 }
}

impl PointerTracker for WindowsPointer {
    fn read(&mut self) -> Result<PointerReading, PointerError> {
        let point = self.cursor()?;
        Ok(PointerReading {
            x: point.x - self.origin.0,
            y: point.y - self.origin.1,
            shift: key_is_down(VK_SHIFT),
            control: key_is_down(VK_CONTROL),
            alt: key_is_down(VK_MENU),
        })
    }

    fn window_under_pointer(&mut self) -> Result<Option<Region>, PointerError> {
        let Some(window) = self.root_window_under_pointer()? else {
            return Ok(None);
        };
        let mut rectangle = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: `rectangle` is a valid, writable RECT.
        if unsafe { GetWindowRect(window, &mut rectangle) } == 0 {
            return Ok(None);
        }
        Ok(Some(region_of(rectangle, self.origin)))
    }

    fn identify_window_under_pointer(&mut self) -> Result<WindowIdentity, PointerError> {
        let Some(window) = self.root_window_under_pointer()? else {
            return Ok(WindowIdentity::default());
        };
        Ok(WindowIdentity {
            app_name: process_name(window),
            title: window_title(window),
        })
    }
}

/// The screen rectangle of a window, measured from `origin`.
fn region_of(rectangle: RECT, origin: (i32, i32)) -> Region {
    Region {
        x: rectangle.left - origin.0,
        y: rectangle.top - origin.1,
        width: u32::try_from(rectangle.right - rectangle.left).unwrap_or(0),
        height: u32::try_from(rectangle.bottom - rectangle.top).unwrap_or(0),
    }
}

fn window_title(window: *mut core::ffi::c_void) -> Option<String> {
    let mut buffer = [0_u16; 512];
    // SAFETY: the buffer is writable and its length is passed.
    let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    Some(String::from_utf16_lossy(&buffer[..length]))
}

/// The name of the program a window belongs to without the folder and `.exe` (e.g. `mpv`).
fn process_name(window: *mut core::ffi::c_void) -> Option<String> {
    let mut process_id = 0_u32;
    // SAFETY: `process_id` is writable.
    unsafe { GetWindowThreadProcessId(window, &mut process_id) };
    if process_id == 0 {
        return None;
    }
    // SAFETY: the handle is checked and closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut buffer = [0_u16; 1024];
    let mut length = buffer.len() as u32;
    // SAFETY: the buffer is writable and `length` holds its size.
    let queried =
        unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
    // SAFETY: `process` is a handle this function opened.
    unsafe { CloseHandle(process) };
    if queried == 0 {
        return None;
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length as usize]));
    Some(path.file_stem()?.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_rectangle_is_measured_from_the_origin_of_the_screen() {
        let rectangle = RECT {
            left: -1920,
            top: 100,
            right: -1000,
            bottom: 700,
        };

        assert_eq!(
            region_of(rectangle, (-1920, 0)),
            Region {
                x: 0,
                y: 100,
                width: 920,
                height: 600
            }
        );
        assert_eq!(
            region_of(rectangle, (0, 0)),
            Region {
                x: -1920,
                y: 100,
                width: 920,
                height: 600
            }
        );
    }

    #[test]
    fn a_broken_rectangle_has_no_size() {
        let rectangle = RECT {
            left: 10,
            top: 10,
            right: 5,
            bottom: 2,
        };

        let region = region_of(rectangle, (0, 0));

        assert_eq!((region.width, region.height), (0, 0));
    }

    #[test]
    fn the_pointer_can_be_read_and_no_modifier_is_held_in_a_test_run() {
        let mut pointer = WindowsPointer::connect().expect("connect");

        let reading = pointer.read().expect("read");

        let screen = virtual_screen();
        assert!(reading.x >= 0 && reading.y >= 0);
        assert!(reading.x <= screen.width as i32 && reading.y <= screen.height as i32);
        assert!(!reading.shift && !reading.control && !reading.alt);
    }

    #[test]
    fn the_window_under_the_pointer_can_be_asked_for_without_failing() {
        let mut pointer = WindowsPointer::connect().expect("connect");

        assert!(pointer.window_under_pointer().is_ok());
        assert!(pointer.identify_window_under_pointer().is_ok());
    }
}
