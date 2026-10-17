use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub expression: String,
    pub reading: String,
    pub meanings: Vec<String>,
    pub parts_of_speech: Vec<String>,
    /// Frequency rank (lower is more common); `None` when unknown.
    pub frequency: Option<u32>,
    /// Downstep positions of the pitch accent (`0` is flat, `1` is a drop after the first mora);
    /// empty when unknown. A word can have several accents.
    #[serde(default)]
    pub pitch: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample_entry() -> Entry {
        Entry {
            expression: "猫".to_string(),
            reading: "ねこ".to_string(),
            meanings: vec!["cat".to_string(), "feline".to_string()],
            parts_of_speech: vec!["noun".to_string()],
            frequency: Some(1520),
            pitch: vec![1, 0],
        }
    }

    #[test]
    fn serializes_with_the_keys_expected_by_the_frontend() {
        assert_eq!(
            serde_json::to_value(sample_entry()).expect("entry must serialize"),
            json!({
                "expression": "猫",
                "reading": "ねこ",
                "meanings": ["cat", "feline"],
                "partsOfSpeech": ["noun"],
                "frequency": 1520,
                "pitch": [1, 0]
            })
        );
    }

    #[test]
    fn serializes_an_unknown_frequency_as_null() {
        let entry = Entry {
            frequency: None,
            pitch: Vec::new(),
            ..sample_entry()
        };

        let value = serde_json::to_value(entry).expect("entry must serialize");

        assert_eq!(value["frequency"], serde_json::Value::Null);
    }

    #[test]
    fn a_payload_without_pitch_has_no_pitch() {
        let entry: Entry = serde_json::from_value(json!({
            "expression": "猫",
            "reading": "ねこ",
            "meanings": ["cat"],
            "partsOfSpeech": ["n"],
            "frequency": null
        }))
        .expect("entry must parse");

        assert_eq!(entry.pitch, Vec::<u8>::new());
    }

    #[test]
    fn round_trips_through_json() {
        let json = serde_json::to_string(&sample_entry()).expect("entry must serialize");

        let parsed: Entry = serde_json::from_str(&json).expect("entry must parse");

        assert_eq!(parsed, sample_entry());
    }

    #[test]
    fn rejects_a_payload_without_meanings() {
        let result = serde_json::from_value::<Entry>(json!({
            "expression": "猫",
            "reading": "ねこ",
            "partsOfSpeech": [],
            "frequency": null
        }));

        assert!(result.is_err());
    }
}
