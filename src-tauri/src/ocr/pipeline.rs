//! The model-independent parts of the MeikiOCR pipeline, ported from the Python reference
//! (https://github.com/rtr46/meikiocr, Apache-2.0): input preparation and output decoding.

use image::RgbImage;

use super::layout::is_vertical;
use super::raster::{crop, resize_bilinear, to_bgr_planar};
use super::{OcrChar, OcrLine};

pub const DET_WIDTH: usize = 960;
pub const DET_HEIGHT: usize = 544;
pub const REC_WIDTH: usize = 960;
pub const REC_HEIGHT: usize = 32;
/// The recognition model emits at most this many characters per line.
pub const REC_MAX_CHARS: usize = 48;
/// The detection model emits at most this many boxes per image.
pub const DET_MAX_BOXES: usize = 64;

/// The recognition model reads about this many characters per line, so longer lines are cut
/// into segments no wider than this many times their height (full-width characters are about as
/// wide as they are tall).
pub const SEGMENT_ASPECT: i32 = 26;
/// Neighbouring segments share this many line heights so no character is lost at a cut.
pub const SEGMENT_OVERLAP_ASPECT: i32 = 4;

/// Vertical recognition canvas: columns are scaled to this width and at most this tall.
pub const VREC_WIDTH: usize = 32;
pub const VREC_HEIGHT: usize = 480;
/// The vertical recognition model emits at most this many characters per segment.
pub const VREC_MAX_CHARS: usize = 24;
/// Height of the content of a segment when a long column has to be split (the rest is padding).
pub const VREC_SPLIT_CONTENT_HEIGHT: usize = 420;
/// Neighbouring vertical segments share this many pixels of the scaled canvas.
pub const VREC_OVERLAP: usize = 64;

const OVERLAP_THRESHOLD: f32 = 0.3;
const EPSILON: f32 = 1e-6;

/// Two-character words the recognition model tends to emit in reverse order; swapping the two
/// characters of each entry gives the correct word.
const SWAPPED_PAIRS: [&str; 8] = [
    "儡傀", "談冗", "汰淘", "沱滂", "攣痙", "酊酩", "麭麺", "哭慟",
];

/// A detection model input and the factor used to map boxes back to the original image.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectionInput {
    /// Planar BGR tensor `[3, DET_HEIGHT, DET_WIDTH]`.
    pub tensor: Vec<f32>,
    /// Value of the second model input: the original size the boxes are expressed in.
    pub target_size: [i64; 2],
}

/// Fits the image inside the detection canvas keeping its aspect ratio (padding bottom/right).
pub fn prepare_detection(image: &RgbImage) -> DetectionInput {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let scale = f64::min(
        DET_WIDTH as f64 / width as f64,
        DET_HEIGHT as f64 / height as f64,
    );
    let resized_width = ((width as f64 * scale) as usize).max(1);
    let resized_height = ((height as f64 * scale) as usize).max(1);

    let resized = resize_bilinear(image.as_raw(), width, height, resized_width, resized_height);
    DetectionInput {
        tensor: to_bgr_planar(
            &resized,
            resized_width,
            resized_height,
            DET_WIDTH,
            DET_HEIGHT,
        ),
        target_size: [
            (DET_WIDTH as f64 / scale) as i64,
            (DET_HEIGHT as f64 / scale) as i64,
        ],
    }
}

/// Keeps boxes above `threshold`, clamps them to the image and sorts them top to bottom.
///
/// `boxes` is `[DET_MAX_BOXES * 4]` (x1, y1, x2, y2 in original image pixels) and `scores`
/// `[DET_MAX_BOXES]`. Degenerate boxes (zero width or height) are dropped.
pub fn decode_detection(
    boxes: &[f32],
    scores: &[f32],
    image_width: u32,
    image_height: u32,
    threshold: f32,
) -> Vec<[i32; 4]> {
    let max_x = image_width as f32;
    let max_y = image_height as f32;
    let mut found: Vec<[i32; 4]> = scores
        .iter()
        .zip(boxes.as_chunks::<4>().0)
        .filter(|(score, _)| **score > threshold)
        .map(|(_, coordinates)| {
            [
                coordinates[0].clamp(0.0, max_x) as i32,
                coordinates[1].clamp(0.0, max_y) as i32,
                coordinates[2].clamp(0.0, max_x) as i32,
                coordinates[3].clamp(0.0, max_y) as i32,
            ]
        })
        .filter(|bbox| bbox[2] > bbox[0] && bbox[3] > bbox[1])
        .collect();
    found.sort_by_key(|bbox| bbox[1]);
    found
}

/// A detected line prepared for recognition: one segment for a normal line, several overlapping
/// ones for a long line.
#[derive(Debug, Clone, PartialEq)]
pub struct LineInput {
    /// Position of the whole line in the original image: `[x1, y1, x2, y2]`.
    pub bbox: [i32; 4],
    /// Whether the text runs top to bottom (a column).
    pub vertical: bool,
    pub segments: Vec<RecognitionInput>,
}

/// Splits a line box into overlapping boxes the recognition model can read. A line that is not
/// longer than `SEGMENT_ASPECT` times its height stays whole.
pub fn segment_boxes(bbox: [i32; 4]) -> Vec<[i32; 4]> {
    let [x1, y1, x2, y2] = bbox;
    let (width, height) = (x2 - x1, y2 - y1);
    let max_width = (height * SEGMENT_ASPECT).max(1);
    if width <= max_width {
        return vec![bbox];
    }

    let stride = (max_width - height * SEGMENT_OVERLAP_ASPECT).max(1);
    let mut starts = Vec::new();
    let mut start = x1;
    while start + max_width < x2 {
        starts.push(start);
        start += stride;
    }
    starts.push(x2 - max_width);
    starts
        .into_iter()
        .map(|left| [left, y1, left + max_width, y2])
        .collect()
}

/// Prepares every segment of a detected line or column; `None` if no segment can be read.
pub fn prepare_line(image: &RgbImage, bbox: [i32; 4]) -> Option<LineInput> {
    let segments: Vec<RecognitionInput> = if is_vertical(bbox) {
        let (boxes, max_height) = vertical_segment_boxes(bbox);
        boxes
            .into_iter()
            .filter_map(|segment| prepare_vertical_recognition(image, segment, max_height))
            .collect()
    } else {
        segment_boxes(bbox)
            .into_iter()
            .filter_map(|segment| prepare_recognition(image, segment))
            .collect()
    };
    if segments.is_empty() {
        None
    } else {
        Some(LineInput {
            bbox,
            vertical: is_vertical(bbox),
            segments,
        })
    }
}

/// Splits a column box into overlapping boxes the vertical model can read, and returns the
/// largest height (in scaled canvas pixels) the content of a segment may have. A column whose
/// scaled height fits the canvas stays whole.
pub fn vertical_segment_boxes(bbox: [i32; 4]) -> (Vec<[i32; 4]>, usize) {
    let [x1, y1, x2, y2] = bbox;
    let scale = VREC_WIDTH as f64 / f64::from(x2 - x1).max(1.0);
    if f64::from(y2 - y1) * scale <= VREC_HEIGHT as f64 {
        return (vec![bbox], VREC_HEIGHT);
    }

    let segment_height = VREC_SPLIT_CONTENT_HEIGHT as f64 / scale;
    let stride = (VREC_SPLIT_CONTENT_HEIGHT - VREC_OVERLAP) as f64 / scale;
    let mut starts: Vec<f64> = Vec::new();
    let mut current = f64::from(y1);
    while current + segment_height < f64::from(y2) {
        starts.push(current);
        current += stride;
    }
    let last = f64::from(y2) - segment_height;
    if starts
        .last()
        .is_none_or(|first_of_last| last > first_of_last + 1.0)
    {
        starts.push(last);
    }

    let boxes = starts
        .into_iter()
        .map(|start| {
            let top = round_half_even(start) as i32;
            let bottom = (round_half_even(start + segment_height) as i32).min(y2);
            [x1, top, x2, bottom]
        })
        .collect();
    (boxes, VREC_SPLIT_CONTENT_HEIGHT)
}

/// A text line cropped and sized for the recognition model.
#[derive(Debug, Clone, PartialEq)]
pub struct RecognitionInput {
    /// Planar BGR tensor: `[3, REC_HEIGHT, REC_WIDTH]` for a line, `[3, VREC_HEIGHT, VREC_WIDTH]`
    /// for a vertical column.
    pub tensor: Vec<f32>,
    /// Size in canvas pixels of the part that holds the text along its direction: the width for a
    /// line, the height for a column (the rest is padding).
    pub effective_length: usize,
    /// Position of the segment in the original image: `[x1, y1, x2, y2]`.
    pub bbox: [i32; 4],
    /// Whether the text runs top to bottom.
    pub vertical: bool,
}

/// Crops a detected line and scales it to the recognition height. `None` for lines too small to
/// produce a single pixel.
pub fn prepare_recognition(image: &RgbImage, bbox: [i32; 4]) -> Option<RecognitionInput> {
    let [x1, y1, x2, y2] = bbox;
    let (crop_width, crop_height) = ((x2 - x1) as usize, (y2 - y1) as usize);
    if crop_width == 0 || crop_height == 0 {
        return None;
    }

    let mut new_height = REC_HEIGHT;
    let mut new_width = round_half_even(crop_width as f64 * REC_HEIGHT as f64 / crop_height as f64);
    if new_width > REC_WIDTH {
        let shrink = REC_WIDTH as f64 / new_width as f64;
        new_width = REC_WIDTH;
        new_height = round_half_even(REC_HEIGHT as f64 * shrink);
    }
    if new_width == 0 || new_height == 0 {
        return None;
    }

    let pixels = crop(image, x1 as usize, y1 as usize, x2 as usize, y2 as usize);
    let resized = resize_bilinear(&pixels, crop_width, crop_height, new_width, new_height);
    Some(RecognitionInput {
        tensor: to_bgr_planar(&resized, new_width, new_height, REC_WIDTH, REC_HEIGHT),
        effective_length: new_width,
        bbox,
        vertical: false,
    })
}

/// Crops a column and scales it to the vertical model width. `max_height` is the largest height
/// of the scaled content (see [`vertical_segment_boxes`]). `None` if nothing is left.
pub fn prepare_vertical_recognition(
    image: &RgbImage,
    bbox: [i32; 4],
    max_height: usize,
) -> Option<RecognitionInput> {
    let [x1, y1, x2, y2] = bbox;
    let (crop_width, crop_height) = ((x2 - x1) as usize, (y2 - y1) as usize);
    if crop_width == 0 || crop_height == 0 {
        return None;
    }

    let scale = VREC_WIDTH as f64 / crop_width as f64;
    let new_height = round_half_even(crop_height as f64 * scale).min(max_height);
    if new_height == 0 {
        return None;
    }

    let pixels = crop(image, x1 as usize, y1 as usize, x2 as usize, y2 as usize);
    let resized = resize_bilinear(&pixels, crop_width, crop_height, VREC_WIDTH, new_height);
    Some(RecognitionInput {
        tensor: to_bgr_planar(&resized, VREC_WIDTH, new_height, VREC_WIDTH, VREC_HEIGHT),
        effective_length: new_height,
        bbox,
        vertical: true,
    })
}

/// Rounds like Python's `round`: halves go to the even neighbour.
fn round_half_even(value: f64) -> usize {
    value.round_ties_even() as usize
}

/// A recognized character before overlapping duplicates are removed.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    character: char,
    bbox: [i32; 4],
    confidence: f32,
}

/// Turns the raw recognition output of one segment into character candidates.
///
/// `labels`, `boxes` (x1, y1, x2, y2 in the model canvas) and `scores` hold `REC_MAX_CHARS`
/// entries for a line or `VREC_MAX_CHARS` for a column. Low scores and characters that fall in
/// the padding are dropped.
pub fn segment_candidates(
    input: &RecognitionInput,
    labels: &[i32],
    boxes: &[f32],
    scores: &[f32],
    threshold: f32,
) -> Vec<Candidate> {
    let [x1, y1, x2, y2] = input.bbox;
    let (segment_width, segment_height) = ((x2 - x1) as f32, (y2 - y1) as f32);
    let effective_length = input.effective_length as f32;

    let mut candidates = Vec::new();
    for ((label, coordinates), score) in labels.iter().zip(boxes.as_chunks::<4>().0).zip(scores) {
        if *score < threshold {
            continue;
        }
        let Some(character) = u32::try_from(*label).ok().and_then(char::from_u32) else {
            continue;
        };
        let (left, top, right, bottom) = (
            coordinates[0],
            coordinates[1],
            coordinates[2],
            coordinates[3],
        );
        let bbox = if input.vertical {
            if top >= effective_length {
                continue;
            }
            let (top, bottom) = (top.min(effective_length), bottom.min(effective_length));
            let mapped = [
                x1 + (left / VREC_WIDTH as f32 * segment_width) as i32,
                y1 + (top / effective_length * segment_height) as i32,
                x1 + (right / VREC_WIDTH as f32 * segment_width) as i32,
                y1 + (bottom / effective_length * segment_height) as i32,
            ];
            if mapped[3] <= mapped[1] {
                continue;
            }
            mapped
        } else {
            if left >= effective_length {
                continue;
            }
            let (left, right) = (left.min(effective_length), right.min(effective_length));
            [
                x1 + (left / effective_length * segment_width) as i32,
                y1 + (top / REC_HEIGHT as f32 * segment_height) as i32,
                x1 + (right / effective_length * segment_width) as i32,
                y1 + (bottom / REC_HEIGHT as f32 * segment_height) as i32,
            ]
        };
        candidates.push(Candidate {
            character,
            bbox,
            confidence: *score,
        });
    }
    candidates
}

/// Builds the line from the candidates of all its segments: overlapping duplicates are
/// suppressed (the most confident wins) and the characters are put in reading order (left to
/// right, or top to bottom for a column).
pub fn assemble_line(bbox: [i32; 4], candidates: Vec<Candidate>, vertical: bool) -> OcrLine {
    let chars: Vec<OcrChar> = suppress_overlaps(candidates, vertical)
        .into_iter()
        .map(|candidate| OcrChar {
            character: candidate.character,
            bbox: candidate.bbox,
            confidence: candidate.confidence,
        })
        .collect();
    build_line(bbox, chars, vertical)
}

/// Turns the raw recognition output of one line that was read as a single segment into text.
pub fn decode_recognition(
    input: &RecognitionInput,
    labels: &[i32],
    boxes: &[f32],
    scores: &[f32],
    threshold: f32,
) -> OcrLine {
    assemble_line(
        input.bbox,
        segment_candidates(input, labels, boxes, scores, threshold),
        input.vertical,
    )
}

/// Greedy non-maximum suppression along the reading axis (horizontal, or vertical for a column),
/// returned in reading order.
fn suppress_overlaps(mut candidates: Vec<Candidate>, vertical: bool) -> Vec<Candidate> {
    let (from, to) = if vertical { (1, 3) } else { (0, 2) };
    candidates.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    let mut accepted: Vec<Candidate> = Vec::new();

    for candidate in candidates {
        let (start, end) = (candidate.bbox[from], candidate.bbox[to]);
        let overlaps = accepted.iter().any(|kept| {
            let (kept_start, kept_end) = (kept.bbox[from], kept.bbox[to]);
            if start >= kept_end || kept_start >= end {
                return false;
            }
            let intersection = (end.min(kept_end) - start.max(kept_start)).max(0) as f32;
            let shortest =
                ((end - start) as f32 + EPSILON).min((kept_end - kept_start) as f32 + EPSILON);
            intersection / shortest > OVERLAP_THRESHOLD
        });
        if !overlaps {
            accepted.push(candidate);
        }
    }
    accepted.sort_by_key(|candidate| candidate.bbox[from]);
    accepted
}

fn build_line(bbox: [i32; 4], mut chars: Vec<OcrChar>, vertical: bool) -> OcrLine {
    let mut text: String = chars.iter().map(|ocr_char| ocr_char.character).collect();
    for wrong in SWAPPED_PAIRS {
        if let Some(byte_index) = text.find(wrong) {
            let index = text[..byte_index].chars().count();
            if index + 1 < chars.len() {
                chars.swap(index, index + 1);
                text = chars.iter().map(|ocr_char| ocr_char.character).collect();
            }
        }
    }
    OcrLine {
        text,
        bbox,
        chars,
        vertical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white_image(width: u32, height: u32) -> RgbImage {
        RgbImage::from_pixel(width, height, image::Rgb([255, 255, 255]))
    }

    fn input(effective_width: usize, bbox: [i32; 4]) -> RecognitionInput {
        RecognitionInput {
            tensor: vec![],
            effective_length: effective_width,
            bbox,
            vertical: false,
        }
    }

    /// Builds the flat model output arrays from `(label, [x1, y1, x2, y2], score)` entries,
    /// padding with zero-score entries up to the model's fixed size.
    fn raw_output(entries: &[(char, [f32; 4], f32)]) -> (Vec<i32>, Vec<f32>, Vec<f32>) {
        let mut labels = Vec::new();
        let mut boxes = Vec::new();
        let mut scores = Vec::new();
        for (character, coordinates, score) in entries {
            labels.push(*character as i32);
            boxes.extend_from_slice(coordinates);
            scores.push(*score);
        }
        while labels.len() < REC_MAX_CHARS {
            labels.push(0);
            boxes.extend_from_slice(&[0.0; 4]);
            scores.push(0.0);
        }
        (labels, boxes, scores)
    }

    fn decode(
        entries: &[(char, [f32; 4], f32)],
        effective_width: usize,
        bbox: [i32; 4],
        threshold: f32,
    ) -> OcrLine {
        let (labels, boxes, scores) = raw_output(entries);
        decode_recognition(
            &input(effective_width, bbox),
            &labels,
            &boxes,
            &scores,
            threshold,
        )
    }

    #[test]
    fn detection_of_a_full_hd_frame_halves_it_and_pads_the_bottom() {
        let prepared = prepare_detection(&white_image(1920, 1080));

        assert_eq!(prepared.tensor.len(), 3 * DET_HEIGHT * DET_WIDTH);
        assert_eq!(prepared.target_size, [1920, 1088]);
        // 1920x1080 at scale 0.5 is 960x540: rows 540..544 are padding, rows below are content.
        let plane = DET_HEIGHT * DET_WIDTH;
        assert_eq!(prepared.tensor[539 * DET_WIDTH + 100], 1.0);
        assert_eq!(prepared.tensor[540 * DET_WIDTH + 100], 0.0);
        assert_eq!(prepared.tensor[plane + 539 * DET_WIDTH + 959], 1.0);
        assert_eq!(prepared.tensor[2 * plane + 543 * DET_WIDTH], 0.0);
    }

    #[test]
    fn detection_of_a_narrow_band_is_limited_by_the_width() {
        let prepared = prepare_detection(&white_image(1920, 346));

        // Scale 0.5: 960x173 content, target size 1920x1088.
        assert_eq!(prepared.target_size, [1920, 1088]);
        assert_eq!(prepared.tensor[172 * DET_WIDTH], 1.0);
        assert_eq!(prepared.tensor[173 * DET_WIDTH], 0.0);
    }

    #[test]
    fn detection_of_a_small_image_scales_it_up() {
        let prepared = prepare_detection(&white_image(480, 272));

        assert_eq!(prepared.target_size, [480, 272]);
        assert_eq!(prepared.tensor[543 * DET_WIDTH + 959], 1.0);
    }

    #[test]
    fn detection_tensor_is_bgr() {
        let image = RgbImage::from_pixel(960, 544, image::Rgb([255, 0, 0]));

        let prepared = prepare_detection(&image);

        let plane = DET_HEIGHT * DET_WIDTH;
        assert_eq!(prepared.tensor[0], 0.0);
        assert_eq!(prepared.tensor[plane], 0.0);
        assert_eq!(prepared.tensor[2 * plane], 1.0);
    }

    fn detection_output(entries: &[([f32; 4], f32)]) -> (Vec<f32>, Vec<f32>) {
        let mut boxes = Vec::new();
        let mut scores = Vec::new();
        for (coordinates, score) in entries {
            boxes.extend_from_slice(coordinates);
            scores.push(*score);
        }
        while scores.len() < DET_MAX_BOXES {
            boxes.extend_from_slice(&[0.0; 4]);
            scores.push(0.0);
        }
        (boxes, scores)
    }

    #[test]
    fn decoding_detection_keeps_confident_boxes_sorted_top_to_bottom() {
        let (boxes, scores) = detection_output(&[
            ([100.0, 400.0, 900.0, 460.0], 0.9),
            ([100.0, 100.0, 500.0, 160.0], 0.8),
            ([0.0, 200.0, 50.0, 250.0], 0.3),
        ]);

        let found = decode_detection(&boxes, &scores, 1920, 1080, 0.5);

        assert_eq!(found, vec![[100, 100, 500, 160], [100, 400, 900, 460]]);
    }

    #[test]
    fn decoding_detection_uses_a_strict_threshold() {
        let (boxes, scores) = detection_output(&[([10.0, 10.0, 90.0, 40.0], 0.5)]);

        assert_eq!(
            decode_detection(&boxes, &scores, 200, 100, 0.5),
            Vec::<[i32; 4]>::new()
        );
        assert_eq!(decode_detection(&boxes, &scores, 200, 100, 0.49).len(), 1);
    }

    #[test]
    fn decoding_detection_clamps_boxes_to_the_image_and_drops_degenerate_ones() {
        let (boxes, scores) = detection_output(&[
            ([-20.0, -5.0, 500.0, 90.0], 0.9),
            ([150.0, 10.0, 150.0, 40.0], 0.9),
            ([300.0, 95.0, 400.0, 100.0], 0.9),
            ([300.0, 200.0, 400.0, 260.0], 0.9),
        ]);

        let found = decode_detection(&boxes, &scores, 200, 100, 0.5);

        assert_eq!(found, vec![[0, 0, 200, 90]]);
    }

    #[test]
    fn recognition_scales_the_line_to_the_model_height() {
        let prepared =
            prepare_recognition(&white_image(800, 100), [100, 10, 500, 60]).expect("line");

        // 400x50 scaled to height 32 gives width 256.
        assert_eq!(prepared.effective_length, 256);
        assert_eq!(prepared.bbox, [100, 10, 500, 60]);
        assert_eq!(prepared.tensor.len(), 3 * REC_HEIGHT * REC_WIDTH);
        assert_eq!(prepared.tensor[31 * REC_WIDTH + 255], 1.0);
        assert_eq!(prepared.tensor[31 * REC_WIDTH + 256], 0.0);
    }

    #[test]
    fn recognition_shrinks_lines_that_are_too_wide_keeping_the_aspect_ratio() {
        let prepared =
            prepare_recognition(&white_image(1900, 100), [0, 0, 1900, 50]).expect("line");

        // 1900x50 would be 1216 wide at height 32; it is shrunk to 960 and height 25 (rounded).
        assert_eq!(prepared.effective_length, 960);
        assert_eq!(prepared.tensor[24 * REC_WIDTH + 959], 1.0);
        assert_eq!(prepared.tensor[25 * REC_WIDTH + 959], 0.0);
    }

    #[test]
    fn recognition_rounds_halves_to_even_like_python() {
        // 40x32 -> width 40 exactly; 5x2 -> 5 * 32 / 2 = 80; use a case that lands on .5:
        // 5x64 -> 2.5, which Python rounds to 2.
        let prepared = prepare_recognition(&white_image(10, 64), [0, 0, 5, 64]).expect("line");

        assert_eq!(prepared.effective_length, 2);
        assert_eq!(round_half_even(0.5), 0);
        assert_eq!(round_half_even(1.5), 2);
        assert_eq!(round_half_even(2.5), 2);
        assert_eq!(round_half_even(2.6), 3);
    }

    #[test]
    fn recognition_skips_empty_and_vanishing_lines() {
        let image = white_image(100, 100);

        assert_eq!(prepare_recognition(&image, [10, 10, 10, 50]), None);
        assert_eq!(prepare_recognition(&image, [10, 10, 50, 10]), None);
        // 1x100 -> width round(0.32) = 0.
        assert_eq!(prepare_recognition(&image, [0, 0, 1, 100]), None);
    }

    #[test]
    fn decoding_recognition_maps_characters_back_to_image_coordinates() {
        // The line is 400x50 at (100, 10); the canvas holds it in 256 columns.
        let line = decode(
            &[
                ('猫', [0.0, 0.0, 32.0, 32.0], 0.9),
                ('が', [32.0, 0.0, 64.0, 32.0], 0.8),
            ],
            256,
            [100, 10, 500, 60],
            0.1,
        );

        assert_eq!(line.text, "猫が");
        assert_eq!(line.bbox, [100, 10, 500, 60]);
        assert_eq!(
            line.chars,
            vec![
                OcrChar {
                    character: '猫',
                    bbox: [100, 10, 150, 60],
                    confidence: 0.9
                },
                OcrChar {
                    character: 'が',
                    bbox: [150, 10, 200, 60],
                    confidence: 0.8
                },
            ]
        );
    }

    #[test]
    fn decoding_recognition_orders_characters_left_to_right_regardless_of_output_order() {
        let line = decode(
            &[
                ('き', [64.0, 0.0, 96.0, 32.0], 0.7),
                ('好', [32.0, 0.0, 64.0, 32.0], 0.9),
                ('猫', [0.0, 0.0, 32.0, 32.0], 0.8),
            ],
            960,
            [0, 0, 960, 32],
            0.1,
        );

        assert_eq!(line.text, "猫好き");
    }

    #[test]
    fn decoding_recognition_drops_low_scores_with_a_non_strict_threshold() {
        let line = decode(
            &[
                ('猫', [0.0, 0.0, 32.0, 32.0], 0.1),
                ('が', [32.0, 0.0, 64.0, 32.0], 0.09),
            ],
            960,
            [0, 0, 960, 32],
            0.1,
        );

        assert_eq!(line.text, "猫");
    }

    #[test]
    fn decoding_recognition_ignores_characters_in_the_padding_and_clips_the_last_one() {
        let line = decode(
            &[
                ('猫', [0.0, 0.0, 32.0, 32.0], 0.9),
                ('が', [100.0, 0.0, 140.0, 32.0], 0.9),
                ('好', [120.0, 0.0, 160.0, 32.0], 0.9),
            ],
            128,
            [0, 0, 128, 32],
            0.1,
        );

        assert_eq!(line.text, "猫が");
        assert_eq!(line.chars[1].bbox, [100, 0, 128, 32]);
    }

    #[test]
    fn overlapping_characters_keep_the_most_confident_one() {
        let line = decode(
            &[
                ('ロ', [0.0, 0.0, 30.0, 32.0], 0.6),
                ('口', [2.0, 0.0, 32.0, 32.0], 0.9),
                ('が', [32.0, 0.0, 64.0, 32.0], 0.5),
            ],
            960,
            [0, 0, 960, 32],
            0.1,
        );

        assert_eq!(line.text, "口が");
    }

    #[test]
    fn slightly_overlapping_characters_are_both_kept() {
        // Overlap of 4 columns out of 32 is 12.5%, below the 30% threshold.
        let line = decode(
            &[
                ('猫', [0.0, 0.0, 32.0, 32.0], 0.9),
                ('が', [28.0, 0.0, 60.0, 32.0], 0.8),
            ],
            960,
            [0, 0, 960, 32],
            0.1,
        );

        assert_eq!(line.text, "猫が");
    }

    #[test]
    fn touching_characters_are_not_overlapping() {
        let line = decode(
            &[
                ('猫', [0.0, 0.0, 32.0, 32.0], 0.9),
                ('が', [32.0, 0.0, 64.0, 32.0], 0.9),
            ],
            960,
            [0, 0, 960, 32],
            0.1,
        );

        assert_eq!(line.text, "猫が");
    }

    #[test]
    fn invalid_label_codes_are_ignored() {
        let (mut labels, boxes, scores) = raw_output(&[
            ('猫', [0.0, 0.0, 32.0, 32.0], 0.9),
            ('が', [32.0, 0.0, 64.0, 32.0], 0.9),
            ('き', [64.0, 0.0, 96.0, 32.0], 0.9),
        ]);
        labels[1] = -5;
        labels[2] = 0xD800;

        let line = decode_recognition(&input(960, [0, 0, 960, 32]), &labels, &boxes, &scores, 0.1);

        assert_eq!(line.text, "猫");
    }

    #[test]
    fn an_empty_output_gives_an_empty_line() {
        let line = decode(&[], 960, [5, 6, 100, 40], 0.1);

        assert_eq!(line.text, "");
        assert_eq!(line.chars, vec![]);
        assert_eq!(line.bbox, [5, 6, 100, 40]);
    }

    #[test]
    fn swapped_character_pairs_are_corrected_in_text_and_chars() {
        let line = decode(
            &[
                ('冗', [0.0, 0.0, 32.0, 32.0], 0.9),
                ('談', [32.0, 0.0, 64.0, 32.0], 0.9),
                ('だ', [64.0, 0.0, 96.0, 32.0], 0.9),
            ],
            960,
            [0, 0, 960, 32],
            0.1,
        );

        // The model emitted 談冗 swapped as 冗談 reversed: here 冗談 is already right.
        assert_eq!(line.text, "冗談だ");

        let swapped = decode(
            &[
                ('談', [0.0, 0.0, 32.0, 32.0], 0.9),
                ('冗', [32.0, 0.0, 64.0, 32.0], 0.9),
                ('だ', [64.0, 0.0, 96.0, 32.0], 0.9),
            ],
            960,
            [0, 0, 960, 32],
            0.1,
        );
        assert_eq!(swapped.text, "冗談だ");
        assert_eq!(
            swapped
                .chars
                .iter()
                .map(|c| c.character)
                .collect::<String>(),
            "冗談だ"
        );
    }

    #[test]
    fn swapped_pairs_are_found_after_other_characters() {
        let line = decode(
            &[
                ('そ', [0.0, 0.0, 32.0, 32.0], 0.9),
                ('の', [32.0, 0.0, 64.0, 32.0], 0.9),
                ('談', [64.0, 0.0, 96.0, 32.0], 0.9),
                ('冗', [96.0, 0.0, 128.0, 32.0], 0.9),
            ],
            960,
            [0, 0, 960, 32],
            0.1,
        );

        assert_eq!(line.text, "その冗談");
    }

    #[test]
    fn a_short_line_is_not_split() {
        // Height 20: lines up to 26 * 20 = 520 pixels stay whole.
        assert_eq!(segment_boxes([100, 50, 620, 70]), vec![[100, 50, 620, 70]]);
        assert_eq!(segment_boxes([0, 0, 10, 20]), vec![[0, 0, 10, 20]]);
    }

    #[test]
    fn a_long_line_is_cut_into_overlapping_segments_covering_it_completely() {
        // Height 20, width 1300: segments of 520 pixels advancing by 520 - 80 = 440.
        let segments = segment_boxes([100, 50, 1400, 70]);

        assert_eq!(
            segments,
            vec![[100, 50, 620, 70], [540, 50, 1060, 70], [880, 50, 1400, 70],]
        );
        assert_eq!(segments.first().expect("first")[0], 100);
        assert_eq!(segments.last().expect("last")[2], 1400);
        for pair in segments.windows(2) {
            assert!(
                pair[1][0] < pair[0][2],
                "neighbouring segments must overlap"
            );
        }
    }

    #[test]
    fn every_segment_is_readable_by_the_model() {
        for width in [521, 800, 1300, 1900, 5000] {
            for segment in segment_boxes([0, 0, width, 20]) {
                assert!(segment[2] - segment[0] <= 20 * SEGMENT_ASPECT);
                assert_eq!(segment[1], 0);
                assert_eq!(segment[3], 20);
            }
        }
    }

    #[test]
    fn a_line_just_over_the_limit_gets_two_segments_ending_at_its_edges() {
        assert_eq!(
            segment_boxes([0, 0, 521, 20]),
            vec![[0, 0, 520, 20], [1, 0, 521, 20]]
        );
    }

    #[test]
    fn prepare_line_gives_one_input_per_segment_and_keeps_the_line_box() {
        let image = white_image(1920, 200);

        let line = prepare_line(&image, [100, 50, 1400, 70]).expect("line");

        assert_eq!(line.bbox, [100, 50, 1400, 70]);
        assert_eq!(line.segments.len(), 3);
        assert_eq!(line.segments[1].bbox, [540, 50, 1060, 70]);
    }

    #[test]
    fn prepare_line_gives_nothing_for_a_line_that_cannot_be_read() {
        assert_eq!(prepare_line(&white_image(100, 100), [10, 10, 10, 50]), None);
    }

    fn candidates(entries: &[(char, [f32; 4], f32)], input: &RecognitionInput) -> Vec<Candidate> {
        let (labels, boxes, scores) = raw_output(entries);
        segment_candidates(input, &labels, &boxes, &scores, 0.1)
    }

    #[test]
    fn characters_seen_by_two_overlapping_segments_are_kept_once() {
        // A 960-pixel line read in two segments that share 80 pixels (x = 440..520). Characters
        // are 40 pixels wide: 猫 at 440, が at 480, 好 at 520. Segment canvases are 960 wide for
        // a 520-pixel segment, so one image pixel is 960 / 520 canvas pixels.
        let scale = 960.0 / 520.0;
        let first = input(960, [0, 0, 520, 20]);
        let second = input(960, [440, 0, 960, 20]);
        let mut all = Vec::new();
        // The first segment sees 猫 and が completely.
        all.extend(candidates(
            &[
                ('猫', [440.0 * scale, 0.0, 480.0 * scale, 32.0], 0.9),
                ('が', [480.0 * scale, 0.0, 520.0 * scale, 32.0], 0.8),
            ],
            &first,
        ));
        // The second sees the same two characters at its left edge, plus 好.
        all.extend(candidates(
            &[
                ('猫', [0.0, 0.0, 40.0 * scale, 32.0], 0.7),
                ('が', [40.0 * scale, 0.0, 80.0 * scale, 32.0], 0.95),
                ('好', [80.0 * scale, 0.0, 120.0 * scale, 32.0], 0.9),
            ],
            &second,
        ));

        let line = assemble_line([0, 0, 960, 20], all, false);

        assert_eq!(line.text, "猫が好");
        assert_eq!(line.chars.len(), 3);
        // The more confident reading of each duplicated character is the one kept.
        assert_eq!(line.chars[0].confidence, 0.9);
        assert_eq!(line.chars[1].confidence, 0.95);
        assert_eq!(line.chars[2].confidence, 0.9);
    }

    #[test]
    fn assembling_without_candidates_gives_an_empty_line_with_the_box() {
        let line = assemble_line([5, 6, 700, 40], vec![], false);

        assert_eq!(line.text, "");
        assert_eq!(line.bbox, [5, 6, 700, 40]);
    }
}
