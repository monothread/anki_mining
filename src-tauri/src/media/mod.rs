//! Media attached to cards: the screenshot of the window a word was found in.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{ExtendedColorType, ImageEncoder, RgbImage};
use thiserror::Error;

use crate::capture::Region;
use crate::cards::MediaFile;

/// Screenshots wider than this are scaled down; cards do not need more.
pub const MAX_SCREENSHOT_WIDTH: u32 = 1280;
pub const JPEG_QUALITY: u8 = 80;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("cannot encode the screenshot: {0}")]
    Encode(String),
}

/// The part of `image` that lies inside `region` (in image pixels); `None` if nothing does.
pub fn crop_to_region(image: &RgbImage, region: Region) -> Option<RgbImage> {
    let visible = region.clamp_to(image.width(), image.height())?;
    Some(
        imageops::crop_imm(
            image,
            visible.x as u32,
            visible.y as u32,
            visible.width,
            visible.height,
        )
        .to_image(),
    )
}

/// Scales the image down to at most [`MAX_SCREENSHOT_WIDTH`] wide, keeping the aspect ratio.
pub fn fit_to_card(image: &RgbImage) -> RgbImage {
    if image.width() <= MAX_SCREENSHOT_WIDTH {
        return image.clone();
    }
    let height = (u64::from(image.height()) * u64::from(MAX_SCREENSHOT_WIDTH)
        / u64::from(image.width()))
    .max(1) as u32;
    imageops::resize(image, MAX_SCREENSHOT_WIDTH, height, FilterType::Triangle)
}

/// The screenshot as a JPEG, scaled down if it is large.
pub fn encode_screenshot(image: &RgbImage) -> Result<Vec<u8>, MediaError> {
    let fitted = fit_to_card(image);
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)
        .write_image(
            fitted.as_raw(),
            fitted.width(),
            fitted.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| MediaError::Encode(error.to_string()))?;
    Ok(bytes)
}

/// The screenshot ready to be sent to Anki: a JPEG named after the moment it was taken, base64
/// encoded.
pub fn screenshot_file(image: &RgbImage, timestamp_ms: u64) -> Result<MediaFile, MediaError> {
    Ok(MediaFile {
        filename: format!("anki-mining-{timestamp_ms}.jpg"),
        data: STANDARD.encode(encode_screenshot(image)?),
    })
}

#[cfg(test)]
mod tests {
    use image::Rgb;

    use super::*;

    fn gradient(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        })
    }

    fn decode(bytes: &[u8]) -> RgbImage {
        image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
            .expect("valid jpeg")
            .to_rgb8()
    }

    #[test]
    fn crop_returns_the_requested_part() {
        let source = gradient(200, 100);

        let part = crop_to_region(
            &source,
            Region {
                x: 50,
                y: 20,
                width: 40,
                height: 30,
            },
        )
        .expect("part");

        assert_eq!((part.width(), part.height()), (40, 30));
        assert_eq!(part.get_pixel(0, 0), source.get_pixel(50, 20));
        assert_eq!(part.get_pixel(39, 29), source.get_pixel(89, 49));
    }

    #[test]
    fn crop_clips_a_region_that_sticks_out_of_the_image() {
        let source = gradient(200, 100);

        let part = crop_to_region(
            &source,
            Region {
                x: 180,
                y: -10,
                width: 100,
                height: 50,
            },
        )
        .expect("part");

        assert_eq!((part.width(), part.height()), (20, 40));
        assert_eq!(part.get_pixel(0, 0), source.get_pixel(180, 0));
    }

    #[test]
    fn crop_of_a_region_outside_the_image_is_none() {
        let source = gradient(200, 100);

        assert!(crop_to_region(
            &source,
            Region {
                x: 300,
                y: 0,
                width: 50,
                height: 50
            }
        )
        .is_none());
        assert!(crop_to_region(
            &source,
            Region {
                x: 0,
                y: 0,
                width: 0,
                height: 50
            }
        )
        .is_none());
    }

    #[test]
    fn small_images_keep_their_size() {
        let fitted = fit_to_card(&gradient(640, 360));

        assert_eq!((fitted.width(), fitted.height()), (640, 360));
    }

    #[test]
    fn an_image_exactly_at_the_limit_is_not_scaled() {
        let fitted = fit_to_card(&gradient(1280, 720));

        assert_eq!((fitted.width(), fitted.height()), (1280, 720));
    }

    #[test]
    fn large_images_are_scaled_down_keeping_the_aspect_ratio() {
        let fitted = fit_to_card(&gradient(1920, 1080));

        assert_eq!((fitted.width(), fitted.height()), (1280, 720));
    }

    #[test]
    fn extreme_aspect_ratios_never_get_a_zero_height() {
        let fitted = fit_to_card(&gradient(5000, 1));

        assert_eq!((fitted.width(), fitted.height()), (1280, 1));
    }

    #[test]
    fn the_encoded_screenshot_is_a_jpeg_of_the_fitted_size() {
        let bytes = encode_screenshot(&gradient(1920, 1080)).expect("encode");

        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF]);
        let decoded = decode(&bytes);
        assert_eq!((decoded.width(), decoded.height()), (1280, 720));
    }

    #[test]
    fn the_encoded_screenshot_resembles_the_original() {
        let original = RgbImage::from_pixel(320, 200, Rgb([200, 40, 40]));

        let decoded = decode(&encode_screenshot(&original).expect("encode"));

        let pixel = decoded.get_pixel(160, 100).0;
        assert!(
            pixel[0] > 180 && pixel[1] < 70 && pixel[2] < 70,
            "{pixel:?}"
        );
    }

    #[test]
    fn the_file_has_a_timestamped_name_and_base64_data_that_decodes_to_the_jpeg() {
        let file = screenshot_file(&gradient(100, 50), 1_760_400_000_123).expect("file");

        assert_eq!(file.filename, "anki-mining-1760400000123.jpg");
        let bytes = STANDARD.decode(&file.data).expect("base64");
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        assert_eq!(decode(&bytes).width(), 100);
    }

    #[test]
    fn an_image_without_pixels_cannot_be_encoded() {
        let error = encode_screenshot(&RgbImage::new(0, 0)).expect_err("must fail");

        assert!(error
            .to_string()
            .starts_with("cannot encode the screenshot:"));
    }
}
