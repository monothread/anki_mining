//! Small image helpers that reproduce what the MeikiOCR Python pipeline does with OpenCV.

use image::RgbImage;

/// Bilinear resize without anti-aliasing, using the same half-pixel sampling as OpenCV's
/// `INTER_LINEAR`. `data` is interleaved RGB; the result has `dst_width * dst_height * 3` bytes.
pub fn resize_bilinear(
    data: &[u8],
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
) -> Vec<u8> {
    let mut output = vec![0_u8; dst_width * dst_height * 3];
    if src_width == 0 || src_height == 0 || dst_width == 0 || dst_height == 0 {
        return output;
    }

    let x_samples = axis_samples(src_width, dst_width);
    let y_samples = axis_samples(src_height, dst_height);

    for (dy, &(y0, y1, wy)) in y_samples.iter().enumerate() {
        for (dx, &(x0, x1, wx)) in x_samples.iter().enumerate() {
            for channel in 0..3 {
                let at = |x: usize, y: usize| f32::from(data[(y * src_width + x) * 3 + channel]);
                let top = at(x0, y0) * (1.0 - wx) + at(x1, y0) * wx;
                let bottom = at(x0, y1) * (1.0 - wx) + at(x1, y1) * wx;
                let value = top * (1.0 - wy) + bottom * wy;
                output[(dy * dst_width + dx) * 3 + channel] = value.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    output
}

/// For every destination index: the two source indices to blend and the weight of the second.
fn axis_samples(src: usize, dst: usize) -> Vec<(usize, usize, f32)> {
    let ratio = src as f32 / dst as f32;
    (0..dst)
        .map(|index| {
            let position = (index as f32 + 0.5) * ratio - 0.5;
            let floor = position.floor();
            let (first, weight) = if floor < 0.0 {
                (0, 0.0)
            } else if floor as usize >= src - 1 {
                (src - 1, 0.0)
            } else {
                (floor as usize, position - floor)
            };
            (first, (first + 1).min(src - 1), weight)
        })
        .collect()
}

/// Crops `[x1, x2) x [y1, y2)` (already clamped to the image) into a new interleaved RGB buffer.
pub fn crop(image: &RgbImage, x1: usize, y1: usize, x2: usize, y2: usize) -> Vec<u8> {
    let width = image.width() as usize;
    let raw = image.as_raw();
    let mut output = Vec::with_capacity((x2 - x1) * (y2 - y1) * 3);
    for y in y1..y2 {
        output.extend_from_slice(&raw[(y * width + x1) * 3..(y * width + x2) * 3]);
    }
    output
}

/// Writes interleaved RGB `pixels` (size `width` x `height`) into the top-left corner of a zero
/// padded planar tensor `[3, canvas_height, canvas_width]` holding BGR values in `0..=1`.
///
/// The MeikiOCR models were fed OpenCV images, so the channel order is blue, green, red.
pub fn to_bgr_planar(
    pixels: &[u8],
    width: usize,
    height: usize,
    canvas_width: usize,
    canvas_height: usize,
) -> Vec<f32> {
    let plane = canvas_width * canvas_height;
    let mut tensor = vec![0.0_f32; 3 * plane];
    for y in 0..height {
        for x in 0..width {
            let source = (y * width + x) * 3;
            let target = y * canvas_width + x;
            tensor[target] = f32::from(pixels[source + 2]) / 255.0;
            tensor[plane + target] = f32::from(pixels[source + 1]) / 255.0;
            tensor[2 * plane + target] = f32::from(pixels[source]) / 255.0;
        }
    }
    tensor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: usize, height: usize, rgb: [u8; 3]) -> Vec<u8> {
        (0..width * height).flat_map(|_| rgb).collect()
    }

    #[test]
    fn resizing_a_solid_color_keeps_the_color() {
        let resized = resize_bilinear(&solid(8, 6, [10, 120, 250]), 8, 6, 3, 2);

        assert_eq!(resized, solid(3, 2, [10, 120, 250]));
    }

    #[test]
    fn resizing_to_the_same_size_is_the_identity() {
        let data: Vec<u8> = (0..4 * 3 * 3)
            .map(|value| (value * 7 % 256) as u8)
            .collect();

        assert_eq!(resize_bilinear(&data, 4, 3, 4, 3), data);
    }

    #[test]
    fn downscaling_samples_with_half_pixel_centers_and_does_not_average_everything() {
        // 4x1 gradient resized to 2x1: centers fall at source positions 0.5 and 2.5.
        let data = [0, 0, 0, 100, 100, 100, 200, 200, 200, 250, 250, 250];

        let resized = resize_bilinear(&data, 4, 1, 2, 1);

        assert_eq!(resized, vec![50, 50, 50, 225, 225, 225]);
    }

    #[test]
    fn upscaling_interpolates_between_neighbours_and_clamps_at_the_borders() {
        // 2x1 -> 4x1: positions -0.25, 0.25, 0.75, 1.25.
        let data = [0, 0, 0, 200, 200, 200];

        let resized = resize_bilinear(&data, 2, 1, 4, 1);

        assert_eq!(
            resized,
            vec![0, 0, 0, 50, 50, 50, 150, 150, 150, 200, 200, 200]
        );
    }

    #[test]
    fn resizing_interpolates_vertically_per_channel() {
        // 1x2 with different channels -> 1x4.
        let data = [0, 100, 200, 200, 0, 100];

        let resized = resize_bilinear(&data, 1, 2, 1, 4);

        assert_eq!(
            resized,
            vec![0, 100, 200, 50, 75, 175, 150, 25, 125, 200, 0, 100]
        );
    }

    #[test]
    fn resizing_with_an_empty_side_returns_a_zeroed_buffer_of_the_target_size() {
        assert_eq!(resize_bilinear(&[], 0, 0, 2, 2), vec![0; 12]);
        assert_eq!(resize_bilinear(&[1, 2, 3], 1, 1, 0, 5), Vec::<u8>::new());
    }

    #[test]
    fn crop_copies_the_requested_rectangle() {
        let mut image = RgbImage::new(4, 3);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgb([x as u8, y as u8, 9]);
        }

        let cropped = crop(&image, 1, 1, 3, 3);

        assert_eq!(cropped, vec![1, 1, 9, 2, 1, 9, 1, 2, 9, 2, 2, 9]);
    }

    #[test]
    fn planar_tensor_is_bgr_normalized_and_zero_padded() {
        // One RGB pixel (255, 128, 0) on a 2x2 canvas.
        let tensor = to_bgr_planar(&[255, 128, 0], 1, 1, 2, 2);

        assert_eq!(tensor.len(), 12);
        // Blue plane, green plane, red plane; only the top-left value is set.
        assert_eq!(tensor[0], 0.0);
        assert_eq!(tensor[4], 128.0 / 255.0);
        assert_eq!(tensor[8], 1.0);
        assert!(tensor[1..4].iter().all(|value| *value == 0.0));
        assert!(tensor[5..8].iter().all(|value| *value == 0.0));
        assert!(tensor[9..12].iter().all(|value| *value == 0.0));
    }
}
