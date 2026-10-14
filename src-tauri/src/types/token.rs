use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub surface: String,
    pub lemma: String,
    pub reading: String,
    pub pos: String,
    /// UTF-16 code unit offset (inclusive), usable directly with `String.prototype.slice`.
    pub start: usize,
    /// UTF-16 code unit offset (exclusive).
    pub end: usize,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample_token() -> Token {
        Token {
            surface: "食べた".to_string(),
            lemma: "食べる".to_string(),
            reading: "タベタ".to_string(),
            pos: "動詞".to_string(),
            start: 3,
            end: 6,
        }
    }

    #[test]
    fn serializes_with_the_keys_expected_by_the_frontend() {
        assert_eq!(
            serde_json::to_value(sample_token()).expect("token must serialize"),
            json!({
                "surface": "食べた",
                "lemma": "食べる",
                "reading": "タベタ",
                "pos": "動詞",
                "start": 3,
                "end": 6
            })
        );
    }

    #[test]
    fn round_trips_through_json() {
        let json = serde_json::to_string(&sample_token()).expect("token must serialize");

        let parsed: Token = serde_json::from_str(&json).expect("token must parse");

        assert_eq!(parsed, sample_token());
    }

    #[test]
    fn rejects_negative_offsets() {
        let result = serde_json::from_value::<Token>(json!({
            "surface": "猫",
            "lemma": "猫",
            "reading": "ネコ",
            "pos": "名詞",
            "start": -1,
            "end": 1
        }));

        assert!(result.is_err());
    }
}
