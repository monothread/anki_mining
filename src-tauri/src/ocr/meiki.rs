use std::path::Path;
use std::sync::Mutex;

use image::RgbImage;
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;

use super::pipeline::{
    assemble_line, decode_detection, prepare_detection, prepare_line, segment_candidates,
    Candidate, LineInput, RecognitionInput, DET_HEIGHT, DET_MAX_BOXES, DET_WIDTH, REC_HEIGHT,
    REC_MAX_CHARS, REC_WIDTH,
};
use super::{OcrEngine, OcrError, OcrLine};

/// Name of the ONNX Runtime shared library inside the OCR resources directory.
pub const RUNTIME_LIBRARY: &str = "libonnxruntime.so";
pub const DETECTION_MODEL: &str = "meiki.text.detect.v0.1.960x544.onnx";
pub const RECOGNITION_MODEL: &str = "meiki.text.rec.v0.960x32.onnx";

/// Confidence thresholds measured in the P3-01 spike (see ADR-006).
pub const DETECTION_THRESHOLD: f32 = 0.5;
pub const RECOGNITION_THRESHOLD: f32 = 0.1;
/// Lines recognized per model call; keeps memory use bounded.
const RECOGNITION_BATCH: usize = 8;

/// MeikiOCR (horizontal text) running on ONNX Runtime.
///
/// Vertical text is not supported yet: tall, narrow boxes are read as horizontal lines.
pub struct MeikiEngine {
    detector: Session,
    recognizer: Session,
}

impl MeikiEngine {
    /// Loads the ONNX Runtime library and both models from `directory` (see
    /// `scripts/fetch-ocr-models.sh`).
    pub fn from_directory(directory: &Path) -> Result<MeikiEngine, OcrError> {
        let environment = shared_environment(&directory.join(RUNTIME_LIBRARY))?;
        Ok(MeikiEngine {
            detector: load_session(&environment, &directory.join(DETECTION_MODEL))?,
            recognizer: load_session(&environment, &directory.join(RECOGNITION_MODEL))?,
        })
    }

    fn detect(&mut self, image: &RgbImage) -> Result<Vec<[i32; 4]>, OcrError> {
        let prepared = prepare_detection(image);
        let images = tensor_f32(&[1, 3, DET_HEIGHT, DET_WIDTH], prepared.tensor)?;
        let sizes = tensor_i64(&[1, 2], prepared.target_size.to_vec())?;

        let outputs = self
            .detector
            .run(ort::inputs!["images" => images, "orig_target_sizes" => sizes])
            .map_err(inference_error)?;
        let (_, boxes) = outputs["boxes"]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;
        let (_, scores) = outputs["scores"]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;
        expect_len("detection boxes", boxes.len(), DET_MAX_BOXES * 4)?;
        expect_len("detection scores", scores.len(), DET_MAX_BOXES)?;

        Ok(decode_detection(
            boxes,
            scores,
            image.width(),
            image.height(),
            DETECTION_THRESHOLD,
        ))
    }

    /// Reads a batch of segments and returns the character candidates of each.
    fn recognize_batch(
        &mut self,
        batch: &[&RecognitionInput],
    ) -> Result<Vec<Vec<Candidate>>, OcrError> {
        let mut data = Vec::with_capacity(batch.len() * 3 * REC_HEIGHT * REC_WIDTH);
        for input in batch {
            data.extend_from_slice(&input.tensor);
        }
        let images = tensor_f32(&[batch.len(), 3, REC_HEIGHT, REC_WIDTH], data)?;
        let sizes = tensor_i64(&[1, 2], vec![REC_WIDTH as i64, REC_HEIGHT as i64])?;

        let outputs = self
            .recognizer
            .run(ort::inputs!["images" => images, "orig_target_sizes" => sizes])
            .map_err(inference_error)?;
        let (_, labels) = outputs["char_codes"]
            .try_extract_tensor::<i32>()
            .map_err(inference_error)?;
        let (_, boxes) = outputs["boxes"]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;
        let (_, scores) = outputs["scores"]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;
        expect_len(
            "recognition labels",
            labels.len(),
            batch.len() * REC_MAX_CHARS,
        )?;
        expect_len(
            "recognition boxes",
            boxes.len(),
            batch.len() * REC_MAX_CHARS * 4,
        )?;
        expect_len(
            "recognition scores",
            scores.len(),
            batch.len() * REC_MAX_CHARS,
        )?;

        Ok(batch
            .iter()
            .enumerate()
            .map(|(index, input)| {
                segment_candidates(
                    input,
                    &labels[index * REC_MAX_CHARS..(index + 1) * REC_MAX_CHARS],
                    &boxes[index * REC_MAX_CHARS * 4..(index + 1) * REC_MAX_CHARS * 4],
                    &scores[index * REC_MAX_CHARS..(index + 1) * REC_MAX_CHARS],
                    RECOGNITION_THRESHOLD,
                )
            })
            .collect())
    }

    /// Reads the detected `boxes`; long lines are read in overlapping segments and merged.
    fn read_boxes(
        &mut self,
        image: &RgbImage,
        boxes: Vec<[i32; 4]>,
    ) -> Result<Vec<OcrLine>, OcrError> {
        let lines: Vec<LineInput> = boxes
            .into_iter()
            .filter_map(|bbox| prepare_line(image, bbox))
            .collect();
        let segments: Vec<&RecognitionInput> =
            lines.iter().flat_map(|line| line.segments.iter()).collect();

        let mut per_segment = Vec::with_capacity(segments.len());
        for batch in segments.chunks(RECOGNITION_BATCH) {
            per_segment.extend(self.recognize_batch(batch)?);
        }

        let mut results = per_segment.into_iter();
        let mut text_lines = Vec::with_capacity(lines.len());
        for line in &lines {
            let candidates: Vec<Candidate> = results
                .by_ref()
                .take(line.segments.len())
                .flatten()
                .collect();
            text_lines.push(assemble_line(line.bbox, candidates));
        }
        text_lines.retain(|line| !line.text.is_empty());
        Ok(text_lines)
    }
}

impl OcrEngine for MeikiEngine {
    fn recognize(&mut self, image: &RgbImage) -> Result<Vec<OcrLine>, OcrError> {
        if image.width() == 0 || image.height() == 0 {
            return Ok(Vec::new());
        }
        let boxes = self.detect(image)?;
        self.read_boxes(image, boxes)
    }

    fn recognize_near(
        &mut self,
        image: &RgbImage,
        cursor: (i32, i32),
    ) -> Result<Vec<OcrLine>, OcrError> {
        if image.width() == 0 || image.height() == 0 {
            return Ok(Vec::new());
        }
        let near: Vec<[i32; 4]> = self
            .detect(image)?
            .into_iter()
            .filter(|bbox| is_near(*bbox, cursor))
            .collect();
        self.read_boxes(image, near)
    }
}

/// Whether the cursor is on the line or close enough to pick one of its characters.
fn is_near(bbox: [i32; 4], cursor: (i32, i32)) -> bool {
    let height = bbox[3] - bbox[1];
    cursor.0 >= bbox[0] - height
        && cursor.0 <= bbox[2] + height
        && cursor.1 >= bbox[1] - height
        && cursor.1 <= bbox[3] + height
}

/// ONNX Runtime allows a single environment per process, so every engine shares the first one.
/// The runtime library of the first engine created is the one used for the whole process.
fn shared_environment(library: &Path) -> Result<Environment, OcrError> {
    static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);

    let mut slot = ENVIRONMENT
        .lock()
        .map_err(|_| OcrError::Inference("the ONNX Runtime state is unavailable".to_string()))?;
    if let Some(environment) = slot.as_ref() {
        return Ok(environment.clone());
    }
    let environment = ort::init_from(library)
        .map_err(|error| OcrError::Model {
            path: library.display().to_string(),
            reason: error.to_string(),
        })?
        .build()
        .map_err(inference_error)?;
    *slot = Some(environment.clone());
    Ok(environment)
}

fn load_session(environment: &Environment, path: &Path) -> Result<Session, OcrError> {
    let model_error = |reason: String| OcrError::Model {
        path: path.display().to_string(),
        reason,
    };
    Session::builder(environment)
        .and_then(|mut builder| builder.commit_from_file(path))
        .map_err(|error| model_error(error.to_string()))
}

fn tensor_f32(shape: &[usize], data: Vec<f32>) -> Result<Tensor<f32>, OcrError> {
    Tensor::from_array((shape.to_vec(), data.into_boxed_slice())).map_err(inference_error)
}

fn tensor_i64(shape: &[usize], data: Vec<i64>) -> Result<Tensor<i64>, OcrError> {
    Tensor::from_array((shape.to_vec(), data.into_boxed_slice())).map_err(inference_error)
}

fn inference_error(error: impl std::fmt::Display) -> OcrError {
    OcrError::Inference(error.to_string())
}

fn expect_len(what: &str, actual: usize, expected: usize) -> Result<(), OcrError> {
    if actual == expected {
        Ok(())
    } else {
        Err(OcrError::UnexpectedOutput(format!(
            "{what} has {actual} values, expected {expected}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use serde::Deserialize;

    use super::*;
    use crate::normalizer::clean_text;
    use crate::ocr::lines_to_text;

    #[derive(Deserialize)]
    struct Fixture {
        file: String,
        text: String,
    }

    fn models_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ocr")
    }

    fn fixtures_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr")
    }

    /// One engine shared by all tests: loading the models is the slow part.
    fn engine() -> &'static Mutex<MeikiEngine> {
        static ENGINE: OnceLock<Mutex<MeikiEngine>> = OnceLock::new();
        ENGINE.get_or_init(|| {
            let engine = MeikiEngine::from_directory(&models_directory()).unwrap_or_else(|error| {
                panic!("{error}; run scripts/fetch-ocr-models.sh to download the models")
            });
            Mutex::new(engine)
        })
    }

    fn fixtures() -> Vec<Fixture> {
        let json =
            std::fs::read_to_string(fixtures_directory().join("cases.json")).expect("cases.json");
        serde_json::from_str(&json).expect("valid cases.json")
    }

    fn load(file: &str) -> RgbImage {
        image::open(fixtures_directory().join(file))
            .expect("fixture image")
            .to_rgb8()
    }

    fn recognize(image: &RgbImage) -> Vec<OcrLine> {
        engine()
            .lock()
            .expect("engine lock")
            .recognize(image)
            .expect("recognize")
    }

    #[test]
    fn reads_every_subtitle_fixture_exactly() {
        for fixture in fixtures() {
            let lines = recognize(&load(&fixture.file));

            // The app normalizes (NFKC) before tokenizing, so full-width and ASCII punctuation
            // such as `？` and `?` are the same text for it.
            assert_eq!(
                clean_text(&lines_to_text(&lines)),
                clean_text(&fixture.text),
                "fixture {}",
                fixture.file
            );
        }
    }

    #[test]
    fn reports_one_line_with_a_box_inside_the_image_and_character_boxes_inside_the_line() {
        let fixture = &fixtures()[0];
        let image = load(&fixture.file);

        let lines = recognize(&image);

        assert_eq!(lines.len(), 1);
        let [x1, y1, x2, y2] = lines[0].bbox;
        assert!(0 <= x1 && x1 < x2 && x2 <= image.width() as i32);
        assert!(0 <= y1 && y1 < y2 && y2 <= image.height() as i32);
        assert_eq!(lines[0].chars.len(), fixture.text.chars().count());
        let mut previous_end = 0;
        for ocr_char in &lines[0].chars {
            assert!(ocr_char.bbox[0] >= x1 - 2 && ocr_char.bbox[2] <= x2 + 2);
            assert!(
                ocr_char.bbox[0] >= previous_end - 4 || previous_end == 0,
                "characters must be left to right"
            );
            assert!(ocr_char.confidence >= RECOGNITION_THRESHOLD);
            previous_end = ocr_char.bbox[2];
        }
        assert_eq!(
            lines[0]
                .chars
                .iter()
                .map(|c| c.character)
                .collect::<String>(),
            lines[0].text
        );
    }

    /// Two long-line fixtures stacked in one image, 200 pixels apart.
    fn two_paragraphs() -> (RgbImage, [Fixture; 2]) {
        let all = fixtures();
        let first = all
            .iter()
            .find(|f| f.file == "long_16px_100c.png")
            .expect("first");
        let second = all
            .iter()
            .find(|f| f.file == "long_24px_60c.png")
            .expect("second");
        let (top, bottom) = (load(&first.file), load(&second.file));
        let mut page = RgbImage::from_pixel(
            1920,
            top.height() + bottom.height(),
            image::Rgb([255, 255, 255]),
        );
        image::imageops::replace(&mut page, &top, 0, 0);
        image::imageops::replace(&mut page, &bottom, 0, i64::from(top.height()));
        let owned = |fixture: &Fixture| Fixture {
            file: fixture.file.clone(),
            text: fixture.text.clone(),
        };
        (page, [owned(first), owned(second)])
    }

    #[test]
    fn long_lines_are_read_completely_in_overlapping_segments() {
        let (page, [first, second]) = two_paragraphs();

        let lines = recognize(&page);

        assert_eq!(lines.len(), 2);
        assert_eq!(clean_text(&lines[0].text), clean_text(&first.text));
        assert_eq!(clean_text(&lines[1].text), clean_text(&second.text));
        // Each character appears once, in reading order.
        for line in &lines {
            let starts: Vec<i32> = line.chars.iter().map(|c| c.bbox[0]).collect();
            assert!(starts.windows(2).all(|pair| pair[0] <= pair[1]));
            assert_eq!(line.chars.len(), line.text.chars().count());
        }
    }

    #[test]
    fn recognize_near_reads_only_the_line_under_the_cursor() {
        let (page, [first, second]) = two_paragraphs();
        let lines = recognize(&page);
        let on = |line: &OcrLine| {
            (
                (line.bbox[0] + line.bbox[2]) / 2,
                (line.bbox[1] + line.bbox[3]) / 2,
            )
        };

        let near_first = engine()
            .lock()
            .expect("lock")
            .recognize_near(&page, on(&lines[0]))
            .expect("near");
        let near_second = engine()
            .lock()
            .expect("lock")
            .recognize_near(&page, on(&lines[1]))
            .expect("near");

        assert_eq!(near_first.len(), 1);
        assert_eq!(clean_text(&near_first[0].text), clean_text(&first.text));
        assert_eq!(near_first[0].bbox, lines[0].bbox);
        assert_eq!(near_second.len(), 1);
        assert_eq!(clean_text(&near_second[0].text), clean_text(&second.text));
    }

    #[test]
    fn recognize_near_finds_nothing_when_the_cursor_is_far_from_any_text() {
        let (page, _) = two_paragraphs();

        let lines = engine()
            .lock()
            .expect("lock")
            .recognize_near(&page, (10, 395))
            .expect("near");

        assert_eq!(lines, vec![]);
    }

    #[test]
    fn recognize_near_on_an_empty_image_gives_no_lines() {
        let lines = engine()
            .lock()
            .expect("lock")
            .recognize_near(&RgbImage::new(0, 0), (0, 0))
            .expect("near");

        assert_eq!(lines, vec![]);
    }

    #[test]
    fn is_near_accepts_the_line_and_a_margin_of_one_line_height_around_it() {
        let line = [100, 200, 900, 230];

        assert!(is_near(line, (500, 215)));
        assert!(is_near(line, (70, 215)));
        assert!(is_near(line, (930, 215)));
        assert!(is_near(line, (500, 170)));
        assert!(is_near(line, (500, 260)));
        assert!(!is_near(line, (69, 215)));
        assert!(!is_near(line, (931, 215)));
        assert!(!is_near(line, (500, 169)));
        assert!(!is_near(line, (500, 261)));
    }

    #[test]
    fn finds_no_text_in_an_empty_frame() {
        let blank = RgbImage::from_pixel(1280, 200, image::Rgb([20, 20, 20]));

        assert_eq!(recognize(&blank), vec![]);
    }

    #[test]
    fn an_image_without_pixels_gives_no_lines() {
        assert_eq!(recognize(&RgbImage::new(0, 0)), vec![]);
    }

    #[test]
    fn a_tiny_image_does_not_fail() {
        let tiny = RgbImage::from_pixel(8, 8, image::Rgb([255, 255, 255]));

        assert_eq!(recognize(&tiny), vec![]);
    }

    #[test]
    fn loading_from_a_missing_directory_reports_the_model_path() {
        let error = match MeikiEngine::from_directory(Path::new("/definitely/missing")) {
            Ok(_) => panic!("must fail"),
            Err(error) => error,
        };

        assert!(matches!(error, OcrError::Model { .. }));
        assert!(error
            .to_string()
            .contains("/definitely/missing/meiki.text.detect.v0.1.960x544.onnx"));
    }

    #[test]
    fn several_engines_can_live_in_the_same_process() {
        let first = MeikiEngine::from_directory(&models_directory());
        let second = MeikiEngine::from_directory(&models_directory());

        assert!(first.is_ok(), "{:?}", first.err());
        let mut second = second.expect("second engine");
        let fixture = &fixtures()[0];
        let lines = second.recognize(&load(&fixture.file)).expect("recognize");
        assert_eq!(
            clean_text(&lines_to_text(&lines)),
            clean_text(&fixture.text)
        );
    }

    #[test]
    fn expect_len_accepts_only_the_exact_size() {
        assert_eq!(expect_len("x", 4, 4), Ok(()));
        assert_eq!(
            expect_len("boxes", 3, 4),
            Err(OcrError::UnexpectedOutput(
                "boxes has 3 values, expected 4".to_string()
            ))
        );
    }
}
