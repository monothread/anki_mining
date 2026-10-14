use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MiningField {
    Expression,
    Reading,
    Meaning,
    Sentence,
    SentenceAudio,
    Picture,
    Frequency,
    Pitch,
    Source,
}

impl MiningField {
    pub const ALL: [MiningField; 9] = [
        MiningField::Expression,
        MiningField::Reading,
        MiningField::Meaning,
        MiningField::Sentence,
        MiningField::SentenceAudio,
        MiningField::Picture,
        MiningField::Frequency,
        MiningField::Pitch,
        MiningField::Source,
    ];

    /// Field name used by the default "AnkiMining" note type.
    pub fn default_anki_name(self) -> &'static str {
        match self {
            MiningField::Expression => "Expression",
            MiningField::Reading => "Reading",
            MiningField::Meaning => "Meaning",
            MiningField::Sentence => "Sentence",
            MiningField::SentenceAudio => "SentenceAudio",
            MiningField::Picture => "Picture",
            MiningField::Frequency => "Frequency",
            MiningField::Pitch => "Pitch",
            MiningField::Source => "Source",
        }
    }
}

/// Maps each mining field to the name of the Anki note type field that receives it.
pub type FieldMapping = BTreeMap<MiningField, String>;

pub fn default_field_mapping() -> FieldMapping {
    MiningField::ALL
        .into_iter()
        .map(|field| (field, field.default_anki_name().to_string()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMatch {
    pub source_id: Option<String>,
    pub app_name: Option<String>,
    /// Part of the window title (any case), e.g. `YouTube` to tell sites apart in a browser.
    #[serde(default)]
    pub window_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub deck_name: String,
    pub model_name: String,
    pub field_mapping: FieldMapping,
    pub r#match: ProfileMatch,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn default_mapping_json() -> serde_json::Value {
        json!({
            "expression": "Expression",
            "reading": "Reading",
            "meaning": "Meaning",
            "sentence": "Sentence",
            "sentenceAudio": "SentenceAudio",
            "picture": "Picture",
            "frequency": "Frequency",
            "pitch": "Pitch",
            "source": "Source"
        })
    }

    fn sample_profile() -> Profile {
        Profile {
            id: "youtube".to_string(),
            name: "YouTube".to_string(),
            deck_name: "Youtube".to_string(),
            model_name: "AnkiMining".to_string(),
            field_mapping: default_field_mapping(),
            r#match: ProfileMatch {
                source_id: Some("browser-extension".to_string()),
                app_name: None,
                window_title: None,
            },
        }
    }

    #[test]
    fn mining_fields_serialize_in_camel_case_in_declaration_order() {
        let names: Vec<String> = MiningField::ALL
            .into_iter()
            .map(|field| {
                serde_json::to_value(field)
                    .expect("field must serialize")
                    .as_str()
                    .expect("field must serialize to a string")
                    .to_string()
            })
            .collect();

        assert_eq!(
            names,
            vec![
                "expression",
                "reading",
                "meaning",
                "sentence",
                "sentenceAudio",
                "picture",
                "frequency",
                "pitch",
                "source"
            ]
        );
    }

    #[test]
    fn default_field_mapping_covers_every_field_with_the_default_note_type_names() {
        let mapping = default_field_mapping();

        assert_eq!(mapping.len(), MiningField::ALL.len());
        assert_eq!(
            serde_json::to_value(&mapping).expect("mapping must serialize"),
            default_mapping_json()
        );
    }

    #[test]
    fn profile_serializes_with_the_keys_expected_by_the_frontend() {
        assert_eq!(
            serde_json::to_value(sample_profile()).expect("profile must serialize"),
            json!({
                "id": "youtube",
                "name": "YouTube",
                "deckName": "Youtube",
                "modelName": "AnkiMining",
                "fieldMapping": default_mapping_json(),
                "match": { "sourceId": "browser-extension", "appName": null, "windowTitle": null }
            })
        );
    }

    #[test]
    fn profile_round_trips_through_json() {
        let json = serde_json::to_string(&sample_profile()).expect("profile must serialize");

        let parsed: Profile = serde_json::from_str(&json).expect("profile must parse");

        assert_eq!(parsed, sample_profile());
    }

    #[test]
    fn rejects_an_unknown_mining_field_in_the_mapping() {
        let result =
            serde_json::from_value::<FieldMapping>(json!({ "translation": "Translation" }));

        assert!(result.is_err());
    }
}
