//! Screen capture on Windows with GDI: everything that is on the screen, all monitors together.

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use image::RgbImage;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS,
    HGDIOBJ, SRCCOPY,
};
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use super::{CaptureError, Region, ScreenCapture};

const BYTES_PER_PIXEL: usize = 4;

/// The rectangle that holds every monitor, in virtual-desktop coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualScreen {
    pub origin: (i32, i32),
    pub width: u32,
    pub height: u32,
}

/// Makes the process see real pixels on screens that are scaled (125%, 150%...); without it
/// Windows would hand out scaled coordinates and blurred screenshots. Harmless when the process
/// is already aware.
pub fn make_dpi_aware() {
    // SAFETY: a plain call with a documented constant; failing only means it was already set.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn virtual_screen() -> VirtualScreen {
    // SAFETY: GetSystemMetrics has no preconditions.
    let (x, y, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    VirtualScreen {
        origin: (x, y),
        width: u32::try_from(width).unwrap_or(0),
        height: u32::try_from(height).unwrap_or(0),
    }
}

/// Captures the whole desktop. Positions are measured from the top-left corner of the area that
/// holds all the monitors.
pub struct WindowsCapture {
    screen: VirtualScreen,
}

impl WindowsCapture {
    pub fn connect() -> Result<WindowsCapture, CaptureError> {
        make_dpi_aware();
        let screen = virtual_screen();
        if screen.width == 0 || screen.height == 0 {
            return Err(CaptureError::Connection(
                "Windows reports a screen with no size".to_string(),
            ));
        }
        Ok(WindowsCapture { screen })
    }
}

impl ScreenCapture for WindowsCapture {
    fn screen_size(&self) -> (u32, u32) {
        (self.screen.width, self.screen.height)
    }

    fn capture(&mut self, region: Region) -> Result<RgbImage, CaptureError> {
        let visible = region
            .clamp_to(self.screen.width, self.screen.height)
            .ok_or(CaptureError::OutsideScreen(region))?;
        let pixels = copy_screen(
            self.screen.origin.0 + visible.x,
            self.screen.origin.1 + visible.y,
            visible.width,
            visible.height,
        )?;
        Ok(rgb_from_bgra(&pixels, visible.width, visible.height))
    }
}

/// The pixels (blue, green, red, unused) of the screen rectangle at `(x, y)`.
fn copy_screen(x: i32, y: i32, width: u32, height: u32) -> Result<Vec<u8>, CaptureError> {
    let (w, h) = (width as i32, height as i32);
    let mut pixels = vec![0_u8; width as usize * height as usize * BYTES_PER_PIXEL];
    // SAFETY: every handle is checked, used only here and released before returning; the buffer
    // is `width * height * 4` bytes, which is what GetDIBits writes for a 32-bit bitmap.
    unsafe {
        let screen_dc = GetDC(null_mut());
        if screen_dc.is_null() {
            return Err(CaptureError::Failed("GetDC returned no device".to_string()));
        }
        let memory_dc = CreateCompatibleDC(screen_dc);
        let bitmap = CreateCompatibleBitmap(screen_dc, w, h);
        let outcome = if memory_dc.is_null() || bitmap.is_null() {
            Err(CaptureError::Failed("cannot create the bitmap".to_string()))
        } else {
            let previous = SelectObject(memory_dc, bitmap as HGDIOBJ);
            let copied = BitBlt(memory_dc, 0, 0, w, h, screen_dc, x, y, SRCCOPY | CAPTUREBLT);
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                // Negative: the rows come top to bottom.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let rows = if copied == 0 {
                0
            } else {
                GetDIBits(
                    memory_dc,
                    bitmap,
                    0,
                    height,
                    pixels.as_mut_ptr().cast(),
                    &mut info,
                    DIB_RGB_COLORS,
                )
            };
            SelectObject(memory_dc, previous);
            if copied == 0 {
                Err(CaptureError::Failed("BitBlt failed".to_string()))
            } else if rows == 0 {
                Err(CaptureError::Failed("GetDIBits failed".to_string()))
            } else {
                Ok(())
            }
        };
        if !bitmap.is_null() {
            DeleteObject(bitmap as HGDIOBJ);
        }
        if !memory_dc.is_null() {
            DeleteDC(memory_dc);
        }
        ReleaseDC(null_mut(), screen_dc);
        outcome?;
    }
    Ok(pixels)
}

/// Turns 32-bit blue-green-red-unused pixels into an RGB image.
pub fn rgb_from_bgra(pixels: &[u8], width: u32, height: u32) -> RgbImage {
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for pixel in pixels.chunks_exact(BYTES_PER_PIXEL) {
        rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
    }
    RgbImage::from_raw(width, height, rgb).unwrap_or_else(|| RgbImage::new(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_green_red_pixels_become_red_green_blue() {
        let image = rgb_from_bgra(&[10, 20, 30, 255, 1, 2, 3, 0], 2, 1);

        assert_eq!(image.get_pixel(0, 0).0, [30, 20, 10]);
        assert_eq!(image.get_pixel(1, 0).0, [3, 2, 1]);
    }

    #[test]
    fn the_desktop_has_a_size_and_a_region_of_it_can_be_captured() {
        let mut capture = WindowsCapture::connect().expect("connect");
        let (width, height) = capture.screen_size();

        let image = capture
            .capture(Region {
                x: 0,
                y: 0,
                width: width.min(64),
                height: height.min(48),
            })
            .expect("capture");

        assert!(width > 0 && height > 0);
        assert_eq!(image.dimensions(), (width.min(64), height.min(48)));
    }

    #[test]
    fn a_region_outside_the_desktop_is_an_error() {
        let mut capture = WindowsCapture::connect().expect("connect");
        let (width, _) = capture.screen_size();
        let region = Region {
            x: width as i32 + 100,
            y: 0,
            width: 10,
            height: 10,
        };

        assert_eq!(
            capture.capture(region),
            Err(CaptureError::OutsideScreen(region))
        );
    }
}
