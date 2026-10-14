use image::RgbImage;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, ImageOrder};
use x11rb::rust_connection::RustConnection;

use super::{CaptureError, Region, ScreenCapture};

const BYTES_PER_PIXEL: usize = 4;

/// Captures the root window of an X11 display (everything that is on screen).
pub struct X11Capture {
    connection: RustConnection,
    root: u32,
    width: u32,
    height: u32,
    /// Byte positions of red, green and blue inside one 32-bit pixel.
    channel_offsets: [usize; 3],
}

impl X11Capture {
    /// Connects to the display named by `$DISPLAY`.
    pub fn connect() -> Result<X11Capture, CaptureError> {
        let (connection, screen_number) =
            x11rb::connect(None).map_err(|error| CaptureError::Connection(error.to_string()))?;
        let setup = connection.setup();
        let screen = setup
            .roots
            .get(screen_number)
            .ok_or_else(|| CaptureError::Connection("the screen does not exist".to_string()))?;

        let format = setup
            .pixmap_formats
            .iter()
            .find(|format| format.depth == screen.root_depth)
            .ok_or_else(|| {
                CaptureError::Unsupported("no pixmap format for the root depth".to_string())
            })?;
        if format.bits_per_pixel != 32 {
            return Err(CaptureError::Unsupported(format!(
                "{} bits per pixel (only 32 is supported)",
                format.bits_per_pixel
            )));
        }
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|depth| depth.visuals.iter())
            .find(|visual| visual.visual_id == screen.root_visual)
            .ok_or_else(|| {
                CaptureError::Unsupported("the root visual was not found".to_string())
            })?;

        let channel_offsets = channel_offsets(
            [visual.red_mask, visual.green_mask, visual.blue_mask],
            setup.image_byte_order == ImageOrder::MSB_FIRST,
        )?;
        Ok(X11Capture {
            root: screen.root,
            width: u32::from(screen.width_in_pixels),
            height: u32::from(screen.height_in_pixels),
            channel_offsets,
            connection,
        })
    }
}

impl ScreenCapture for X11Capture {
    fn screen_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn capture(&mut self, region: Region) -> Result<RgbImage, CaptureError> {
        let visible = region
            .clamp_to(self.width, self.height)
            .ok_or(CaptureError::OutsideScreen(region))?;
        let reply = self
            .connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.root,
                visible.x as i16,
                visible.y as i16,
                visible.width as u16,
                visible.height as u16,
                u32::MAX,
            )
            .map_err(|error| CaptureError::Failed(error.to_string()))?
            .reply()
            .map_err(|error| CaptureError::Failed(error.to_string()))?;

        convert(
            &reply.data,
            visible.width,
            visible.height,
            self.channel_offsets,
        )
    }
}

/// Maps the channel masks of a 32-bit true-color visual to the byte index of each channel.
fn channel_offsets(
    masks: [u32; 3],
    most_significant_first: bool,
) -> Result<[usize; 3], CaptureError> {
    let mut offsets = [0; 3];
    for (offset, mask) in offsets.iter_mut().zip(masks) {
        let byte = match mask {
            0xff => 0,
            0xff00 => 1,
            0xff_0000 => 2,
            0xff00_0000 => 3,
            other => {
                return Err(CaptureError::Unsupported(format!(
                    "unusual color mask {other:#x}"
                )))
            }
        };
        *offset = if most_significant_first {
            3 - byte
        } else {
            byte
        };
    }
    Ok(offsets)
}

/// Converts the 32-bit pixels returned by X11 to an RGB image.
fn convert(
    data: &[u8],
    width: u32,
    height: u32,
    offsets: [usize; 3],
) -> Result<RgbImage, CaptureError> {
    let (width, height) = (width as usize, height as usize);
    // Rows are padded to a multiple of 4 bytes, which is always true for 32-bit pixels.
    let expected = width * height * BYTES_PER_PIXEL;
    if data.len() < expected {
        return Err(CaptureError::Failed(format!(
            "the server returned {} bytes, expected {expected}",
            data.len()
        )));
    }

    let mut pixels = Vec::with_capacity(width * height * 3);
    for pixel in data[..expected].as_chunks::<BYTES_PER_PIXEL>().0 {
        pixels.extend_from_slice(&[pixel[offsets[0]], pixel[offsets[1]], pixel[offsets[2]]]);
    }
    RgbImage::from_raw(width as u32, height as u32, pixels)
        .ok_or_else(|| CaptureError::Failed("the pixel buffer does not match the size".to_string()))
}

#[cfg(test)]
mod tests {
    use x11rb::protocol::xproto::{CreateWindowAux, WindowClass};
    use x11rb::COPY_DEPTH_FROM_PARENT;

    use super::*;

    #[test]
    fn little_endian_bgrx_masks_give_the_usual_offsets() {
        assert_eq!(
            channel_offsets([0xff_0000, 0xff00, 0xff], false),
            Ok([2, 1, 0])
        );
    }

    #[test]
    fn rgb_masks_and_big_endian_servers_are_handled() {
        assert_eq!(
            channel_offsets([0xff, 0xff00, 0xff_0000], false),
            Ok([0, 1, 2])
        );
        assert_eq!(
            channel_offsets([0xff_0000, 0xff00, 0xff], true),
            Ok([1, 2, 3])
        );
        assert_eq!(
            channel_offsets([0xff00_0000, 0xff_0000, 0xff00], false),
            Ok([3, 2, 1])
        );
    }

    #[test]
    fn unusual_masks_are_rejected() {
        let error = channel_offsets([0xf800, 0x7e0, 0x1f], false).expect_err("must fail");

        assert_eq!(
            error,
            CaptureError::Unsupported("unusual color mask 0xf800".to_string())
        );
    }

    #[test]
    fn converts_bgrx_pixels_to_rgb() {
        // Two pixels: blue=1 green=2 red=3 and blue=4 green=5 red=6, plus the unused byte.
        let image = convert(&[1, 2, 3, 0, 4, 5, 6, 0], 2, 1, [2, 1, 0]).expect("convert");

        assert_eq!(image.as_raw(), &vec![3, 2, 1, 6, 5, 4]);
        assert_eq!((image.width(), image.height()), (2, 1));
    }

    #[test]
    fn short_data_is_reported() {
        let error = convert(&[0; 7], 2, 1, [2, 1, 0]).expect_err("must fail");

        assert_eq!(
            error,
            CaptureError::Failed("the server returned 7 bytes, expected 8".to_string())
        );
    }

    /// Opens a window of a known color on the test display and returns its connection.
    fn paint_window(x: i16, y: i16, width: u16, height: u16, color: u32) -> RustConnection {
        let (connection, screen_number) = x11rb::connect(None).expect("display");
        let screen = connection.setup().roots[screen_number].clone();
        let window = connection.generate_id().expect("id");
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                screen.root,
                x,
                y,
                width,
                height,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new()
                    .background_pixel(color)
                    .override_redirect(1),
            )
            .expect("create window");
        connection.map_window(window).expect("map");
        connection
            .get_input_focus()
            .expect("sync")
            .reply()
            .expect("sync reply");
        connection
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn captures_the_pixels_of_a_window_on_screen() {
        let _display = crate::x11_test_lock();
        let _window = paint_window(40, 30, 120, 60, 0x00_33_66_99);
        let mut capture = X11Capture::connect().expect("connect");

        let image = capture
            .capture(Region {
                x: 50,
                y: 40,
                width: 50,
                height: 20,
            })
            .expect("capture");

        assert_eq!((image.width(), image.height()), (50, 20));
        assert!(image.pixels().all(|pixel| pixel.0 == [0x33, 0x66, 0x99]));
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn reports_the_screen_size_and_clips_regions_that_stick_out() {
        let _display = crate::x11_test_lock();
        let mut capture = X11Capture::connect().expect("connect");
        let (width, height) = capture.screen_size();

        let image = capture
            .capture(Region {
                x: width as i32 - 10,
                y: height as i32 - 5,
                width: 100,
                height: 100,
            })
            .expect("capture");

        assert!(width > 0 && height > 0);
        assert_eq!((image.width(), image.height()), (10, 5));
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn a_region_outside_the_screen_is_an_error() {
        let _display = crate::x11_test_lock();
        let mut capture = X11Capture::connect().expect("connect");
        let (width, _) = capture.screen_size();
        let region = Region {
            x: width as i32,
            y: 0,
            width: 10,
            height: 10,
        };

        assert_eq!(
            capture.capture(region),
            Err(CaptureError::OutsideScreen(region))
        );
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn connecting_fails_clearly_when_the_display_does_not_exist() {
        let _display = crate::x11_test_lock();
        // SAFETY: tests that read DISPLAY are the ignored ones above, which run in a separate
        // invocation per `--test-threads`; restoring the value keeps them unaffected.
        let original = std::env::var("DISPLAY").ok();
        std::env::set_var("DISPLAY", ":987");

        let result = X11Capture::connect();

        match original {
            Some(value) => std::env::set_var("DISPLAY", value),
            None => std::env::remove_var("DISPLAY"),
        }
        assert!(matches!(result, Err(CaptureError::Connection(_))));
    }
}
