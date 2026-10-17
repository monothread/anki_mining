//! Looking up the word under the mouse cursor on the whole screen.

pub mod locate;
pub mod placement;
pub mod trigger;

use std::sync::Mutex;

use image::RgbImage;

use crate::pointer::WindowIdentity;
use serde::Serialize;
use thiserror::Error;

use crate::capture::{CaptureError, Region, ScreenCapture};
use crate::cards::CardInput;
use crate::dictionary::ScanResult;
use crate::ocr::{OcrEngine, OcrError};
use crate::tokenizer::{Tokenizer, TokenizerError};

use self::locate::{character_at, word_at};

#[derive(Debug, Error)]
pub enum HoverError {
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error(transparent)]
    Ocr(#[from] OcrError),
    #[error(transparent)]
    Tokenizer(#[from] TokenizerError),
    #[error("dictionary lookup failed: {0}")]
    Dictionary(String),
}

/// Source recorded on the cards made from a hover lookup.
pub const CARD_SOURCE: &str = "screen";

/// What the popup shows right now, kept so that it can be added to Anki.
#[derive(Debug)]
struct Remembered {
    result: HoverResult,
    /// The window the word was found in, as it looked at that moment.
    screenshot: Option<RgbImage>,
    /// The application the word was found in.
    identity: WindowIdentity,
}

/// The word of the last lookup that was shown, kept so that it can be added to Anki.
#[derive(Debug, Default)]
pub struct HoverMemory(Mutex<Option<Remembered>>);

impl HoverMemory {
    pub fn set(&self, result: HoverResult) {
        self.set_with_screenshot(result, None);
    }

    pub fn set_with_screenshot(&self, result: HoverResult, screenshot: Option<RgbImage>) {
        self.set_with_origin(result, screenshot, WindowIdentity::default());
    }

    pub fn set_with_origin(
        &self,
        result: HoverResult,
        screenshot: Option<RgbImage>,
        identity: WindowIdentity,
    ) {
        if let Ok(mut current) = self.0.lock() {
            *current = Some(Remembered {
                result,
                screenshot,
                identity,
            });
        }
    }

    pub fn identity(&self) -> WindowIdentity {
        self.0
            .lock()
            .ok()
            .and_then(|current| {
                current
                    .as_ref()
                    .map(|remembered| remembered.identity.clone())
            })
            .unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut current) = self.0.lock() {
            *current = None;
        }
    }

    pub fn get(&self) -> Option<HoverResult> {
        self.0
            .lock()
            .ok()
            .and_then(|current| current.as_ref().map(|remembered| remembered.result.clone()))
    }

    pub fn screenshot(&self) -> Option<RgbImage> {
        self.0.lock().ok().and_then(|current| {
            current
                .as_ref()
                .and_then(|remembered| remembered.screenshot.clone())
        })
    }
}

/// The card for `result.results[result_index].entries[entry_index]`: the sentence of the lookup
/// with the matched text highlighted. `None` if there is no such entry.
pub fn card_input(
    result: &HoverResult,
    result_index: usize,
    entry_index: usize,
) -> Option<CardInput> {
    let matched = result.results.get(result_index)?;
    let found = matched.entries.get(entry_index)?;
    Some(CardInput {
        entry: found.entry.clone(),
        sentence: result.sentence.clone(),
        highlight: Some((result.word_start, result.word_start + matched.length)),
        source: CARD_SOURCE.to_string(),
        audio: None,
        picture: None,
        word_audio: None,
    })
}

/// Looks a sentence up from a UTF-16 offset (see `AppState::scan_at`).
pub type SentenceScan<'a> = dyn Fn(&str, usize) -> Result<Vec<ScanResult>, String> + 'a;

/// The word found under the cursor, ready for the popup.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverResult {
    /// The OCR line the word is in.
    pub sentence: String,
    /// Where the word starts in `sentence` (UTF-16 code units).
    pub word_start: usize,
    /// Dictionary matches for the text starting at `word_start`, longest first.
    pub results: Vec<ScanResult>,
    /// `[x1, y1, x2, y2]` of the character under the cursor, in screen pixels.
    pub char_bbox: [i32; 4],
    /// `[x1, y1, x2, y2]` of the whole line, in screen pixels.
    pub line_bbox: [i32; 4],
    /// Whether the text is a vertical column (the popup then goes beside it, not under it).
    pub vertical: bool,
}

/// What a lookup found at the cursor; the reasons for finding nothing are told to the user.
#[derive(Debug, Clone, PartialEq)]
pub enum LookupOutcome {
    Found(HoverResult),
    /// No character of recognized text is at the cursor; `lines` is how many lines the OCR read
    /// near it.
    NoText {
        lines: usize,
    },
    /// The character under the cursor is punctuation or a symbol.
    NotAWord {
        text: String,
    },
}

impl LookupOutcome {
    /// A short sentence for the user when nothing was found; `None` when a word was found.
    pub fn explanation(&self) -> Option<String> {
        match self {
            LookupOutcome::Found(result) if result.results.is_empty() => {
                let word: String = result
                    .sentence
                    .chars()
                    .skip(char_index(&result.sentence, result.word_start))
                    .take(2)
                    .collect();
                Some(format!(
                    "No dictionary entry for the word starting with \"{word}\""
                ))
            }
            LookupOutcome::Found(_) => None,
            LookupOutcome::NoText { lines: 0 } => {
                Some("No text recognized under the cursor".to_string())
            }
            LookupOutcome::NoText { lines } => Some(format!(
                "The cursor is not on a character ({lines} line(s) of text were read near it)"
            )),
            LookupOutcome::NotAWord { text } => Some(format!(
                "No word under the cursor (punctuation in \"{text}\")"
            )),
        }
    }
}

/// Index in characters of a UTF-16 offset of `text`.
fn char_index(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0;
    for (index, character) in text.chars().enumerate() {
        if units >= utf16_offset {
            return index;
        }
        units += character.len_utf16();
    }
    text.chars().count()
}

/// Reads the whole screen and finds the dictionary entries for the word under `cursor`.
///
/// `scan` looks a sentence up from a UTF-16 offset (see `AppState::scan_at`).
pub fn lookup_at(
    capture: &mut dyn ScreenCapture,
    engine: &mut dyn OcrEngine,
    tokenizer: &Tokenizer,
    scan: &SentenceScan,
    cursor: (i32, i32),
) -> Result<LookupOutcome, HoverError> {
    let (width, height) = capture.screen_size();
    let screen = capture.capture(Region {
        x: 0,
        y: 0,
        width,
        height,
    })?;
    lookup_in(&screen, engine, tokenizer, scan, cursor)
}

/// Finds the dictionary entries for the word under `cursor` in an image of the whole screen.
pub fn lookup_in(
    screen: &RgbImage,
    engine: &mut dyn OcrEngine,
    tokenizer: &Tokenizer,
    scan: &SentenceScan,
    cursor: (i32, i32),
) -> Result<LookupOutcome, HoverError> {
    let lines = engine.recognize_near(screen, cursor)?;

    let Some(hit) = character_at(&lines, cursor) else {
        return Ok(LookupOutcome::NoText { lines: lines.len() });
    };
    let tokens = tokenizer.tokenize(&hit.text)?;
    let Some(token) = word_at(&tokens, hit.offset) else {
        return Ok(LookupOutcome::NotAWord { text: hit.text });
    };
    let results = scan(&hit.text, token.start).map_err(HoverError::Dictionary)?;
    Ok(LookupOutcome::Found(HoverResult {
        sentence: hit.text,
        word_start: token.start,
        results,
        char_bbox: hit.char_bbox,
        line_bbox: hit.line_bbox,
        vertical: hit.vertical,
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use image::{Rgb, RgbImage};

    use super::*;
    use crate::dictionary::Dictionary;
    use crate::normalizer::clean_text;
    use crate::ocr::{MeikiEngine, OcrLine};
    use crate::text_offsets::utf16_offset_to_byte;

    const SCREEN_WIDTH: u32 = 1920;
    const SCREEN_HEIGHT: u32 = 1080;
    const BAND_TOP: u32 = 700;
    const FIXTURE: &str = "00_1080p_outline_busy__jpeg25.png";
    const FIXTURE_TEXT: &str = "昨日、友達と映画を見に行きました。";

    const DICTIONARY_JSON: &str = r#"{ "tags": {}, "words": [
        { "id": "1", "kanji": [{ "common": true, "text": "友達", "tags": [] }],
          "kana": [{ "common": true, "text": "ともだち", "tags": [], "appliesToKanji": ["*"] }],
          "sense": [{ "partOfSpeech": ["n"], "gloss": [{ "text": "friend" }] }] },
        { "id": "2", "kanji": [{ "common": true, "text": "映画", "tags": [] }],
          "kana": [{ "common": true, "text": "えいが", "tags": [], "appliesToKanji": ["*"] }],
          "sense": [{ "partOfSpeech": ["n"], "gloss": [{ "text": "movie" }] }] },
        { "id": "4", "kanji": [{ "common": true, "text": "勉強", "tags": [] }],
          "kana": [{ "common": true, "text": "べんきょう", "tags": [], "appliesToKanji": ["*"] }],
          "sense": [{ "partOfSpeech": ["n", "vs"], "gloss": [{ "text": "study" }] }] },
        { "id": "5", "kanji": [{ "common": true, "text": "世界", "tags": [] }],
          "kana": [{ "common": true, "text": "せかい", "tags": [], "appliesToKanji": ["*"] }],
          "sense": [{ "partOfSpeech": ["n"], "gloss": [{ "text": "world" }] }] },
        { "id": "3", "kanji": [{ "common": true, "text": "行く", "tags": [] }],
          "kana": [{ "common": true, "text": "いく", "tags": [], "appliesToKanji": ["*"] }],
          "sense": [{ "partOfSpeech": ["v5k-s", "vi"], "gloss": [{ "text": "to go" }] }] }
    ] }"#;

    /// A screen that always returns the same prepared image.
    struct FakeScreen(RgbImage);

    impl ScreenCapture for FakeScreen {
        fn screen_size(&self) -> (u32, u32) {
            (self.0.width(), self.0.height())
        }

        fn capture(&mut self, region: Region) -> Result<RgbImage, CaptureError> {
            assert_eq!(
                region,
                Region {
                    x: 0,
                    y: 0,
                    width: self.0.width(),
                    height: self.0.height()
                }
            );
            Ok(self.0.clone())
        }
    }

    struct BrokenScreen;

    impl ScreenCapture for BrokenScreen {
        fn screen_size(&self) -> (u32, u32) {
            (100, 100)
        }

        fn capture(&mut self, _region: Region) -> Result<RgbImage, CaptureError> {
            Err(CaptureError::Failed("no access".to_string()))
        }
    }

    struct FixedEngine(Vec<OcrLine>);

    impl OcrEngine for FixedEngine {
        fn recognize(&mut self, _image: &RgbImage) -> Result<Vec<OcrLine>, OcrError> {
            Ok(self.0.clone())
        }
    }

    struct BrokenEngine;

    impl OcrEngine for BrokenEngine {
        fn recognize(&mut self, _image: &RgbImage) -> Result<Vec<OcrLine>, OcrError> {
            Err(OcrError::Inference("boom".to_string()))
        }
    }

    fn real_engine() -> MeikiEngine {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ocr");
        MeikiEngine::from_directory(&directory)
            .unwrap_or_else(|error| panic!("{error}; run scripts/fetch-ocr-models.sh"))
    }

    /// A black Full HD screen with a fixture pasted near the bottom, or higher when it is tall
    /// enough not to fit there.
    fn screen_with(fixture: &str) -> RgbImage {
        let band = image::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/ocr")
                .join(fixture),
        )
        .expect("fixture")
        .to_rgb8();
        let top = BAND_TOP.min(SCREEN_HEIGHT - band.height());
        let mut screen = RgbImage::from_pixel(SCREEN_WIDTH, SCREEN_HEIGHT, Rgb([0, 0, 0]));
        image::imageops::replace(&mut screen, &band, 0, i64::from(top));
        screen
    }

    fn screen_with_subtitle() -> RgbImage {
        screen_with(FIXTURE)
    }

    fn dictionary_scan() -> impl Fn(&str, usize) -> Result<Vec<ScanResult>, String> {
        let mut dictionary = Dictionary::open_in_memory().expect("dictionary");
        dictionary
            .import_jmdict(DICTIONARY_JSON.as_bytes())
            .expect("import");
        move |sentence, start| {
            let byte =
                utf16_offset_to_byte(sentence, start).ok_or("offset outside the sentence")?;
            dictionary
                .scan(&sentence[byte..])
                .map_err(|error| error.to_string())
        }
    }

    fn found(outcome: LookupOutcome) -> HoverResult {
        match outcome {
            LookupOutcome::Found(result) => result,
            other => panic!("expected a word, got {other:?}"),
        }
    }

    fn center(bbox: [i32; 4]) -> (i32, i32) {
        ((bbox[0] + bbox[2]) / 2, (bbox[1] + bbox[3]) / 2)
    }

    #[test]
    fn finds_the_word_under_the_cursor_on_a_real_screenshot() {
        let mut screen = FakeScreen(screen_with_subtitle());
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();
        // Aim the cursor at the middle of the character 友 (index 3), found by the same OCR.
        let lines = engine.recognize(&screen_with_subtitle()).expect("ocr");
        let cursor = center(lines[0].chars[3].bbox);

        let result = lookup_at(&mut screen, &mut engine, &tokenizer, &scan, cursor)
            .map(found)
            .expect("lookup");

        assert_eq!(result.sentence, FIXTURE_TEXT);
        assert_eq!(result.word_start, 3);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].source, "友達");
        assert_eq!(result.results[0].length, 2);
        assert_eq!(result.results[0].entries[0].entry.expression, "友達");
        assert_eq!(result.results[0].entries[0].entry.reading, "ともだち");
        assert_eq!(
            result.results[0].entries[0].entry.meanings,
            vec!["friend".to_string()]
        );
        assert_eq!(result.char_bbox, lines[0].chars[3].bbox);
        assert_eq!(result.line_bbox, lines[0].bbox);
        assert!(
            result.line_bbox[1] >= BAND_TOP as i32,
            "the line is inside the pasted band"
        );
    }

    #[test]
    fn resolves_an_inflected_verb_to_its_dictionary_form() {
        let mut screen = FakeScreen(screen_with_subtitle());
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();
        let lines = engine.recognize(&screen_with_subtitle()).expect("ocr");
        // 行きました: 行 is the character at index 11.
        let cursor = center(lines[0].chars[11].bbox);

        let result = lookup_at(&mut screen, &mut engine, &tokenizer, &scan, cursor)
            .map(found)
            .expect("lookup");

        assert_eq!(result.word_start, 11);
        assert_eq!(result.results[0].source, "行きました");
        assert_eq!(result.results[0].entries[0].entry.expression, "行く");
        assert_eq!(result.results[0].entries[0].reasons, vec!["past", "polite"]);
    }

    #[test]
    fn a_word_without_a_dictionary_entry_gives_an_empty_result_list() {
        let mut screen = FakeScreen(screen_with_subtitle());
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();
        let lines = engine.recognize(&screen_with_subtitle()).expect("ocr");
        // 昨日 is not in the small test dictionary.
        let cursor = center(lines[0].chars[0].bbox);

        let result = lookup_at(&mut screen, &mut engine, &tokenizer, &scan, cursor)
            .map(found)
            .expect("lookup");

        assert_eq!(result.word_start, 0);
        assert_eq!(result.results, vec![]);
    }

    #[test]
    fn the_cursor_on_empty_screen_finds_nothing() {
        let mut screen = FakeScreen(screen_with_subtitle());
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();

        let result =
            lookup_at(&mut screen, &mut engine, &tokenizer, &scan, (200, 100)).expect("lookup");

        assert_eq!(result, LookupOutcome::NoText { lines: 0 });
    }

    fn line_of(text: &str, x: i32) -> OcrLine {
        use crate::ocr::OcrChar;
        let chars: Vec<OcrChar> = text
            .chars()
            .enumerate()
            .map(|(index, character)| OcrChar {
                character,
                bbox: [x + index as i32 * 40, 100, x + (index as i32 + 1) * 40, 150],
                confidence: 0.9,
            })
            .collect();
        OcrLine {
            text: text.to_string(),
            bbox: [x, 100, x + chars.len() as i32 * 40, 150],
            chars,
            vertical: false,
        }
    }

    fn blank_screen() -> FakeScreen {
        FakeScreen(RgbImage::new(400, 300))
    }

    fn unused_scan() -> impl Fn(&str, usize) -> Result<Vec<ScanResult>, String> {
        |_, _| Ok(vec![])
    }

    #[test]
    fn punctuation_under_the_cursor_is_not_a_word() {
        let mut engine = FixedEngine(vec![line_of("猫。", 0)]);
        let tokenizer = Tokenizer::new().expect("tokenizer");

        let result = lookup_at(
            &mut blank_screen(),
            &mut engine,
            &tokenizer,
            &unused_scan(),
            (60, 120),
        );

        assert_eq!(
            result.expect("lookup"),
            LookupOutcome::NotAWord {
                text: "猫。".to_string()
            }
        );
    }

    #[test]
    fn the_scan_receives_the_sentence_and_the_offset_of_the_token() {
        let mut engine = FixedEngine(vec![line_of("私は猫が好き", 0)]);
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let calls = std::cell::RefCell::new(Vec::new());
        let scan = |sentence: &str, start: usize| {
            calls.borrow_mut().push((sentence.to_string(), start));
            Ok(vec![])
        };

        // The cursor is on 猫 (third character).
        lookup_at(
            &mut blank_screen(),
            &mut engine,
            &tokenizer,
            &scan,
            (100, 120),
        )
        .expect("lookup");

        assert_eq!(*calls.borrow(), vec![("私は猫が好き".to_string(), 2)]);
    }

    #[test]
    fn errors_from_the_screen_the_ocr_and_the_dictionary_are_reported() {
        let tokenizer = Tokenizer::new().expect("tokenizer");

        let capture = lookup_at(
            &mut BrokenScreen,
            &mut FixedEngine(vec![]),
            &tokenizer,
            &unused_scan(),
            (1, 1),
        );
        let ocr = lookup_at(
            &mut blank_screen(),
            &mut BrokenEngine,
            &tokenizer,
            &unused_scan(),
            (1, 1),
        );
        let failing_scan = |_: &str, _: usize| Err("database locked".to_string());
        let dictionary = lookup_at(
            &mut blank_screen(),
            &mut FixedEngine(vec![line_of("猫", 0)]),
            &tokenizer,
            &failing_scan,
            (10, 120),
        );

        assert_eq!(
            capture.expect_err("capture").to_string(),
            "screen capture failed: no access"
        );
        assert_eq!(
            ocr.expect_err("ocr").to_string(),
            "OCR inference failed: boom"
        );
        assert_eq!(
            dictionary.expect_err("dictionary").to_string(),
            "dictionary lookup failed: database locked"
        );
    }

    #[test]
    fn a_result_serializes_with_camel_case_keys() {
        let result = HoverResult {
            sentence: "猫".to_string(),
            word_start: 0,
            results: vec![],
            char_bbox: [1, 2, 3, 4],
            line_bbox: [0, 2, 10, 4],
            vertical: false,
        };

        assert_eq!(
            serde_json::to_value(result).expect("serialize"),
            serde_json::json!({
                "sentence": "猫",
                "wordStart": 0,
                "results": [],
                "charBbox": [1, 2, 3, 4],
                "lineBbox": [0, 2, 10, 4],
                "vertical": false
            })
        );
    }

    #[test]
    fn a_cursor_beside_text_reports_how_many_lines_were_read_nearby() {
        let mut engine = FixedEngine(vec![line_of("猫", 0)]);
        let tokenizer = Tokenizer::new().expect("tokenizer");

        let outcome = lookup_at(
            &mut blank_screen(),
            &mut engine,
            &tokenizer,
            &unused_scan(),
            (350, 120),
        )
        .expect("lookup");

        assert_eq!(outcome, LookupOutcome::NoText { lines: 1 });
    }

    #[test]
    fn explanations_tell_the_user_why_nothing_was_shown() {
        let empty_entry = LookupOutcome::Found(HoverResult {
            sentence: "私は猫が好き".to_string(),
            word_start: 2,
            results: vec![],
            char_bbox: [0, 0, 1, 1],
            line_bbox: [0, 0, 1, 1],
            vertical: false,
        });

        assert_eq!(
            empty_entry.explanation(),
            Some("No dictionary entry for the word starting with \"猫が\"".to_string())
        );
        assert_eq!(
            LookupOutcome::NoText { lines: 0 }.explanation(),
            Some("No text recognized under the cursor".to_string())
        );
        assert_eq!(
            LookupOutcome::NoText { lines: 3 }.explanation(),
            Some(
                "The cursor is not on a character (3 line(s) of text were read near it)"
                    .to_string()
            )
        );
        assert_eq!(
            LookupOutcome::NotAWord {
                text: "猫。".to_string()
            }
            .explanation(),
            Some("No word under the cursor (punctuation in \"猫。\")".to_string())
        );
    }

    #[test]
    fn a_word_with_entries_needs_no_explanation() {
        let found_word = LookupOutcome::Found(HoverResult {
            sentence: "猫".to_string(),
            word_start: 0,
            results: vec![crate::dictionary::ScanResult {
                source: "猫".to_string(),
                length: 1,
                entries: vec![],
            }],
            char_bbox: [0, 0, 1, 1],
            line_bbox: [0, 0, 1, 1],
            vertical: false,
        });

        assert_eq!(found_word.explanation(), None);
    }

    #[test]
    fn char_index_converts_utf16_offsets() {
        assert_eq!(char_index("😀猫が", 0), 0);
        assert_eq!(char_index("😀猫が", 2), 1);
        assert_eq!(char_index("😀猫が", 3), 2);
        assert_eq!(char_index("猫", 5), 1);
    }

    #[test]
    fn hovering_the_second_line_of_a_subtitle_uses_both_lines_as_the_sentence() {
        let screen_image = screen_with("twoline_1080p.png");
        let mut screen = FakeScreen(screen_image.clone());
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();
        let lines = engine.recognize(&screen_image).expect("ocr");
        assert_eq!(lines.len(), 2, "the fixture has two lines");
        // 行 is the first character of the second line.
        let cursor = center(lines[1].chars[0].bbox);

        let result =
            found(lookup_at(&mut screen, &mut engine, &tokenizer, &scan, cursor).expect("lookup"));

        assert_eq!(
            clean_text(&result.sentence),
            "昨日、友達と映画を見に行きました。とても楽しかったです"
        );
        // The first line has 11 characters, so 行 is at offset 11.
        assert_eq!(result.word_start, 11);
        assert_eq!(result.results[0].source, "行きました");
        assert_eq!(result.results[0].entries[0].entry.expression, "行く");
        assert_eq!(result.line_bbox, lines[1].bbox);
    }

    #[test]
    fn hovering_the_first_line_finds_a_word_that_ends_on_the_next_line_context() {
        let screen_image = screen_with("twoline_1080p.png");
        let mut screen = FakeScreen(screen_image.clone());
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();
        let lines = engine.recognize(&screen_image).expect("ocr");
        // 友 is the fourth character of the first line.
        let cursor = center(lines[0].chars[3].bbox);

        let result =
            found(lookup_at(&mut screen, &mut engine, &tokenizer, &scan, cursor).expect("lookup"));

        assert_eq!(result.word_start, 3);
        assert_eq!(result.results[0].source, "友達");
        assert_eq!(
            clean_text(&result.sentence),
            "昨日、友達と映画を見に行きました。とても楽しかったです"
        );
    }

    #[test]
    fn hovering_a_vertical_column_finds_the_word_and_joins_the_columns_right_to_left() {
        let screen_image = screen_with("vertical_clean_5.png");
        let mut screen = FakeScreen(screen_image.clone());
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();
        let lines = engine.recognize(&screen_image).expect("ocr");
        assert_eq!(lines.len(), 2, "the fixture has two columns");
        assert!(lines.iter().all(|line| line.vertical));
        // The right column is read first: 日本語を勉強しています. 勉 is its fifth character.
        let right = lines
            .iter()
            .max_by_key(|line| line.bbox[2])
            .expect("right column");
        let cursor = center(right.chars[4].bbox);

        let result =
            found(lookup_at(&mut screen, &mut engine, &tokenizer, &scan, cursor).expect("lookup"));

        assert!(result.vertical);
        assert_eq!(result.sentence, "日本語を勉強しています世界で一番大切な人");
        assert_eq!(result.word_start, 4);
        assert_eq!(result.results[0].source, "勉強");
        assert_eq!(
            result.results[0].entries[0].entry.meanings,
            vec!["study".to_string()]
        );
        assert_eq!(result.line_bbox, right.bbox);
    }

    #[test]
    fn hovering_the_second_column_continues_the_sentence_after_the_first() {
        let screen_image = screen_with("vertical_clean_5.png");
        let mut screen = FakeScreen(screen_image.clone());
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();
        let lines = engine.recognize(&screen_image).expect("ocr");
        let left = lines
            .iter()
            .min_by_key(|line| line.bbox[0])
            .expect("left column");
        // 世 is the first character of the left column, after the eleven of the right one.
        let cursor = center(left.chars[0].bbox);

        let result =
            found(lookup_at(&mut screen, &mut engine, &tokenizer, &scan, cursor).expect("lookup"));

        assert_eq!(result.word_start, 11);
        assert_eq!(result.results[0].source, "世界");
    }

    // --- memory and cards ---

    fn two_entry_result() -> HoverResult {
        let entry = |expression: &str, reading: &str| crate::dictionary::DeinflectedEntry {
            entry: crate::types::entry::Entry {
                expression: expression.to_string(),
                reading: reading.to_string(),
                meanings: vec!["meaning".to_string()],
                parts_of_speech: vec!["n".to_string()],
                frequency: None,
                pitch: Vec::new(),
            },
            source: expression.to_string(),
            reasons: vec![],
        };
        HoverResult {
            sentence: "昨日、友達と映画".to_string(),
            word_start: 3,
            results: vec![
                crate::dictionary::ScanResult {
                    source: "友達".to_string(),
                    length: 2,
                    entries: vec![entry("友達", "ともだち"), entry("友だち", "ともだち")],
                },
                crate::dictionary::ScanResult {
                    source: "友".to_string(),
                    length: 1,
                    entries: vec![entry("友", "とも")],
                },
            ],
            char_bbox: [0, 0, 1, 1],
            line_bbox: [0, 0, 10, 2],
            vertical: false,
        }
    }

    #[test]
    fn the_card_highlights_the_matched_text_of_the_chosen_result() {
        let result = two_entry_result();

        let longest = card_input(&result, 0, 1).expect("card");
        let shortest = card_input(&result, 1, 0).expect("card");

        assert_eq!(longest.entry.expression, "友だち");
        assert_eq!(longest.sentence, "昨日、友達と映画");
        assert_eq!(longest.highlight, Some((3, 5)));
        assert_eq!(longest.source, "screen");
        assert_eq!((longest.audio, longest.picture), (None, None));
        assert_eq!(shortest.entry.expression, "友");
        assert_eq!(shortest.highlight, Some((3, 4)));
    }

    #[test]
    fn there_is_no_card_for_an_entry_that_does_not_exist() {
        let result = two_entry_result();

        assert_eq!(card_input(&result, 2, 0), None);
        assert_eq!(card_input(&result, 0, 2), None);
        let empty = HoverResult {
            results: vec![],
            ..result
        };
        assert_eq!(card_input(&empty, 0, 0), None);
    }

    #[test]
    fn the_memory_keeps_the_last_word_until_it_is_cleared() {
        let memory = HoverMemory::default();
        assert_eq!(memory.get(), None);

        memory.set(two_entry_result());
        assert_eq!(memory.get(), Some(two_entry_result()));

        let other = HoverResult {
            sentence: "別".to_string(),
            ..two_entry_result()
        };
        memory.set(other.clone());
        assert_eq!(memory.get(), Some(other));

        memory.clear();
        assert_eq!(memory.get(), None);
    }

    #[test]
    fn the_memory_keeps_the_screenshot_with_the_word_and_drops_it_together() {
        let memory = HoverMemory::default();
        assert!(memory.screenshot().is_none());

        memory.set(two_entry_result());
        assert!(memory.screenshot().is_none());

        let window = RgbImage::from_pixel(8, 4, Rgb([9, 9, 9]));
        memory.set_with_screenshot(two_entry_result(), Some(window.clone()));
        assert_eq!(memory.screenshot(), Some(window));
        assert_eq!(memory.get(), Some(two_entry_result()));

        memory.clear();
        assert!(memory.screenshot().is_none());
    }

    #[test]
    fn lookup_in_gives_the_same_answer_as_lookup_at_for_the_same_screen() {
        let screen_image = screen_with_subtitle();
        let mut engine = real_engine();
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let scan = dictionary_scan();
        let lines = engine.recognize(&screen_image).expect("ocr");
        let cursor = center(lines[0].chars[3].bbox);

        let direct = lookup_in(&screen_image, &mut engine, &tokenizer, &scan, cursor).expect("in");
        let through_capture = lookup_at(
            &mut FakeScreen(screen_image.clone()),
            &mut engine,
            &tokenizer,
            &scan,
            cursor,
        )
        .expect("at");

        assert_eq!(direct, through_capture);
        assert_eq!(found(direct).results[0].source, "友達");
    }

    #[test]
    fn the_memory_keeps_the_application_the_word_was_found_in() {
        let memory = HoverMemory::default();
        assert_eq!(memory.identity(), WindowIdentity::default());

        let identity = WindowIdentity {
            app_name: Some("mpv".to_string()),
            title: Some("episode 1".to_string()),
        };
        memory.set_with_origin(two_entry_result(), None, identity.clone());
        assert_eq!(memory.identity(), identity);

        memory.set(two_entry_result());
        assert_eq!(memory.identity(), WindowIdentity::default());

        memory.set_with_origin(two_entry_result(), None, identity);
        memory.clear();
        assert_eq!(memory.identity(), WindowIdentity::default());
    }
}
