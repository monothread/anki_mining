use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera_analysis::tokenizer::Tokenizer as LinderaTokenizer;
use thiserror::Error;

use crate::types::token::Token;

const EMBEDDED_DICTIONARY: &str = "embedded://unidic";

/// UniDic feature columns used to build a `Token` (see the UniDic-cwj feature layout).
const POS_COLUMN: usize = 0;
const LEMMA_READING_COLUMN: usize = 6;
const LEMMA_ORTHOGRAPHY_COLUMN: usize = 10;
const MINIMUM_KNOWN_WORD_COLUMNS: usize = LEMMA_ORTHOGRAPHY_COLUMN + 1;
const UNKNOWN_FEATURE: &str = "*";
const UNKNOWN_POS: &str = "未知語";

#[derive(Debug, Error)]
pub enum TokenizerError {
    #[error("failed to load the embedded dictionary: {0}")]
    Dictionary(String),
    #[error("failed to tokenize the text: {0}")]
    Tokenize(String),
}

/// Japanese morphological tokenizer backed by Lindera and the embedded UniDic dictionary.
pub struct Tokenizer {
    inner: LinderaTokenizer,
}

impl Tokenizer {
    pub fn new() -> Result<Tokenizer, TokenizerError> {
        let dictionary = load_dictionary(EMBEDDED_DICTIONARY)
            .map_err(|error| TokenizerError::Dictionary(error.to_string()))?;
        let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
        Ok(Tokenizer {
            inner: LinderaTokenizer::new(segmenter),
        })
    }

    /// Splits `text` into tokens.
    ///
    /// - `lemma` is the dictionary form as written in the text (UniDic `orthBase`), e.g. `しまう`.
    /// - `reading` is the katakana reading of the lemma, e.g. `タベル`.
    /// - `pos` is the top-level part of speech, e.g. `動詞`.
    /// - `start`/`end` are UTF-16 code unit offsets into `text`.
    ///
    /// Unknown words get their surface as lemma, an empty reading and the `未知語` part of speech.
    pub fn tokenize(&self, text: &str) -> Result<Vec<Token>, TokenizerError> {
        let mut raw_tokens = self
            .inner
            .tokenize(text)
            .map_err(|error| TokenizerError::Tokenize(error.to_string()))?;
        let utf16_offsets = utf16_offsets_by_byte(text);

        let mut tokens = Vec::with_capacity(raw_tokens.len());
        for raw in raw_tokens.iter_mut() {
            let surface = raw.surface.to_string();
            let start = utf16_offsets[raw.byte_start];
            let end = utf16_offsets[raw.byte_end];
            let details: Vec<String> = raw.details().iter().map(ToString::to_string).collect();
            tokens.push(build_token(surface, &details, start, end));
        }
        Ok(tokens)
    }
}

fn build_token(surface: String, details: &[String], start: usize, end: usize) -> Token {
    if details.len() < MINIMUM_KNOWN_WORD_COLUMNS {
        return Token {
            lemma: surface.clone(),
            surface,
            reading: String::new(),
            pos: UNKNOWN_POS.to_string(),
            start,
            end,
        };
    }

    Token {
        lemma: feature_or(&details[LEMMA_ORTHOGRAPHY_COLUMN], &surface),
        reading: feature_or(&details[LEMMA_READING_COLUMN], ""),
        pos: feature_or(&details[POS_COLUMN], UNKNOWN_POS),
        surface,
        start,
        end,
    }
}

fn feature_or(feature: &str, fallback: &str) -> String {
    if feature == UNKNOWN_FEATURE || feature.is_empty() {
        fallback.to_string()
    } else {
        feature.to_string()
    }
}

/// For each byte index of `text` (including `text.len()`), the matching UTF-16 code unit offset.
/// Entries for indices in the middle of a multi-byte character repeat the offset of that character.
fn utf16_offsets_by_byte(text: &str) -> Vec<usize> {
    let mut offsets = vec![0; text.len() + 1];
    let mut utf16_offset = 0;

    for (byte_index, character) in text.char_indices() {
        let character_length = character.len_utf8();
        for inner_index in 0..character_length {
            offsets[byte_index + inner_index] = utf16_offset;
        }
        utf16_offset += character.len_utf16();
    }
    offsets[text.len()] = utf16_offset;
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenizer() -> Tokenizer {
        Tokenizer::new().expect("the embedded dictionary must load")
    }

    fn token(
        surface: &str,
        lemma: &str,
        reading: &str,
        pos: &str,
        start: usize,
        end: usize,
    ) -> Token {
        Token {
            surface: surface.to_string(),
            lemma: lemma.to_string(),
            reading: reading.to_string(),
            pos: pos.to_string(),
            start,
            end,
        }
    }

    fn details(columns: &[&str]) -> Vec<String> {
        columns.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn tokenizes_a_simple_sentence_with_lemma_reading_pos_and_offsets() {
        assert_eq!(
            tokenizer().tokenize("猫が好きです").expect("tokenize"),
            vec![
                token("猫", "猫", "ネコ", "名詞", 0, 1),
                token("が", "が", "ガ", "助詞", 1, 2),
                token("好き", "好き", "スキ", "形状詞", 2, 4),
                token("です", "です", "デス", "助動詞", 4, 6),
            ]
        );
    }

    #[test]
    fn returns_dictionary_forms_for_conjugated_verbs() {
        assert_eq!(
            tokenizer()
                .tokenize("昨日、友達と映画を見に行きました。")
                .expect("tokenize"),
            vec![
                token("昨日", "昨日", "キノウ", "名詞", 0, 2),
                token("、", "、", "", "補助記号", 2, 3),
                token("友達", "友達", "トモダチ", "名詞", 3, 5),
                token("と", "と", "ト", "助詞", 5, 6),
                token("映画", "映画", "エイガ", "名詞", 6, 8),
                token("を", "を", "ヲ", "助詞", 8, 9),
                token("見", "見る", "ミル", "動詞", 9, 10),
                token("に", "に", "ニ", "助詞", 10, 11),
                token("行き", "行く", "イク", "動詞", 11, 13),
                token("まし", "ます", "マス", "助動詞", 13, 15),
                token("た", "た", "タ", "助動詞", 15, 16),
                token("。", "。", "", "補助記号", 16, 17),
            ]
        );
    }

    #[test]
    fn splits_stacked_auxiliaries_into_separate_tokens() {
        assert_eq!(
            tokenizer()
                .tokenize("食べさせられたくなかった")
                .expect("tokenize"),
            vec![
                token("食べ", "食べる", "タベル", "動詞", 0, 2),
                token("させ", "させる", "サセル", "助動詞", 2, 4),
                token("られ", "られる", "ラレル", "助動詞", 4, 6),
                token("たく", "たい", "タイ", "助動詞", 6, 8),
                token("なかっ", "ない", "ナイ", "形容詞", 8, 11),
                token("た", "た", "タ", "助動詞", 11, 12),
            ]
        );
    }

    #[test]
    fn uses_the_written_form_of_the_lemma_for_kana_spelled_words() {
        let tokens = tokenizer().tokenize("買ってしまった").expect("tokenize");

        assert_eq!(
            tokens,
            vec![
                token("買っ", "買う", "カウ", "動詞", 0, 2),
                token("て", "て", "テ", "助詞", 2, 3),
                token("しまっ", "しまう", "シマウ", "動詞", 3, 6),
                token("た", "た", "タ", "助動詞", 6, 7),
            ]
        );
    }

    #[test]
    fn offsets_count_utf16_code_units_so_emoji_take_two() {
        assert_eq!(
            tokenizer().tokenize("😀猫").expect("tokenize"),
            vec![
                token("😀", "😀", "", "補助記号", 0, 2),
                token("猫", "猫", "ネコ", "名詞", 2, 3),
            ]
        );
    }

    #[test]
    fn offsets_skip_whitespace_that_produces_no_token() {
        assert_eq!(
            tokenizer().tokenize("ＡＢＣ abc").expect("tokenize"),
            vec![
                token("ＡＢＣ", "ＡＢＣ", "エービーシー", "名詞", 0, 3),
                token("a", "a", "エー", "記号", 4, 5),
                token("b", "b", "ビー", "記号", 5, 6),
                token("c", "c", "シー", "記号", 6, 7),
            ]
        );
    }

    #[test]
    fn offsets_can_slice_the_original_text() {
        let text = "😀猫が好きです";
        let utf16: Vec<u16> = text.encode_utf16().collect();

        for token in tokenizer().tokenize(text).expect("tokenize") {
            let slice = String::from_utf16(&utf16[token.start..token.end]).expect("valid utf16");
            assert_eq!(slice, token.surface);
        }
    }

    #[test]
    fn returns_no_tokens_for_empty_or_blank_text() {
        assert_eq!(tokenizer().tokenize("").expect("tokenize"), vec![]);
        assert_eq!(tokenizer().tokenize("   ").expect("tokenize"), vec![]);
    }

    #[test]
    fn build_token_falls_back_to_the_surface_for_unknown_words() {
        assert_eq!(
            build_token("ﾎﾞﾝ".to_string(), &details(&["UNK"]), 3, 5),
            token("ﾎﾞﾝ", "ﾎﾞﾝ", "", "未知語", 3, 5)
        );
    }

    #[test]
    fn build_token_replaces_wildcard_features_with_fallbacks() {
        let columns = details(&["名詞", "*", "*", "*", "*", "*", "*", "*", "*", "*", "*"]);

        assert_eq!(
            build_token("謎".to_string(), &columns, 0, 1),
            token("謎", "謎", "", "名詞", 0, 1)
        );
    }

    #[test]
    fn build_token_reads_pos_reading_and_lemma_from_the_unidic_columns() {
        let columns = details(&[
            "動詞",
            "一般",
            "*",
            "*",
            "五段-カ行",
            "連用形-一般",
            "イク",
            "行く",
            "行き",
            "イキ",
            "行く",
        ]);

        assert_eq!(
            build_token("行き".to_string(), &columns, 11, 13),
            token("行き", "行く", "イク", "動詞", 11, 13)
        );
    }

    #[test]
    fn feature_or_uses_the_fallback_for_wildcards_and_empty_values_only() {
        assert_eq!(feature_or("*", "x"), "x");
        assert_eq!(feature_or("", "x"), "x");
        assert_eq!(feature_or("値", "x"), "値");
    }

    #[test]
    fn utf16_offsets_map_every_byte_of_multibyte_characters() {
        assert_eq!(utf16_offsets_by_byte(""), vec![0]);
        assert_eq!(utf16_offsets_by_byte("a猫"), vec![0, 1, 1, 1, 2]);
        assert_eq!(utf16_offsets_by_byte("😀a"), vec![0, 0, 0, 0, 2, 3]);
    }

    #[test]
    fn tokenizer_errors_have_descriptive_messages() {
        assert_eq!(
            TokenizerError::Dictionary("missing".to_string()).to_string(),
            "failed to load the embedded dictionary: missing"
        );
        assert_eq!(
            TokenizerError::Tokenize("boom".to_string()).to_string(),
            "failed to tokenize the text: boom"
        );
    }
}
