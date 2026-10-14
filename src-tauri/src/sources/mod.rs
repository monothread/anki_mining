pub mod clipboard;
pub mod ocr;
pub mod websocket;
mod worker;

use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::text_source::TextSourceEvent;

/// Builds an event without window information, stamped with the current time.
fn text_event(source_id: &str, text: String) -> TextSourceEvent {
    TextSourceEvent {
        text,
        source_id: source_id.to_string(),
        timestamp: now_millis(),
        window_info: None,
    }
}

pub fn now_millis() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_event_carries_the_text_and_source_without_window_info() {
        let before = now_millis();

        let event = text_event("clipboard", "猫が好きです".to_string());

        assert_eq!(event.text, "猫が好きです");
        assert_eq!(event.source_id, "clipboard");
        assert_eq!(event.window_info, None);
        assert!(event.timestamp >= before);
        assert!(event.timestamp <= now_millis());
    }

    #[test]
    fn now_millis_is_a_recent_unix_timestamp() {
        // 2024-01-01T00:00:00Z in milliseconds.
        assert!(now_millis() > 1_704_067_200_000);
    }
}
