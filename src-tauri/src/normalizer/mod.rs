//! Cleans up raw text from any source before it is tokenized.

use std::sync::OnceLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::types::text_source::TextSourceEvent;

/// Identical text arriving again within this window is dropped (OCR and subtitle repeats).
pub const DEFAULT_DEBOUNCE_MS: u64 = 1_000;

/// Furigana markup: the readings inside `<rt>`/`<rp>` are not part of the sentence.
fn ruby_annotation() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?is)<(rt|rp)\b[^>]*>.*?</(rt|rp)>").expect("valid regex"))
}

fn html_tag() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?s)<[^>]*>").expect("valid regex"))
}

/// ASS/SSA override blocks such as `{\an8}`.
fn ass_override() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\{\\[^}]*\}").expect("valid regex"))
}

/// Kanji followed by a kana reading in brackets, e.g. `漢字(かんじ)`.
fn bracketed_reading() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(\p{Han}+)[(（][\p{Hiragana}\p{Katakana}ー]+[)）]").expect("valid regex")
    })
}

fn line_break() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\s*[\r\n]+\s*").expect("valid regex"))
}

fn blank_run() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"[ \t\u{3000}]{2,}").expect("valid regex"))
}

/// Returns true for hiragana, katakana (including half-width), kanji and the iteration mark.
pub fn is_japanese_char(character: char) -> bool {
    matches!(
        character,
        '\u{3041}'..='\u{3096}'
            | '\u{30A1}'..='\u{30FA}'
            | '\u{30FD}'..='\u{30FF}'
            | '\u{31F0}'..='\u{31FF}'
            | '\u{FF66}'..='\u{FF9F}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{3005}'
    )
}

pub fn contains_japanese(text: &str) -> bool {
    text.chars().any(is_japanese_char)
}

/// Cleans one piece of text: markup and furigana removed, Unicode normalized (NFKC), line
/// breaks joined, whitespace tidied. Returns an empty string when nothing is left.
pub fn clean_text(text: &str) -> String {
    let without_ruby = ruby_annotation().replace_all(text, "");
    let without_tags = html_tag().replace_all(&without_ruby, "");
    let without_overrides = ass_override().replace_all(&without_tags, "");
    let with_breaks = without_overrides
        .replace("\\N", "\n")
        .replace("\\n", "\n")
        .replace("\\h", " ");
    let without_readings = bracketed_reading().replace_all(&with_breaks, "$1");
    let normalized: String = without_readings.nfkc().collect();

    let joined = join_lines(&normalized);
    blank_run().replace_all(&joined, " ").trim().to_string()
}

/// Line breaks inside Japanese text are removed; between other characters they become a space.
fn join_lines(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;

    for found in line_break().find_iter(text) {
        result.push_str(&text[last_end..found.start()]);
        let before = text[..found.start()].chars().next_back();
        let after = text[found.end()..].chars().next();
        let between_japanese = matches!((before, after), (Some(left), Some(right))
            if is_japanese_char(left) || is_japanese_char(right));
        if !between_japanese && before.is_some() && after.is_some() {
            result.push(' ');
        }
        last_end = found.end();
    }
    result.push_str(&text[last_end..]);
    result
}

/// Drops text identical to the previous one if it arrives within the window.
#[derive(Debug)]
pub struct Debouncer {
    window_ms: u64,
    last: Option<(String, u64)>,
}

impl Debouncer {
    pub fn new(window_ms: u64) -> Debouncer {
        Debouncer {
            window_ms,
            last: None,
        }
    }

    /// Returns false when `text` repeats the previous text inside the window.
    pub fn accept(&mut self, text: &str, now_ms: u64) -> bool {
        if let Some((last_text, last_ms)) = &self.last {
            if last_text == text && now_ms.saturating_sub(*last_ms) < self.window_ms {
                return false;
            }
        }
        self.last = Some((text.to_string(), now_ms));
        true
    }
}

/// Cleans events and drops the ones that are not worth tokenizing.
#[derive(Debug)]
pub struct Normalizer {
    debouncer: Debouncer,
}

impl Normalizer {
    pub fn new(debounce_ms: u64) -> Normalizer {
        Normalizer {
            debouncer: Debouncer::new(debounce_ms),
        }
    }

    /// Returns the cleaned event, or `None` if the text is empty, has no Japanese, or repeats
    /// the previous text within the debounce window (measured by event timestamps).
    pub fn process(&mut self, event: TextSourceEvent) -> Option<TextSourceEvent> {
        let text = clean_text(&event.text);
        if text.is_empty() || !contains_japanese(&text) {
            return None;
        }
        if !self.debouncer.accept(&text, event.timestamp) {
            return None;
        }
        Some(TextSourceEvent { text, ..event })
    }
}

impl Default for Normalizer {
    fn default() -> Normalizer {
        Normalizer::new(DEFAULT_DEBOUNCE_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::text_source::WindowInfo;

    fn event(text: &str, timestamp: u64) -> TextSourceEvent {
        TextSourceEvent {
            text: text.to_string(),
            source_id: "clipboard".to_string(),
            timestamp,
            window_info: Some(WindowInfo {
                title: "Player".to_string(),
                app_name: Some("mpv".to_string()),
            }),
        }
    }

    #[test]
    fn plain_japanese_text_is_left_untouched() {
        assert_eq!(clean_text("猫が好きです。"), "猫が好きです。");
    }

    #[test]
    fn removes_ruby_readings_and_keeps_the_base_text() {
        assert_eq!(
            clean_text("<ruby>漢字<rp>(</rp><rt>かんじ</rt><rp>)</rp></ruby>を読む"),
            "漢字を読む"
        );
    }

    #[test]
    fn removes_ruby_markup_across_lines_and_in_any_case() {
        assert_eq!(clean_text("<RUBY>日本<RT>に\\nほん</RT></RUBY>"), "日本");
        assert_eq!(clean_text("<ruby>日本<rt>に\nほん</rt></ruby>語"), "日本語");
    }

    #[test]
    fn strips_other_html_tags() {
        assert_eq!(clean_text("<i>こんにちは</i><br/>世界"), "こんにちは世界");
        assert_eq!(clean_text("<font color=\"#fff\">字幕</font>"), "字幕");
    }

    #[test]
    fn strips_ass_override_blocks_and_converts_ass_line_breaks() {
        assert_eq!(clean_text("{\\an8}上の字幕"), "上の字幕");
        assert_eq!(clean_text("{\\i1}一行目\\N二行目{\\i0}"), "一行目二行目");
        assert_eq!(clean_text("a\\hb"), "a b");
    }

    #[test]
    fn keeps_braces_that_are_not_ass_overrides() {
        assert_eq!(clean_text("{メモ}"), "{メモ}");
    }

    #[test]
    fn removes_bracketed_kana_readings_after_kanji() {
        assert_eq!(
            clean_text("漢字(かんじ)を勉強（べんきょう）する"),
            "漢字を勉強する"
        );
        assert_eq!(clean_text("日本語(ニホンゴ)"), "日本語");
    }

    #[test]
    fn keeps_brackets_that_do_not_hold_a_kana_reading() {
        assert_eq!(clean_text("猫(2匹)"), "猫(2匹)");
        assert_eq!(clean_text("はい(笑)"), "はい(笑)");
    }

    #[test]
    fn applies_nfkc_normalization() {
        assert_eq!(clean_text("ｶﾀｶﾅ"), "カタカナ");
        assert_eq!(clean_text("ＡＢＣ１２３"), "ABC123");
        assert_eq!(clean_text("２０２６年"), "2026年");
    }

    #[test]
    fn joins_line_breaks_inside_japanese_text() {
        assert_eq!(clean_text("今日は\n天気が\r\nいい"), "今日は天気がいい");
        assert_eq!(clean_text("今日は \n 天気"), "今日は天気");
    }

    #[test]
    fn turns_line_breaks_between_non_japanese_words_into_a_space() {
        assert_eq!(clean_text("hello\nworld"), "hello world");
    }

    #[test]
    fn keeps_a_line_break_next_to_japanese_text_joined_even_with_latin_on_one_side() {
        assert_eq!(clean_text("Wi-Fi\nが繋がらない"), "Wi-Fiが繋がらない");
    }

    #[test]
    fn collapses_repeated_blanks_and_trims() {
        assert_eq!(clean_text("  猫　　が   好き \t "), "猫 が 好き");
    }

    #[test]
    fn ignores_leading_and_trailing_line_breaks() {
        assert_eq!(clean_text("\n\n猫\n"), "猫");
    }

    #[test]
    fn returns_an_empty_string_when_only_markup_remains() {
        assert_eq!(clean_text("<i></i>{\\an8}  \n"), "");
        assert_eq!(clean_text(""), "");
    }

    #[test]
    fn recognizes_japanese_characters() {
        for character in [
            'あ', 'ゖ', 'ア', 'ヺ', 'ヽ', 'ㇰ', 'ｱ', '漢', '㐀', '々', '豈',
        ] {
            assert!(
                is_japanese_char(character),
                "{character} should be Japanese"
            );
        }
        for character in ['a', 'Z', '1', ' ', '。', '、', 'ー', '한'] {
            assert!(
                !is_japanese_char(character),
                "{character} should not be Japanese"
            );
        }
    }

    #[test]
    fn contains_japanese_checks_any_character() {
        assert!(contains_japanese("hello 世界"));
        assert!(contains_japanese("ｶﾀｶﾅ"));
        assert!(!contains_japanese("hello world 123"));
        assert!(!contains_japanese("。、！"));
        assert!(!contains_japanese(""));
    }

    #[test]
    fn debouncer_accepts_the_first_text_and_changes() {
        let mut debouncer = Debouncer::new(1_000);

        assert!(debouncer.accept("一", 0));
        assert!(debouncer.accept("二", 10));
        assert!(debouncer.accept("一", 20));
    }

    #[test]
    fn debouncer_rejects_the_same_text_inside_the_window() {
        let mut debouncer = Debouncer::new(1_000);

        assert!(debouncer.accept("一", 5_000));
        assert!(!debouncer.accept("一", 5_000));
        assert!(!debouncer.accept("一", 5_999));
    }

    #[test]
    fn debouncer_accepts_the_same_text_again_once_the_window_has_passed() {
        let mut debouncer = Debouncer::new(1_000);

        assert!(debouncer.accept("一", 5_000));
        assert!(debouncer.accept("一", 6_000));
        assert!(!debouncer.accept("一", 6_500));
    }

    #[test]
    fn debouncer_tolerates_timestamps_going_backwards() {
        let mut debouncer = Debouncer::new(1_000);

        assert!(debouncer.accept("一", 5_000));
        assert!(!debouncer.accept("一", 4_000));
    }

    #[test]
    fn process_returns_the_cleaned_event_and_keeps_the_other_fields() {
        let mut normalizer = Normalizer::default();

        let result = normalizer.process(event("<i>猫が\n好きです</i>", 42));

        assert_eq!(result, Some(event("猫が好きです", 42)));
    }

    #[test]
    fn process_drops_events_without_japanese_text() {
        let mut normalizer = Normalizer::default();

        assert_eq!(normalizer.process(event("Hello, world!", 0)), None);
        assert_eq!(normalizer.process(event("   ", 1)), None);
        assert_eq!(normalizer.process(event("<br/>", 2)), None);
    }

    #[test]
    fn process_drops_repeats_inside_the_window_but_not_after_it() {
        let mut normalizer = Normalizer::new(1_000);

        assert!(normalizer.process(event("猫", 10_000)).is_some());
        assert_eq!(normalizer.process(event("猫", 10_500)), None);
        assert_eq!(normalizer.process(event("<i>猫</i>", 10_900)), None);
        assert!(normalizer.process(event("猫", 11_000)).is_some());
    }

    #[test]
    fn process_does_not_start_the_window_for_dropped_events() {
        let mut normalizer = Normalizer::new(1_000);

        assert_eq!(normalizer.process(event("english only", 0)), None);
        assert!(normalizer.process(event("猫", 10)).is_some());
    }

    #[test]
    fn default_normalizer_uses_the_default_debounce_window() {
        let mut normalizer = Normalizer::default();

        assert!(normalizer.process(event("猫", 0)).is_some());
        assert_eq!(
            normalizer.process(event("猫", DEFAULT_DEBOUNCE_MS - 1)),
            None
        );
        assert!(normalizer
            .process(event("猫", DEFAULT_DEBOUNCE_MS))
            .is_some());
    }
}
