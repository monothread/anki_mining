//! On-screen text recognition (Japanese), run locally with the bundled MeikiOCR models.

pub mod layout;
pub mod meiki;
pub mod pipeline;
mod raster;

use image::RgbImage;
use serde::Serialize;
use thiserror::Error;

pub use meiki::MeikiEngine;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OcrError {
    #[error("cannot load the OCR model `{path}`: {reason}")]
    Model { path: String, reason: String },
    #[error("OCR inference failed: {0}")]
    Inference(String),
    #[error("unexpected OCR model output: {0}")]
    UnexpectedOutput(String),
}

/// One recognized character with its position in the image.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrChar {
    pub character: char,
    /// `[x1, y1, x2, y2]` in image pixels.
    pub bbox: [i32; 4],
    pub confidence: f32,
}

/// A line of text found in the image.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    /// `[x1, y1, x2, y2]` in image pixels.
    pub bbox: [i32; 4],
    pub chars: Vec<OcrChar>,
    /// Whether the text runs top to bottom (a column); `chars` are then ordered from top to
    /// bottom.
    pub vertical: bool,
}

/// Reads the text of an image, top to bottom. Lines without text are not returned.
pub trait OcrEngine {
    fn recognize(&mut self, image: &RgbImage) -> Result<Vec<OcrLine>, OcrError>;

    /// Like [`recognize`](OcrEngine::recognize) but only has to return the lines near `cursor`
    /// (`x`, `y` in image pixels). Engines can use this to skip the rest of the image, which
    /// matters on a full screen of text. The default reads everything.
    fn recognize_near(
        &mut self,
        image: &RgbImage,
        _cursor: (i32, i32),
    ) -> Result<Vec<OcrLine>, OcrError> {
        self.recognize(image)
    }
}

/// Joins the recognized lines into one sentence (Japanese is written without spaces), in reading
/// order: horizontal lines as given (top to bottom), vertical columns from right to left.
pub fn lines_to_text(lines: &[OcrLine]) -> String {
    let mut ordered: Vec<&OcrLine> = lines.iter().collect();
    if !ordered.is_empty() && ordered.iter().all(|line| line.vertical) {
        ordered.sort_by_key(|line| std::cmp::Reverse(line.bbox[2]));
    }
    ordered.iter().map(|line| line.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> OcrLine {
        OcrLine {
            text: text.to_string(),
            bbox: [0, 0, 10, 10],
            chars: vec![],
            vertical: false,
        }
    }

    #[test]
    fn lines_are_joined_without_separators() {
        assert_eq!(
            lines_to_text(&[line("今日は"), line("いい天気")]),
            "今日はいい天気"
        );
    }

    #[test]
    fn vertical_columns_are_read_from_right_to_left() {
        let column = |text: &str, x: i32| OcrLine {
            text: text.to_string(),
            bbox: [x, 0, x + 40, 400],
            chars: vec![],
            vertical: true,
        };

        assert_eq!(
            lines_to_text(&[
                column("二列目", 100),
                column("一列目", 300),
                column("三列目", 20)
            ]),
            "一列目二列目三列目"
        );
    }

    #[test]
    fn mixed_lines_keep_the_given_order() {
        let mut vertical = line("縦");
        vertical.vertical = true;
        vertical.bbox = [500, 0, 540, 300];
        let mut first = line("横");
        first.bbox = [0, 0, 100, 40];

        assert_eq!(lines_to_text(&[first, vertical]), "横縦");
    }

    #[test]
    fn no_lines_give_empty_text() {
        assert_eq!(lines_to_text(&[]), "");
    }

    #[test]
    fn lines_serialize_with_camel_case_keys() {
        let value = serde_json::to_value(OcrLine {
            text: "猫".to_string(),
            bbox: [1, 2, 3, 4],
            chars: vec![OcrChar {
                character: '猫',
                bbox: [1, 2, 3, 4],
                confidence: 0.5,
            }],
            vertical: false,
        })
        .expect("serialize");

        assert_eq!(
            value,
            serde_json::json!({
                "text": "猫",
                "bbox": [1, 2, 3, 4],
                "chars": [{ "character": "猫", "bbox": [1, 2, 3, 4], "confidence": 0.5 }],
                "vertical": false
            })
        );
    }

    #[test]
    fn errors_have_descriptive_messages() {
        assert_eq!(
            OcrError::Model {
                path: "a.onnx".to_string(),
                reason: "missing".to_string()
            }
            .to_string(),
            "cannot load the OCR model `a.onnx`: missing"
        );
        assert_eq!(
            OcrError::Inference("boom".to_string()).to_string(),
            "OCR inference failed: boom"
        );
        assert_eq!(
            OcrError::UnexpectedOutput("shape".to_string()).to_string(),
            "unexpected OCR model output: shape"
        );
    }
}
