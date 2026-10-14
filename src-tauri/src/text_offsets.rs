//! Conversions between UTF-16 code unit offsets (used by the frontend) and Rust byte offsets.

use std::ops::Range;

/// Byte offset of the character starting at UTF-16 `offset`; `text.len()` for the end of the
/// text. `None` if the offset is past the end or falls inside a surrogate pair.
pub fn utf16_offset_to_byte(text: &str, offset: usize) -> Option<usize> {
    let mut utf16_offset = 0;
    for (byte_index, character) in text.char_indices() {
        if utf16_offset == offset {
            return Some(byte_index);
        }
        utf16_offset += character.len_utf16();
    }
    (utf16_offset == offset).then_some(text.len())
}

/// Converts a UTF-16 `[start, end)` range to byte offsets; `None` if it is empty, out of bounds,
/// or cuts a character in half.
pub fn utf16_range_to_byte_range(text: &str, start: usize, end: usize) -> Option<Range<usize>> {
    if start >= end {
        return None;
    }
    Some(utf16_offset_to_byte(text, start)?..utf16_offset_to_byte(text, end)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_map_to_the_byte_index_of_each_character() {
        assert_eq!(utf16_offset_to_byte("猫が", 0), Some(0));
        assert_eq!(utf16_offset_to_byte("猫が", 1), Some(3));
        assert_eq!(utf16_offset_to_byte("猫が", 2), Some(6));
    }

    #[test]
    fn the_end_of_the_text_maps_to_its_length() {
        assert_eq!(utf16_offset_to_byte("", 0), Some(0));
        assert_eq!(utf16_offset_to_byte("abc", 3), Some(3));
    }

    #[test]
    fn offsets_past_the_end_are_rejected() {
        assert_eq!(utf16_offset_to_byte("猫", 2), None);
        assert_eq!(utf16_offset_to_byte("", 1), None);
    }

    #[test]
    fn offsets_account_for_surrogate_pairs_and_reject_the_middle_of_one() {
        // 😀 takes two UTF-16 units but four bytes.
        assert_eq!(utf16_offset_to_byte("😀猫", 0), Some(0));
        assert_eq!(utf16_offset_to_byte("😀猫", 1), None);
        assert_eq!(utf16_offset_to_byte("😀猫", 2), Some(4));
        assert_eq!(utf16_offset_to_byte("😀猫", 3), Some(7));
    }

    #[test]
    fn ranges_account_for_surrogate_pairs() {
        assert_eq!(utf16_range_to_byte_range("😀猫", 2, 3), Some(4..7));
        assert_eq!(utf16_range_to_byte_range("😀猫", 0, 2), Some(0..4));
        assert_eq!(utf16_range_to_byte_range("😀猫", 0, 3), Some(0..7));
    }

    #[test]
    fn ranges_cutting_a_character_in_half_are_rejected() {
        assert_eq!(utf16_range_to_byte_range("😀猫", 1, 3), None);
        assert_eq!(utf16_range_to_byte_range("😀猫", 0, 1), None);
    }

    #[test]
    fn ranges_outside_the_text_or_empty_are_rejected() {
        assert_eq!(utf16_range_to_byte_range("猫", 0, 2), None);
        assert_eq!(utf16_range_to_byte_range("猫", 2, 3), None);
        assert_eq!(utf16_range_to_byte_range("猫", 1, 1), None);
        assert_eq!(utf16_range_to_byte_range("猫", 1, 0), None);
        assert_eq!(utf16_range_to_byte_range("", 0, 1), None);
    }
}
