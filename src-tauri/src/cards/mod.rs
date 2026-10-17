//! Turns a dictionary entry plus the sentence it came from into an Anki note.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::text_offsets::utf16_range_to_byte_range;
use crate::types::anki::{AnkiMedia, AnkiNote};
use crate::types::entry::Entry;
use crate::types::profile::{MiningField, Profile};

pub const DEFAULT_TAG: &str = "anki-mining";
/// Cards with very long sense lists (e.g. 行く has 12) stay readable.
pub const MAX_MEANINGS: usize = 8;

/// A file to attach to the card; `data` is base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub filename: String,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardInput {
    pub entry: Entry,
    pub sentence: String,
    /// UTF-16 offsets `[start, end)` of the mined word inside `sentence`, if known.
    pub highlight: Option<(usize, usize)>,
    /// Where the sentence came from (source id, window title, ...); goes to the `source` field.
    pub source: String,
    pub audio: Option<MediaFile>,
    pub picture: Option<MediaFile>,
}

/// Builds the note for `input` using the deck, note type and field names of `profile`.
pub fn build_note(profile: &Profile, input: &CardInput) -> AnkiNote {
    let mut fields = BTreeMap::new();
    for field in MiningField::ALL {
        let value = field_value(field, input);
        let name = profile
            .field_mapping
            .get(&field)
            .map(String::as_str)
            .unwrap_or_default();
        // Media fields are filled by AnkiConnect with the sound/image reference.
        if !name.is_empty() {
            fields.insert(name.to_string(), value);
        }
    }

    AnkiNote {
        deck_name: profile.deck_name.clone(),
        model_name: profile.model_name.clone(),
        fields,
        tags: vec![DEFAULT_TAG.to_string()],
        audio: media_list(input.audio.as_ref(), profile, MiningField::SentenceAudio),
        picture: media_list(input.picture.as_ref(), profile, MiningField::Picture),
    }
}

/// The pitch accents as `[1], [0]` (the number is where the pitch drops; `0` is flat).
fn pitch_label(positions: &[u8]) -> String {
    positions
        .iter()
        .map(|position| format!("[{position}]"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn field_value(field: MiningField, input: &CardInput) -> String {
    match field {
        MiningField::Expression => escape_html(&input.entry.expression),
        MiningField::Reading => escape_html(&input.entry.reading),
        MiningField::Meaning => format_meanings(&input.entry.meanings),
        MiningField::Sentence => highlight_sentence(&input.sentence, input.highlight),
        MiningField::Frequency => input
            .entry
            .frequency
            .map(|rank| rank.to_string())
            .unwrap_or_default(),
        MiningField::Source => escape_html(&input.source),
        MiningField::Pitch => pitch_label(&input.entry.pitch),
        MiningField::SentenceAudio | MiningField::Picture => String::new(),
    }
}

fn media_list(media: Option<&MediaFile>, profile: &Profile, field: MiningField) -> Vec<AnkiMedia> {
    let target = profile
        .field_mapping
        .get(&field)
        .filter(|name| !name.is_empty());
    match (media, target) {
        (Some(file), Some(name)) => vec![AnkiMedia {
            filename: file.filename.clone(),
            data: file.data.clone(),
            fields: vec![name.clone()],
        }],
        _ => Vec::new(),
    }
}

/// One sense as plain text, several as an ordered list (at most [`MAX_MEANINGS`]).
fn format_meanings(meanings: &[String]) -> String {
    match meanings {
        [] => String::new(),
        [single] => escape_html(single),
        many => {
            let items: String = many
                .iter()
                .take(MAX_MEANINGS)
                .map(|meaning| format!("<li>{}</li>", escape_html(meaning)))
                .collect();
            format!("<ol>{items}</ol>")
        }
    }
}

/// Escapes the sentence and wraps the highlighted range in `<b>`.
fn highlight_sentence(sentence: &str, highlight: Option<(usize, usize)>) -> String {
    let range = highlight.and_then(|(start, end)| utf16_range_to_byte_range(sentence, start, end));
    match range {
        Some(range) => format!(
            "{}<b>{}</b>{}",
            escape_html(&sentence[..range.start]),
            escape_html(&sentence[range.start..range.end]),
            escape_html(&sentence[range.end..]),
        ),
        None => escape_html(sentence),
    }
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_profile;

    fn entry() -> Entry {
        Entry {
            expression: "猫".to_string(),
            reading: "ねこ".to_string(),
            meanings: vec!["cat; feline".to_string(), "shamisen".to_string()],
            parts_of_speech: vec!["n".to_string()],
            frequency: Some(1520),
            pitch: Vec::new(),
        }
    }

    fn input() -> CardInput {
        CardInput {
            entry: entry(),
            sentence: "私は猫が好きです".to_string(),
            highlight: Some((2, 3)),
            source: "YouTube".to_string(),
            audio: None,
            picture: None,
        }
    }

    fn media(filename: &str, data: &str) -> MediaFile {
        MediaFile {
            filename: filename.to_string(),
            data: data.to_string(),
        }
    }

    #[test]
    fn builds_the_note_with_deck_model_tag_and_every_field() {
        let note = build_note(&default_profile(), &input());

        assert_eq!(
            note,
            AnkiNote {
                deck_name: "Mining".to_string(),
                model_name: "AnkiMining".to_string(),
                fields: BTreeMap::from([
                    ("Expression".to_string(), "猫".to_string()),
                    ("Reading".to_string(), "ねこ".to_string()),
                    (
                        "Meaning".to_string(),
                        "<ol><li>cat; feline</li><li>shamisen</li></ol>".to_string()
                    ),
                    (
                        "Sentence".to_string(),
                        "私は<b>猫</b>が好きです".to_string()
                    ),
                    ("SentenceAudio".to_string(), String::new()),
                    ("Picture".to_string(), String::new()),
                    ("Frequency".to_string(), "1520".to_string()),
                    ("Pitch".to_string(), String::new()),
                    ("Source".to_string(), "YouTube".to_string()),
                ]),
                tags: vec!["anki-mining".to_string()],
                audio: vec![],
                picture: vec![],
            }
        );
    }

    #[test]
    fn uses_the_deck_model_and_field_names_of_the_profile() {
        let mut profile = default_profile();
        profile.deck_name = "Games".to_string();
        profile.model_name = "Japanese Sentences".to_string();
        profile
            .field_mapping
            .insert(MiningField::Expression, "Word".to_string());
        profile
            .field_mapping
            .insert(MiningField::Meaning, "Definition".to_string());

        let note = build_note(&profile, &input());

        assert_eq!(note.deck_name, "Games");
        assert_eq!(note.model_name, "Japanese Sentences");
        assert_eq!(note.fields.get("Word"), Some(&"猫".to_string()));
        assert_eq!(
            note.fields.get("Definition"),
            Some(&"<ol><li>cat; feline</li><li>shamisen</li></ol>".to_string())
        );
        assert_eq!(note.fields.get("Expression"), None);
        assert_eq!(note.fields.get("Meaning"), None);
    }

    #[test]
    fn skips_fields_mapped_to_an_empty_name() {
        let mut profile = default_profile();
        profile
            .field_mapping
            .insert(MiningField::Pitch, String::new());
        profile
            .field_mapping
            .insert(MiningField::Frequency, String::new());

        let note = build_note(&profile, &input());

        assert_eq!(note.fields.len(), 7);
        assert_eq!(note.fields.get("Pitch"), None);
        assert_eq!(note.fields.get("Frequency"), None);
    }

    #[test]
    fn attaches_audio_and_picture_to_the_mapped_fields() {
        let mut profile = default_profile();
        profile
            .field_mapping
            .insert(MiningField::SentenceAudio, "Audio".to_string());
        let with_media = CardInput {
            audio: Some(media("clip.mp3", "AAEC")),
            picture: Some(media("shot.jpg", "AwQF")),
            ..input()
        };

        let note = build_note(&profile, &with_media);

        assert_eq!(
            note.audio,
            vec![AnkiMedia {
                filename: "clip.mp3".to_string(),
                data: "AAEC".to_string(),
                fields: vec!["Audio".to_string()],
            }]
        );
        assert_eq!(
            note.picture,
            vec![AnkiMedia {
                filename: "shot.jpg".to_string(),
                data: "AwQF".to_string(),
                fields: vec!["Picture".to_string()],
            }]
        );
        assert_eq!(note.fields.get("Audio"), Some(&String::new()));
    }

    #[test]
    fn drops_media_when_its_field_is_not_mapped() {
        let mut profile = default_profile();
        profile
            .field_mapping
            .insert(MiningField::SentenceAudio, String::new());
        profile
            .field_mapping
            .insert(MiningField::Picture, String::new());
        let with_media = CardInput {
            audio: Some(media("clip.mp3", "AAEC")),
            picture: Some(media("shot.jpg", "AwQF")),
            ..input()
        };

        let note = build_note(&profile, &with_media);

        assert_eq!(note.audio, vec![]);
        assert_eq!(note.picture, vec![]);
    }

    #[test]
    fn a_single_meaning_is_plain_text() {
        let single = CardInput {
            entry: Entry {
                meanings: vec!["cat".to_string()],
                ..entry()
            },
            ..input()
        };

        let note = build_note(&default_profile(), &single);

        assert_eq!(note.fields.get("Meaning"), Some(&"cat".to_string()));
    }

    #[test]
    fn no_meanings_leave_the_field_empty() {
        let none = CardInput {
            entry: Entry {
                meanings: vec![],
                ..entry()
            },
            ..input()
        };

        let note = build_note(&default_profile(), &none);

        assert_eq!(note.fields.get("Meaning"), Some(&String::new()));
    }

    #[test]
    fn caps_the_number_of_meanings() {
        let many = CardInput {
            entry: Entry {
                meanings: (1..=12).map(|number| format!("sense {number}")).collect(),
                ..entry()
            },
            ..input()
        };

        let note = build_note(&default_profile(), &many);

        let meaning = note.fields.get("Meaning").expect("meaning field");
        assert_eq!(meaning.matches("<li>").count(), MAX_MEANINGS);
        assert!(meaning.contains("sense 8"));
        assert!(!meaning.contains("sense 9"));
    }

    #[test]
    fn an_unknown_frequency_leaves_the_field_empty() {
        let unknown = CardInput {
            entry: Entry {
                frequency: None,
                pitch: Vec::new(),
                ..entry()
            },
            ..input()
        };

        let note = build_note(&default_profile(), &unknown);

        assert_eq!(note.fields.get("Frequency"), Some(&String::new()));
    }

    #[test]
    fn escapes_html_in_every_text_field() {
        let tricky = CardInput {
            entry: Entry {
                expression: "<i>猫</i>".to_string(),
                reading: "a&b".to_string(),
                meanings: vec!["\"quoted\" <b>".to_string()],
                ..entry()
            },
            sentence: "A&B <猫> です".to_string(),
            highlight: Some((5, 6)),
            source: "<script>".to_string(),
            ..input()
        };

        let note = build_note(&default_profile(), &tricky);

        assert_eq!(
            note.fields.get("Expression"),
            Some(&"&lt;i&gt;猫&lt;/i&gt;".to_string())
        );
        assert_eq!(note.fields.get("Reading"), Some(&"a&amp;b".to_string()));
        assert_eq!(
            note.fields.get("Meaning"),
            Some(&"&quot;quoted&quot; &lt;b&gt;".to_string())
        );
        assert_eq!(
            note.fields.get("Sentence"),
            Some(&"A&amp;B &lt;<b>猫</b>&gt; です".to_string())
        );
        assert_eq!(
            note.fields.get("Source"),
            Some(&"&lt;script&gt;".to_string())
        );
    }

    #[test]
    fn highlight_sentence_handles_missing_and_invalid_ranges() {
        assert_eq!(highlight_sentence("猫が好き", None), "猫が好き");
        assert_eq!(highlight_sentence("猫が好き", Some((2, 2))), "猫が好き");
        assert_eq!(highlight_sentence("猫が好き", Some((3, 2))), "猫が好き");
        assert_eq!(highlight_sentence("猫が好き", Some((0, 9))), "猫が好き");
        assert_eq!(highlight_sentence("猫が好き", Some((7, 9))), "猫が好き");
    }

    #[test]
    fn highlight_sentence_wraps_the_range_at_the_start_middle_and_end() {
        assert_eq!(
            highlight_sentence("猫が好き", Some((0, 1))),
            "<b>猫</b>が好き"
        );
        assert_eq!(
            highlight_sentence("猫が好き", Some((2, 4))),
            "猫が<b>好き</b>"
        );
        assert_eq!(
            highlight_sentence("猫が好き", Some((0, 4))),
            "<b>猫が好き</b>"
        );
    }

    #[test]
    fn card_input_uses_camel_case_json() {
        let json = serde_json::to_value(CardInput {
            audio: Some(media("clip.mp3", "AAEC")),
            ..input()
        })
        .expect("serialize");

        assert_eq!(json["sentence"], serde_json::json!("私は猫が好きです"));
        assert_eq!(json["highlight"], serde_json::json!([2, 3]));
        assert_eq!(json["picture"], serde_json::Value::Null);
        assert_eq!(
            json["audio"],
            serde_json::json!({ "filename": "clip.mp3", "data": "AAEC" })
        );
        assert_eq!(json["entry"]["partsOfSpeech"], serde_json::json!(["n"]));
    }

    #[test]
    fn the_pitch_field_lists_every_accent_of_the_word() {
        let with_pitch = CardInput {
            entry: Entry {
                pitch: vec![1, 0],
                ..entry()
            },
            ..input()
        };

        let note = build_note(&default_profile(), &with_pitch);

        assert_eq!(note.fields.get("Pitch"), Some(&"[1], [0]".to_string()));
    }

    #[test]
    fn the_pitch_field_is_empty_when_the_pitch_is_unknown() {
        let note = build_note(&default_profile(), &input());

        assert_eq!(note.fields.get("Pitch"), Some(&String::new()));
        assert_eq!(pitch_label(&[]), "");
        assert_eq!(pitch_label(&[3]), "[3]");
    }
}
