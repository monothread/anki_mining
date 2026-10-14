use std::sync::mpsc::Sender;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub title: String,
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSourceEvent {
    pub text: String,
    pub source_id: String,
    /// Unix epoch in milliseconds.
    pub timestamp: u64,
    pub window_info: Option<WindowInfo>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SourceError {
    #[error("source `{0}` is already running")]
    AlreadyRunning(String),
    #[error("source `{0}` is not running")]
    NotRunning(String),
    #[error("source `{id}` failed: {reason}")]
    Failed { id: String, reason: String },
}

/// A producer of Japanese text (clipboard, websocket, OCR, ...).
pub trait TextSource: Send {
    fn id(&self) -> &str;
    fn start(&mut self, sink: Sender<TextSourceEvent>) -> Result<(), SourceError>;
    fn stop(&mut self) -> Result<(), SourceError>;
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use serde_json::json;

    use super::*;

    fn sample_event() -> TextSourceEvent {
        TextSourceEvent {
            text: "猫が好きです".to_string(),
            source_id: "clipboard".to_string(),
            timestamp: 1_760_400_000_000,
            window_info: Some(WindowInfo {
                title: "Player".to_string(),
                app_name: Some("mpv".to_string()),
            }),
        }
    }

    #[test]
    fn event_serializes_with_camel_case_keys() {
        assert_eq!(
            serde_json::to_value(sample_event()).expect("event must serialize"),
            json!({
                "text": "猫が好きです",
                "sourceId": "clipboard",
                "timestamp": 1_760_400_000_000_u64,
                "windowInfo": { "title": "Player", "appName": "mpv" }
            })
        );
    }

    #[test]
    fn event_serializes_missing_window_info_as_null() {
        let event = TextSourceEvent {
            window_info: None,
            ..sample_event()
        };

        let value = serde_json::to_value(event).expect("event must serialize");

        assert_eq!(value["windowInfo"], serde_json::Value::Null);
    }

    #[test]
    fn event_round_trips_through_json() {
        let json = serde_json::to_string(&sample_event()).expect("event must serialize");

        let parsed: TextSourceEvent = serde_json::from_str(&json).expect("event must parse");

        assert_eq!(parsed, sample_event());
    }

    #[test]
    fn window_info_parses_a_null_app_name() {
        let parsed: WindowInfo =
            serde_json::from_value(json!({ "title": "Game", "appName": null }))
                .expect("window info must parse");

        assert_eq!(
            parsed,
            WindowInfo {
                title: "Game".to_string(),
                app_name: None,
            }
        );
    }

    #[test]
    fn event_rejects_a_payload_without_text() {
        let result = serde_json::from_value::<TextSourceEvent>(json!({
            "sourceId": "clipboard",
            "timestamp": 1,
            "windowInfo": null
        }));

        assert!(result.is_err());
    }

    #[test]
    fn source_errors_have_descriptive_messages() {
        assert_eq!(
            SourceError::AlreadyRunning("clipboard".to_string()).to_string(),
            "source `clipboard` is already running"
        );
        assert_eq!(
            SourceError::NotRunning("clipboard".to_string()).to_string(),
            "source `clipboard` is not running"
        );
        assert_eq!(
            SourceError::Failed {
                id: "ocr".to_string(),
                reason: "no display".to_string(),
            }
            .to_string(),
            "source `ocr` failed: no display"
        );
    }

    struct FakeSource {
        sink: Option<Sender<TextSourceEvent>>,
    }

    impl TextSource for FakeSource {
        fn id(&self) -> &str {
            "fake"
        }

        fn start(&mut self, sink: Sender<TextSourceEvent>) -> Result<(), SourceError> {
            if self.sink.is_some() {
                return Err(SourceError::AlreadyRunning(self.id().to_string()));
            }
            self.sink = Some(sink);
            Ok(())
        }

        fn stop(&mut self) -> Result<(), SourceError> {
            match self.sink.take() {
                Some(_) => Ok(()),
                None => Err(SourceError::NotRunning(self.id().to_string())),
            }
        }
    }

    #[test]
    fn a_source_delivers_events_to_the_sink_it_was_started_with() {
        let (sender, receiver) = mpsc::channel();
        let mut source = FakeSource { sink: None };

        source.start(sender).expect("start must succeed");
        source
            .sink
            .as_ref()
            .expect("sink must be stored")
            .send(sample_event())
            .expect("send must succeed");

        assert_eq!(receiver.recv().expect("event must arrive"), sample_event());
    }

    #[test]
    fn a_source_cannot_be_started_twice_or_stopped_when_idle() {
        let (sender, _receiver) = mpsc::channel();
        let (second_sender, _second_receiver) = mpsc::channel();
        let mut source = FakeSource { sink: None };

        assert_eq!(
            source.stop(),
            Err(SourceError::NotRunning("fake".to_string()))
        );
        assert_eq!(source.start(sender), Ok(()));
        assert_eq!(
            source.start(second_sender),
            Err(SourceError::AlreadyRunning("fake".to_string()))
        );
        assert_eq!(source.stop(), Ok(()));
    }
}
