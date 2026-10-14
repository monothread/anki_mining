use std::sync::mpsc::Receiver;
use std::sync::Arc;

use serde::Serialize;

use crate::normalizer::Normalizer;
use crate::tokenizer::{Tokenizer, TokenizerError};
use crate::types::text_source::{TextSourceEvent, WindowInfo};
use crate::types::token::Token;

/// A cleaned and tokenized sentence, as sent to the overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentencePayload {
    pub text: String,
    pub tokens: Vec<Token>,
    pub source_id: String,
    pub window_info: Option<WindowInfo>,
    pub timestamp: u64,
}

/// Cleans and tokenizes one event; `Ok(None)` when the normalizer drops it.
pub fn analyze(
    normalizer: &mut Normalizer,
    tokenizer: &Tokenizer,
    event: TextSourceEvent,
) -> Result<Option<SentencePayload>, TokenizerError> {
    let Some(event) = normalizer.process(event) else {
        return Ok(None);
    };
    let tokens = tokenizer.tokenize(&event.text)?;
    Ok(Some(SentencePayload {
        text: event.text,
        tokens,
        source_id: event.source_id,
        window_info: event.window_info,
        timestamp: event.timestamp,
    }))
}

/// Processes events until every sender is gone. Tokenizer failures are reported to `on_error`
/// and do not stop the pipeline.
pub fn run(
    events: &Receiver<TextSourceEvent>,
    mut normalizer: Normalizer,
    tokenizer: &Arc<Tokenizer>,
    mut on_sentence: impl FnMut(SentencePayload),
    mut on_error: impl FnMut(String),
) {
    for event in events {
        match analyze(&mut normalizer, tokenizer, event) {
            Ok(Some(sentence)) => on_sentence(sentence),
            Ok(None) => {}
            Err(error) => on_error(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn tokenizer() -> Arc<Tokenizer> {
        Arc::new(Tokenizer::new().expect("tokenizer"))
    }

    fn event(text: &str, timestamp: u64) -> TextSourceEvent {
        TextSourceEvent {
            text: text.to_string(),
            source_id: "clipboard".to_string(),
            timestamp,
            window_info: Some(WindowInfo {
                title: "Player".to_string(),
                app_name: None,
            }),
        }
    }

    #[test]
    fn analyze_cleans_the_text_and_tokenizes_it_keeping_the_event_metadata() {
        let tokenizer = tokenizer();
        let mut normalizer = Normalizer::default();

        let sentence = analyze(&mut normalizer, &tokenizer, event("<i>猫が好きです</i>", 7))
            .expect("analyze")
            .expect("sentence");

        assert_eq!(sentence.text, "猫が好きです");
        assert_eq!(sentence.source_id, "clipboard");
        assert_eq!(sentence.timestamp, 7);
        assert_eq!(
            sentence.window_info,
            Some(WindowInfo {
                title: "Player".to_string(),
                app_name: None
            })
        );
        let surfaces: Vec<&str> = sentence
            .tokens
            .iter()
            .map(|token| token.surface.as_str())
            .collect();
        assert_eq!(surfaces, vec!["猫", "が", "好き", "です"]);
        assert_eq!(sentence.tokens[2].start, 2);
        assert_eq!(sentence.tokens[2].end, 4);
    }

    #[test]
    fn analyze_returns_none_when_the_normalizer_drops_the_event() {
        let tokenizer = tokenizer();
        let mut normalizer = Normalizer::default();

        assert_eq!(
            analyze(&mut normalizer, &tokenizer, event("English only", 0)).expect("analyze"),
            None
        );
    }

    #[test]
    fn run_forwards_sentences_in_order_and_skips_dropped_events() {
        let tokenizer = tokenizer();
        let (sender, receiver) = mpsc::channel();
        sender.send(event("一つ目", 0)).expect("send");
        sender.send(event("no japanese", 1)).expect("send");
        sender.send(event("一つ目", 2)).expect("send");
        sender.send(event("二つ目", 3)).expect("send");
        drop(sender);
        let mut texts = Vec::new();
        let mut errors = Vec::new();

        run(
            &receiver,
            Normalizer::default(),
            &tokenizer,
            |sentence| texts.push(sentence.text),
            |error| errors.push(error),
        );

        assert_eq!(texts, vec!["一つ目".to_string(), "二つ目".to_string()]);
        assert_eq!(errors, Vec::<String>::new());
    }

    #[test]
    fn sentence_payload_serializes_with_camel_case_keys() {
        let tokenizer = tokenizer();
        let mut normalizer = Normalizer::default();
        let sentence = analyze(&mut normalizer, &tokenizer, event("猫", 5))
            .expect("analyze")
            .expect("sentence");

        let json = serde_json::to_value(&sentence).expect("serialize");

        assert_eq!(json["text"], serde_json::json!("猫"));
        assert_eq!(json["sourceId"], serde_json::json!("clipboard"));
        assert_eq!(json["timestamp"], serde_json::json!(5));
        assert_eq!(
            json["windowInfo"],
            serde_json::json!({ "title": "Player", "appName": null })
        );
        assert_eq!(
            json["tokens"],
            serde_json::json!([{
                "surface": "猫",
                "lemma": "猫",
                "reading": "ネコ",
                "pos": "名詞",
                "start": 0,
                "end": 1
            }])
        );
    }
}
