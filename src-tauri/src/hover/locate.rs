//! Finds which OCR line, character and token lie under the mouse cursor.

use serde::Serialize;

use crate::ocr::layout::block_around;
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
    /// Text of the block the line belongs to: the lines of a multi-line subtitle joined together
    /// (the sentence the word belongs to).
    pub text: String,
    /// Offset of the character in `text`, in UTF-16 code units.
    pub offset: usize,
    /// `[x1, y1, x2, y2]` of the character, in screen pixels.
    pub char_bbox: [i32; 4],
    /// `[x1, y1, x2, y2]` of the hovered line, in screen pixels.
    pub line_bbox: [i32; 4],
    /// Whether the hovered text is a vertical column.
    pub vertical: bool,
}

/// Finds the character at `cursor` (screen pixels). When the cursor is between two lines the
/// nearest one wins; within a line the nearest character wins if it is close enough. Works for
/// horizontal lines and vertical columns alike.
pub fn character_at(lines: &[OcrLine], cursor: (i32, i32)) -> Option<CharacterHit> {
    let (line_index, line) = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| Some((index, line, distances_to_line(line, cursor)?)))
        .min_by_key(|(_, _, distances)| *distances)
        .map(|(index, line, _)| (index, line))?;

    let thickness = if line.vertical {
        line.bbox[2] - line.bbox[0]
    } else {
        line.bbox[3] - line.bbox[1]
    };
    let along = if line.vertical { cursor.1 } else { cursor.0 };
    let (index, distance) = line
        .chars
        .iter()
        .enumerate()
        .map(|(index, ocr_char)| {
            let (start, end) = if line.vertical {
                (ocr_char.bbox[1], ocr_char.bbox[3])
            } else {
                (ocr_char.bbox[0], ocr_char.bbox[2])
            };
            (index, axis_distance(start, end, along))
        })
        .min_by_key(|(_, distance)| *distance)?;
    if distance > thickness / 2 {
        return None;
    }

    let offset_in_line: usize = line.chars[..index]
        .iter()
        .map(|ocr_char| ocr_char.character.len_utf16())
        .sum();
    let (text, offset_of_line) = join_block(lines, line_index);
    Some(CharacterHit {
        text,
        offset: offset_of_line + offset_in_line,
        char_bbox: line.chars[index].bbox,
        line_bbox: line.bbox,
        vertical: line.vertical,
    })
}

/// How far the cursor is from a line, or `None` if it is too far across the line to be on it.
/// The pair is (distance of the cursor from the center of the line across it, distance along it),
/// so lines compare first by how well they are hit and then by how close they are.
fn distances_to_line(line: &OcrLine, cursor: (i32, i32)) -> Option<(i32, i32)> {
    let [left, top, right, bottom] = line.bbox;
    let (across_start, across_end, across, along_start, along_end, along) = if line.vertical {
        (left, right, cursor.0, top, bottom, cursor.1)
    } else {
        (top, bottom, cursor.1, left, right, cursor.0)
    };
    if across < across_start - LINE_MARGIN || across > across_end + LINE_MARGIN {
        return None;
    }
    Some((
        (across - (across_start + across_end) / 2).abs(),
        axis_distance(along_start, along_end, along),
    ))
}

/// The text of the block around `lines[line_index]` (its lines top to bottom, joined without
/// separators as Japanese has no spaces) and where the line itself starts in that text, in UTF-16
/// code units.
fn join_block(lines: &[OcrLine], line_index: usize) -> (String, usize) {
    let boxes: Vec<[i32; 4]> = lines.iter().map(|line| line.bbox).collect();
    let mut text = String::new();
    let mut offset_of_line = 0;
    for member in block_around(&boxes, line_index) {
        if member == line_index {
            offset_of_line = text.encode_utf16().count();
        }
        text.push_str(&lines[member].text);
    }
    (text, offset_of_line)
}

/// 0 when `value` is inside `[start, end)`, otherwise the distance to the nearest end.
fn axis_distance(start: i32, end: i32, value: i32) -> i32 {
    if value < start {
        start - value
    } else if value >= end {
        value - end + 1
    } else {
        0
    }
}

/// Whether the token is a word (not punctuation, a symbol or a space).
pub fn is_word(token: &Token) -> bool {
    !NON_WORD_POS.contains(&token.pos.as_str())
}

/// The word token that contains the UTF-16 `offset`; punctuation and symbols do not count.
pub fn word_at(tokens: &[Token], offset: usize) -> Option<&Token> {
    tokens
        .iter()
        .find(|token| token.start <= offset && offset < token.end)
        .filter(|token| is_word(token))
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
            vertical: false,
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
                vertical: false,
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

        let upper = character_at(&lines, (120, 948)).expect("hit");
        let lower = character_at(&lines, (120, 958)).expect("hit");

        // The lines are stacked, so both belong to one sentence; the hovered line decides where.
        assert_eq!(upper.line_bbox, lines[0].bbox);
        assert_eq!(upper.offset, 0);
        assert_eq!(lower.line_bbox, lines[1].bbox);
        assert_eq!(lower.offset, 3);
        assert_eq!(lower.text, "上の行下の行");
    }

    #[test]
    fn lines_side_by_side_are_told_apart_by_the_horizontal_position() {
        let lines = [line("左", 100, 900, 40, 50), line("右", 900, 900, 40, 50)];

        assert_eq!(character_at(&lines, (905, 920)).expect("hit").text, "右");
    }

    #[test]
    fn the_lines_of_a_two_line_subtitle_are_joined_into_one_sentence() {
        let lines = [
            line("昨日、友達と映画を", 400, 900, 40, 50),
            line("見に行きました。", 560, 970, 40, 50),
        ];

        let on_first = character_at(&lines, (420, 920)).expect("first line");
        let on_second = character_at(&lines, (580, 990)).expect("second line");

        assert_eq!(on_first.text, "昨日、友達と映画を見に行きました。");
        assert_eq!(on_second.text, "昨日、友達と映画を見に行きました。");
        assert_eq!(on_first.offset, 0);
        // 見 is the first character of the second line, after the nine characters of the first.
        assert_eq!(on_second.offset, 9);
        assert_eq!(on_first.line_bbox, lines[0].bbox);
        assert_eq!(on_second.line_bbox, lines[1].bbox);
    }

    #[test]
    fn the_offset_in_the_second_line_adds_the_length_of_the_first() {
        let lines = [
            line("猫が", 400, 900, 40, 50),
            line("好きです", 400, 970, 40, 50),
        ];

        // き is the second character of the second line.
        let hit = character_at(&lines, (400 + 40 + 5, 990)).expect("hit");

        assert_eq!(hit.text, "猫が好きです");
        assert_eq!(hit.offset, 3);
    }

    #[test]
    fn a_word_split_across_two_lines_can_be_found_from_either_half() {
        let lines = [
            line("映", 400, 900, 40, 50),
            line("画を見る", 400, 970, 40, 50),
        ];

        assert_eq!(
            character_at(&lines, (410, 920)).expect("first").text,
            "映画を見る"
        );
        assert_eq!(character_at(&lines, (410, 990)).expect("second").offset, 1);
    }

    #[test]
    fn the_offset_counts_utf16_units_of_the_lines_before_the_hovered_one() {
        let lines = [
            line("😀猫", 400, 900, 40, 50),
            line("が好き", 400, 970, 40, 50),
        ];

        let hit = character_at(&lines, (410, 990)).expect("hit");

        assert_eq!(hit.text, "😀猫が好き");
        // The emoji takes two UTF-16 units, so the second line starts at offset 3.
        assert_eq!(hit.offset, 3);
    }

    #[test]
    fn unrelated_lines_elsewhere_are_not_joined() {
        let lines = [
            line("猫が好き", 400, 900, 40, 50),
            line("別の場所", 100, 100, 40, 50),
        ];

        assert_eq!(
            character_at(&lines, (410, 920)).expect("hit").text,
            "猫が好き"
        );
    }

    #[test]
    fn a_paragraph_contributes_the_hovered_line_and_its_neighbours_only() {
        let lines: Vec<OcrLine> = (0..6)
            .map(|row| line(&format!("行{row}"), 300, 500 + row * 22, 16, 16))
            .collect();

        let hit = character_at(&lines, (310, 500 + 3 * 22 + 8)).expect("hit");

        assert_eq!(hit.text, "行2行3行4");
        // 行3 starts after the two characters of 行2.
        assert_eq!(hit.offset, 2);
    }

    /// A vertical column of `text` with `size`-pixel characters, its top-left corner at (x, y).
    fn column_of(text: &str, x: i32, y: i32, size: i32) -> OcrLine {
        let chars: Vec<OcrChar> = text
            .chars()
            .enumerate()
            .map(|(index, character)| OcrChar {
                character,
                bbox: [
                    x,
                    y + index as i32 * size,
                    x + size,
                    y + (index as i32 + 1) * size,
                ],
                confidence: 0.9,
            })
            .collect();
        OcrLine {
            text: text.to_string(),
            bbox: [x, y, x + size, y + chars.len() as i32 * size],
            chars,
            vertical: true,
        }
    }

    #[test]
    fn finds_the_character_under_the_cursor_in_a_vertical_column() {
        let lines = [column_of("猫が好き", 800, 200, 40)];

        let hit = character_at(&lines, (820, 285)).expect("hit");

        assert_eq!(
            hit,
            CharacterHit {
                text: "猫が好き".to_string(),
                offset: 2,
                char_bbox: [800, 280, 840, 320],
                line_bbox: [800, 200, 840, 360],
                vertical: true,
            }
        );
    }

    #[test]
    fn the_first_and_last_characters_of_a_column_are_reachable_and_the_ends_are_tolerant() {
        let lines = [column_of("猫が好き", 800, 200, 40)];

        assert_eq!(character_at(&lines, (810, 200)).expect("first").offset, 0);
        assert_eq!(character_at(&lines, (810, 359)).expect("last").offset, 3);
        assert_eq!(
            character_at(&lines, (810, 375))
                .expect("past the end")
                .offset,
            3
        );
        assert_eq!(
            character_at(&lines, (810, 185))
                .expect("before the start")
                .offset,
            0
        );
    }

    #[test]
    fn the_cursor_far_from_a_column_hits_nothing() {
        let lines = [column_of("猫が好き", 800, 200, 40)];

        assert_eq!(character_at(&lines, (810, 450)), None);
        assert_eq!(character_at(&lines, (700, 250)), None);
        assert_eq!(character_at(&lines, (900, 250)), None);
        assert!(character_at(&lines, (800 - LINE_MARGIN, 250)).is_some());
        assert_eq!(character_at(&lines, (800 - LINE_MARGIN - 1, 250)), None);
    }

    #[test]
    fn neighbouring_columns_are_joined_from_right_to_left() {
        let lines = [
            column_of("二列目です", 740, 220, 40),
            column_of("一列目から", 800, 200, 40),
        ];

        let on_first = character_at(&lines, (820, 215)).expect("first column");
        let on_second = character_at(&lines, (760, 235)).expect("second column");

        assert_eq!(on_first.text, "一列目から二列目です");
        assert_eq!(on_first.offset, 0);
        assert_eq!(on_second.text, "一列目から二列目です");
        assert_eq!(on_second.offset, 5);
        assert!(on_second.vertical);
    }

    #[test]
    fn a_horizontal_line_beside_a_column_is_not_joined_to_it() {
        let lines = [
            column_of("縦書き", 800, 200, 40),
            line("横書き", 100, 220, 40, 40),
        ];

        assert_eq!(
            character_at(&lines, (820, 215)).expect("column").text,
            "縦書き"
        );
        assert_eq!(
            character_at(&lines, (120, 240)).expect("line").text,
            "横書き"
        );
    }

    #[test]
    fn the_column_hit_by_the_cursor_wins_over_a_nearer_unrelated_one() {
        let lines = [
            column_of("近い", 800, 200, 40),
            column_of("遠い", 500, 200, 40),
        ];

        assert_eq!(character_at(&lines, (515, 215)).expect("hit").text, "遠い");
    }

    #[test]
    fn no_lines_or_lines_without_characters_hit_nothing() {
        assert_eq!(character_at(&[], (10, 10)), None);
        let empty = OcrLine {
            text: String::new(),
            bbox: [0, 0, 100, 40],
            chars: vec![],
            vertical: false,
        };
        assert_eq!(character_at(&[empty], (10, 10)), None);
    }

    #[test]
    fn only_words_are_words() {
        assert!(is_word(&token("猫", "名詞", 0, 1)));
        assert!(is_word(&token("が", "助詞", 1, 2)));
        assert!(!is_word(&token("。", "補助記号", 2, 3)));
        assert!(!is_word(&token("★", "記号", 3, 4)));
        assert!(!is_word(&token(" ", "空白", 4, 5)));
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
                "lineBbox": [0, 0, 40, 50],
                "vertical": false
            })
        );
    }
}
