//! Finds which OCR line, character and token lie under the mouse cursor.

use serde::Serialize;

use crate::ocr::OcrLine;
use crate::types::token::Token;

/// Part-of-speech values that are never worth looking up (same list as the frontend).
const NON_WORD_POS: [&str; 3] = ["補助記号", "記号", "空白"];

/// How far (in pixels) above or below an OCR line the cursor may be and still count as on it.
pub const LINE_MARGIN: i32 = 6;

/// The character under the cursor and the line it belongs to.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterHit {
    /// Text of the line (the sentence the word belongs to).
    pub text: String,
    /// Offset of the character in `text`, in UTF-16 code units.
    pub offset: usize,
    /// `[x1, y1, x2, y2]` of the character, in screen pixels.
    pub char_bbox: [i32; 4],
    /// `[x1, y1, x2, y2]` of the line, in screen pixels.
    pub line_bbox: [i32; 4],
}

/// Finds the character at `cursor` (screen pixels). When the cursor is between two lines the
/// nearest one wins; within a line the nearest character wins if it is close enough.
pub fn character_at(lines: &[OcrLine], cursor: (i32, i32)) -> Option<CharacterHit> {
    let (cursor_x, cursor_y) = cursor;
    let line = lines
        .iter()
        .filter(|line| {
            let [_, top, _, bottom] = line.bbox;
            cursor_y >= top - LINE_MARGIN && cursor_y <= bottom + LINE_MARGIN
        })
        .min_by_key(|line| {
            let [left, top, right, bottom] = line.bbox;
            // Closest vertically first; for lines at the same height, the closest horizontally.
            (
                (cursor_y - (top + bottom) / 2).abs(),
                horizontal_distance([left, top, right, bottom], cursor_x),
            )
        })?;

    let line_height = line.bbox[3] - line.bbox[1];
    let (index, distance) = line
        .chars
        .iter()
        .enumerate()
        .map(|(index, ocr_char)| (index, horizontal_distance(ocr_char.bbox, cursor_x)))
        .min_by_key(|(_, distance)| *distance)?;
    if distance > line_height / 2 {
        return None;
    }

    let offset: usize = line.chars[..index]
        .iter()
        .map(|ocr_char| ocr_char.character.len_utf16())
        .sum();
    Some(CharacterHit {
        text: line.text.clone(),
        offset,
        char_bbox: line.chars[index].bbox,
        line_bbox: line.bbox,
    })
}

/// 0 when `x` is inside the box, otherwise the distance to its nearest side.
fn horizontal_distance(bbox: [i32; 4], x: i32) -> i32 {
    if x < bbox[0] {
        bbox[0] - x
    } else if x >= bbox[2] {
        x - bbox[2] + 1
    } else {
        0
    }
}

/// The word token that contains the UTF-16 `offset`; punctuation and symbols do not count.
pub fn word_at(tokens: &[Token], offset: usize) -> Option<&Token> {
    tokens
        .iter()
        .find(|token| token.start <= offset && offset < token.end)
        .filter(|token| !NON_WORD_POS.contains(&token.pos.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::OcrChar;

    /// A horizontal line of `text` with `width`-pixel characters starting at `(x, y)`.
    fn line(text: &str, x: i32, y: i32, width: i32, height: i32) -> OcrLine {
        let chars: Vec<OcrChar> = text
            .chars()
            .enumerate()
            .map(|(index, character)| OcrChar {
                character,
                bbox: [
                    x + index as i32 * width,
                    y,
                    x + (index as i32 + 1) * width,
                    y + height,
                ],
                confidence: 0.9,
            })
            .collect();
        OcrLine {
            text: text.to_string(),
            bbox: [x, y, x + chars.len() as i32 * width, y + height],
            chars,
        }
    }

    fn token(surface: &str, pos: &str, start: usize, end: usize) -> Token {
        Token {
            surface: surface.to_string(),
            lemma: surface.to_string(),
            reading: String::new(),
            pos: pos.to_string(),
            start,
            end,
        }
    }

    #[test]
    fn finds_the_character_under_the_cursor() {
        let lines = [line("猫が好き", 100, 900, 40, 50)];

        let hit = character_at(&lines, (185, 920)).expect("hit");

        assert_eq!(
            hit,
            CharacterHit {
                text: "猫が好き".to_string(),
                offset: 2,
                char_bbox: [180, 900, 220, 950],
                line_bbox: [100, 900, 260, 950],
            }
        );
    }

    #[test]
    fn the_first_and_last_characters_are_reachable() {
        let lines = [line("猫が好き", 100, 900, 40, 50)];

        assert_eq!(character_at(&lines, (100, 920)).expect("first").offset, 0);
        assert_eq!(character_at(&lines, (259, 920)).expect("last").offset, 3);
    }

    #[test]
    fn the_offset_counts_utf16_units_for_characters_outside_the_bmp() {
        let lines = [line("😀猫", 0, 0, 40, 50)];

        assert_eq!(character_at(&lines, (60, 20)).expect("hit").offset, 2);
    }

    #[test]
    fn the_cursor_slightly_past_the_end_of_the_line_still_picks_the_last_character() {
        let lines = [line("猫が好き", 100, 900, 40, 50)];

        assert_eq!(character_at(&lines, (270, 920)).expect("hit").offset, 3);
        assert_eq!(character_at(&lines, (90, 920)).expect("hit").offset, 0);
    }

    #[test]
    fn the_cursor_far_from_the_text_hits_nothing() {
        let lines = [line("猫が好き", 100, 900, 40, 50)];

        assert_eq!(character_at(&lines, (400, 920)), None);
        assert_eq!(character_at(&lines, (40, 920)), None);
    }

    #[test]
    fn the_cursor_above_or_below_the_line_hits_nothing_beyond_the_margin() {
        let lines = [line("猫が好き", 100, 900, 40, 50)];

        assert!(character_at(&lines, (120, 900 - LINE_MARGIN)).is_some());
        assert!(character_at(&lines, (120, 950 + LINE_MARGIN)).is_some());
        assert_eq!(character_at(&lines, (120, 900 - LINE_MARGIN - 1)), None);
        assert_eq!(character_at(&lines, (120, 950 + LINE_MARGIN + 1)), None);
    }

    #[test]
    fn the_nearest_line_wins_when_the_margins_overlap() {
        let lines = [
            line("上の行", 100, 900, 40, 50),
            line("下の行", 100, 953, 40, 50),
        ];

        assert_eq!(
            character_at(&lines, (120, 948)).expect("hit").text,
            "上の行"
        );
        assert_eq!(
            character_at(&lines, (120, 958)).expect("hit").text,
            "下の行"
        );
    }

    #[test]
    fn lines_side_by_side_are_told_apart_by_the_horizontal_position() {
        let lines = [line("左", 100, 900, 40, 50), line("右", 900, 900, 40, 50)];

        assert_eq!(character_at(&lines, (905, 920)).expect("hit").text, "右");
    }

    #[test]
    fn no_lines_or_lines_without_characters_hit_nothing() {
        assert_eq!(character_at(&[], (10, 10)), None);
        let empty = OcrLine {
            text: String::new(),
            bbox: [0, 0, 100, 40],
            chars: vec![],
        };
        assert_eq!(character_at(&[empty], (10, 10)), None);
    }

    #[test]
    fn word_at_returns_the_token_that_contains_the_offset() {
        let tokens = [
            token("猫", "名詞", 0, 1),
            token("が", "助詞", 1, 2),
            token("好き", "形状詞", 2, 4),
        ];

        assert_eq!(word_at(&tokens, 0).map(|t| t.surface.as_str()), Some("猫"));
        assert_eq!(
            word_at(&tokens, 3).map(|t| t.surface.as_str()),
            Some("好き")
        );
        assert_eq!(
            word_at(&tokens, 2).map(|t| t.surface.as_str()),
            Some("好き")
        );
    }

    #[test]
    fn word_at_skips_punctuation_and_offsets_outside_the_tokens() {
        let tokens = [token("猫", "名詞", 0, 1), token("。", "補助記号", 1, 2)];

        assert_eq!(word_at(&tokens, 1), None);
        assert_eq!(word_at(&tokens, 2), None);
        assert_eq!(word_at(&[], 0), None);
    }

    #[test]
    fn particles_and_auxiliaries_are_words() {
        let tokens = [token("です", "助動詞", 0, 2), token("は", "助詞", 2, 3)];

        assert!(word_at(&tokens, 0).is_some());
        assert!(word_at(&tokens, 2).is_some());
    }

    #[test]
    fn a_hit_serializes_with_camel_case_keys() {
        let hit = character_at(&[line("猫", 0, 0, 40, 50)], (10, 10)).expect("hit");

        assert_eq!(
            serde_json::to_value(hit).expect("serialize"),
            serde_json::json!({
                "text": "猫",
                "offset": 0,
                "charBbox": [0, 0, 40, 50],
                "lineBbox": [0, 0, 40, 50]
            })
        );
    }
}
