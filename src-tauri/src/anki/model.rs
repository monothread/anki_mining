use serde::Serialize;

use crate::config::DEFAULT_MODEL_NAME;
use crate::types::profile::MiningField;

const CSS: &str = "\
.card { font-family: 'Noto Sans CJK JP', sans-serif; font-size: 20px; text-align: center; }
.expression { font-size: 40px; margin-bottom: 12px; }
.sentence { font-size: 24px; margin: 12px 0; }
.reading { font-size: 22px; color: #777; }
.meaning { margin: 12px 0; }
.meta { font-size: 14px; color: #999; }
img { max-width: 100%; }
b { color: #d33; }
";

const FRONT_TEMPLATE: &str = "\
<div class=\"expression\">{{Expression}}</div>
<div class=\"sentence\">{{Sentence}}</div>
{{SentenceAudio}}";

const BACK_TEMPLATE: &str = "\
{{FrontSide}}
<hr id=\"answer\">
<div class=\"reading\">{{Reading}}</div>
<div class=\"meaning\">{{Meaning}}</div>
{{Picture}}
<div class=\"meta\">{{Frequency}} {{Pitch}} {{Source}}</div>";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardTemplate {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Front")]
    pub front: String,
    #[serde(rename = "Back")]
    pub back: String,
}

/// Parameters of AnkiConnect's `createModel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModel {
    pub model_name: String,
    pub in_order_fields: Vec<String>,
    pub css: String,
    pub is_cloze: bool,
    pub card_templates: Vec<CardTemplate>,
}

/// The note type the app creates when the user does not have one: one field per mining field.
pub fn default_mining_model() -> CreateModel {
    CreateModel {
        model_name: DEFAULT_MODEL_NAME.to_string(),
        in_order_fields: MiningField::ALL
            .into_iter()
            .map(|field| field.default_anki_name().to_string())
            .collect(),
        css: CSS.to_string(),
        is_cloze: false,
        card_templates: vec![CardTemplate {
            name: "Recognition".to_string(),
            front: FRONT_TEMPLATE.to_string(),
            back: BACK_TEMPLATE.to_string(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_default_model_has_one_field_per_mining_field_in_order() {
        let model = default_mining_model();

        assert_eq!(model.model_name, "AnkiMining");
        assert_eq!(
            model.in_order_fields,
            vec![
                "Expression",
                "Reading",
                "Meaning",
                "Sentence",
                "SentenceAudio",
                "Picture",
                "Frequency",
                "Pitch",
                "Source"
            ]
        );
        assert!(!model.is_cloze);
    }

    #[test]
    fn the_default_model_has_a_single_recognition_template_using_its_fields() {
        let model = default_mining_model();

        assert_eq!(model.card_templates.len(), 1);
        let template = &model.card_templates[0];
        assert_eq!(template.name, "Recognition");
        for field in ["Expression", "Sentence", "SentenceAudio"] {
            assert!(
                template.front.contains(&format!("{{{{{field}}}}}")),
                "front lacks {field}"
            );
        }
        for field in [
            "Reading",
            "Meaning",
            "Picture",
            "Frequency",
            "Pitch",
            "Source",
        ] {
            assert!(
                template.back.contains(&format!("{{{{{field}}}}}")),
                "back lacks {field}"
            );
        }
        assert!(template.back.contains("{{FrontSide}}"));
    }

    #[test]
    fn serializes_with_the_key_names_expected_by_ankiconnect() {
        let value = serde_json::to_value(default_mining_model()).expect("serialize");

        assert_eq!(value["modelName"], json!("AnkiMining"));
        assert_eq!(value["isCloze"], json!(false));
        assert_eq!(value["inOrderFields"].as_array().map(Vec::len), Some(9));
        assert_eq!(value["css"], json!(CSS));
        assert_eq!(
            value["cardTemplates"],
            json!([{
                "Name": "Recognition",
                "Front": FRONT_TEMPLATE,
                "Back": BACK_TEMPLATE
            }])
        );
    }
}
