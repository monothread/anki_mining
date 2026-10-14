use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnkiMedia {
    pub filename: String,
    /// Base64-encoded file contents.
    pub data: String,
    /// Note fields that receive the media reference.
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnkiNote {
    pub deck_name: String,
    pub model_name: String,
    pub fields: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub audio: Vec<AnkiMedia>,
    pub picture: Vec<AnkiMedia>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample_note() -> AnkiNote {
        AnkiNote {
            deck_name: "Youtube".to_string(),
            model_name: "AnkiMining".to_string(),
            fields: BTreeMap::from([
                ("Expression".to_string(), "猫".to_string()),
                ("Sentence".to_string(), "猫が好きです".to_string()),
            ]),
            tags: vec!["anki-mining".to_string()],
            audio: vec![AnkiMedia {
                filename: "clip.mp3".to_string(),
                data: "AAEC".to_string(),
                fields: vec!["SentenceAudio".to_string()],
            }],
            picture: vec![],
        }
    }

    #[test]
    fn serializes_with_the_keys_expected_by_the_anki_client() {
        assert_eq!(
            serde_json::to_value(sample_note()).expect("note must serialize"),
            json!({
                "deckName": "Youtube",
                "modelName": "AnkiMining",
                "fields": { "Expression": "猫", "Sentence": "猫が好きです" },
                "tags": ["anki-mining"],
                "audio": [
                    { "filename": "clip.mp3", "data": "AAEC", "fields": ["SentenceAudio"] }
                ],
                "picture": []
            })
        );
    }

    #[test]
    fn round_trips_through_json() {
        let json = serde_json::to_string(&sample_note()).expect("note must serialize");

        let parsed: AnkiNote = serde_json::from_str(&json).expect("note must parse");

        assert_eq!(parsed, sample_note());
    }

    #[test]
    fn rejects_media_without_a_filename() {
        let result = serde_json::from_value::<AnkiMedia>(json!({
            "data": "AAEC",
            "fields": ["Picture"]
        }));

        assert!(result.is_err());
    }
}
