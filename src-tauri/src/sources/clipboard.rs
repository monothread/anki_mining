use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::text_event;
use super::worker::{is_stopped, Worker};
use crate::types::text_source::{SourceError, TextSource, TextSourceEvent};

pub const SOURCE_ID: &str = "clipboard";
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Reads the text currently on the clipboard.
pub trait ClipboardReader {
    /// `Ok(None)` means the clipboard holds no text (empty or another format).
    fn read_text(&mut self) -> Result<Option<String>, String>;
}

type ReaderFactory = Arc<dyn Fn() -> Result<Box<dyn ClipboardReader>, String> + Send + Sync>;

/// Emits an event whenever the clipboard text changes. Text already on the clipboard when the
/// source starts is not emitted.
pub struct ClipboardSource {
    factory: ReaderFactory,
    poll_interval: Duration,
    worker: Option<Worker>,
}

impl ClipboardSource {
    /// The reader is created on the polling thread, so it does not need to be `Send`.
    pub fn new<F>(factory: F, poll_interval: Duration) -> ClipboardSource
    where
        F: Fn() -> Result<Box<dyn ClipboardReader>, String> + Send + Sync + 'static,
    {
        ClipboardSource {
            factory: Arc::new(factory),
            poll_interval,
            worker: None,
        }
    }

    /// Source backed by the system clipboard (X11 now; Wayland arrives with the platform layer).
    pub fn system() -> ClipboardSource {
        ClipboardSource::new(system_reader, DEFAULT_POLL_INTERVAL)
    }
}

impl TextSource for ClipboardSource {
    fn id(&self) -> &str {
        SOURCE_ID
    }

    fn start(&mut self, sink: Sender<TextSourceEvent>) -> Result<(), SourceError> {
        if self.worker.is_some() {
            return Err(SourceError::AlreadyRunning(SOURCE_ID.to_string()));
        }

        let factory = Arc::clone(&self.factory);
        let poll_interval = self.poll_interval;
        let (ready_sender, ready_receiver) = mpsc::channel::<Result<(), String>>();
        let worker = Worker::spawn("clipboard-source", move |stop| {
            let mut reader = match factory() {
                Ok(reader) => reader,
                Err(reason) => {
                    let _ = ready_sender.send(Err(reason));
                    return;
                }
            };
            // The baseline is read before reporting readiness so text copied right after
            // `start` returns is seen as a change instead of being taken as existing content.
            let mut last_seen = reader.read_text().ok().flatten();
            let _ = ready_sender.send(Ok(()));

            while !is_stopped(&stop) {
                thread::sleep(poll_interval);
                if let Ok(Some(text)) = reader.read_text() {
                    if last_seen.as_deref() == Some(text.as_str()) {
                        continue;
                    }
                    let trimmed = text.trim().to_string();
                    last_seen = Some(text);
                    if !trimmed.is_empty() && sink.send(text_event(SOURCE_ID, trimmed)).is_err() {
                        break;
                    }
                }
            }
        })
        .map_err(|error| failed(error.to_string()))?;

        match ready_receiver.recv() {
            Ok(Ok(())) => {
                self.worker = Some(worker);
                Ok(())
            }
            Ok(Err(reason)) => {
                worker.stop();
                Err(failed(reason))
            }
            Err(_) => {
                worker.stop();
                Err(failed(
                    "the clipboard thread ended unexpectedly".to_string(),
                ))
            }
        }
    }

    fn stop(&mut self) -> Result<(), SourceError> {
        match self.worker.take() {
            Some(worker) => {
                worker.stop();
                Ok(())
            }
            None => Err(SourceError::NotRunning(SOURCE_ID.to_string())),
        }
    }
}

fn failed(reason: String) -> SourceError {
    SourceError::Failed {
        id: SOURCE_ID.to_string(),
        reason,
    }
}

struct SystemClipboard(arboard::Clipboard);

impl ClipboardReader for SystemClipboard {
    fn read_text(&mut self) -> Result<Option<String>, String> {
        match self.0.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn system_reader() -> Result<Box<dyn ClipboardReader>, String> {
    let clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    Ok(Box::new(SystemClipboard(clipboard)))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    const WAIT: Duration = Duration::from_secs(3);

    /// The X11 clipboard is shared by every client of the display, so those tests run one by one.
    static DISPLAY_LOCK: Mutex<()> = Mutex::new(());

    const FAST_POLL: Duration = Duration::from_millis(2);

    type Script = Arc<Mutex<VecDeque<Result<Option<String>, String>>>>;

    /// Replays scripted reads, then keeps returning the last value forever.
    struct ScriptedReader {
        script: Script,
        last: Result<Option<String>, String>,
    }

    impl ClipboardReader for ScriptedReader {
        fn read_text(&mut self) -> Result<Option<String>, String> {
            if let Some(next) = self.script.lock().expect("lock").pop_front() {
                self.last = next;
            }
            self.last.clone()
        }
    }

    fn text(value: &str) -> Result<Option<String>, String> {
        Ok(Some(value.to_string()))
    }

    fn source_with_script(reads: Vec<Result<Option<String>, String>>) -> (ClipboardSource, Script) {
        let script: Script = Arc::new(Mutex::new(reads.into_iter().collect()));
        let factory_script = Arc::clone(&script);
        let source = ClipboardSource::new(
            move || {
                Ok(Box::new(ScriptedReader {
                    script: Arc::clone(&factory_script),
                    last: Ok(None),
                }) as Box<dyn ClipboardReader>)
            },
            FAST_POLL,
        );
        (source, script)
    }

    fn push(script: &Script, read: Result<Option<String>, String>) {
        script.lock().expect("lock").push_back(read);
    }

    #[test]
    fn identifies_itself_as_the_clipboard_source() {
        let (source, _script) = source_with_script(vec![]);

        assert_eq!(source.id(), "clipboard");
    }

    #[test]
    fn does_not_emit_the_text_already_on_the_clipboard() {
        let (mut source, _script) = source_with_script(vec![text("既存")]);
        let (sender, receiver) = mpsc::channel();

        source.start(sender).expect("start");
        thread::sleep(Duration::from_millis(50));

        assert!(receiver.try_recv().is_err());
        source.stop().expect("stop");
    }

    #[test]
    fn emits_an_event_when_the_text_changes() {
        let (mut source, script) = source_with_script(vec![text("既存")]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");
        let before = super::super::now_millis();

        push(&script, text("新しい文"));
        let event = receiver.recv_timeout(WAIT).expect("event");

        assert_eq!(event.text, "新しい文");
        assert_eq!(event.source_id, "clipboard");
        assert_eq!(event.window_info, None);
        assert!(event.timestamp >= before);
        source.stop().expect("stop");
    }

    #[test]
    fn emits_each_change_once_and_in_order() {
        let (mut source, script) = source_with_script(vec![]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");

        push(&script, text("一"));
        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "一");
        push(&script, text("二"));
        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "二");
        thread::sleep(Duration::from_millis(50));

        assert!(receiver.try_recv().is_err());
        source.stop().expect("stop");
    }

    #[test]
    fn trims_whitespace_and_ignores_blank_text() {
        let (mut source, script) = source_with_script(vec![]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");

        push(&script, text("   \n"));
        push(&script, text("  前後の空白\n"));
        let event = receiver.recv_timeout(WAIT).expect("event");

        assert_eq!(event.text, "前後の空白");
        source.stop().expect("stop");
    }

    #[test]
    fn ignores_read_errors_and_missing_text_and_keeps_polling() {
        let (mut source, script) = source_with_script(vec![]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");

        push(&script, Err("temporarily unavailable".to_string()));
        push(&script, Ok(None));
        push(&script, text("回復"));
        let event = receiver.recv_timeout(WAIT).expect("event");

        assert_eq!(event.text, "回復");
        source.stop().expect("stop");
    }

    #[test]
    fn stops_polling_when_the_receiver_is_dropped() {
        let (mut source, script) = source_with_script(vec![]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");
        drop(receiver);

        push(&script, text("誰もいない"));
        thread::sleep(Duration::from_millis(50));

        assert!(source.stop().is_ok());
    }

    #[test]
    fn reports_a_failure_when_the_reader_cannot_be_created() {
        let mut source = ClipboardSource::new(|| Err("no display".to_string()), FAST_POLL);
        let (sender, _receiver) = mpsc::channel();

        let error = source.start(sender).expect_err("must fail");

        assert_eq!(
            error,
            SourceError::Failed {
                id: "clipboard".to_string(),
                reason: "no display".to_string(),
            }
        );
        assert_eq!(
            source.stop(),
            Err(SourceError::NotRunning("clipboard".to_string()))
        );
    }

    #[test]
    fn cannot_be_started_twice_or_stopped_when_idle() {
        let (mut source, _script) = source_with_script(vec![]);
        let (first_sender, _first_receiver) = mpsc::channel();
        let (second_sender, _second_receiver) = mpsc::channel();

        assert_eq!(
            source.stop(),
            Err(SourceError::NotRunning("clipboard".to_string()))
        );
        source.start(first_sender).expect("start");
        assert_eq!(
            source.start(second_sender),
            Err(SourceError::AlreadyRunning("clipboard".to_string()))
        );
        assert_eq!(source.stop(), Ok(()));
    }

    #[test]
    fn can_be_restarted_and_ignores_text_copied_while_stopped() {
        let (mut source, script) = source_with_script(vec![text("前")]);
        let (first_sender, _first_receiver) = mpsc::channel();
        source.start(first_sender).expect("start");
        source.stop().expect("stop");
        push(&script, text("停止中にコピー"));
        let (sender, receiver) = mpsc::channel();

        source.start(sender).expect("restart");
        thread::sleep(Duration::from_millis(50));
        push(&script, text("再開後"));

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "再開後");
        source.stop().expect("stop");
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn system_clipboard_emits_text_copied_by_another_client() {
        let _display = crate::x11_test_lock();
        let _display = DISPLAY_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut owner = arboard::Clipboard::new().expect("clipboard");
        owner.set_text("最初のテキスト").expect("set text");
        let mut source = ClipboardSource::new(system_reader, FAST_POLL);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");

        owner.set_text("コピーした文").expect("set text");
        let event = receiver.recv_timeout(WAIT).expect("event");

        assert_eq!(event.text, "コピーした文");
        assert_eq!(event.source_id, "clipboard");
        source.stop().expect("stop");
    }

    #[test]
    #[ignore = "needs an X11 display; run through `scripts/dev.sh test`"]
    fn system_reader_reports_missing_text_as_none() {
        let _display = crate::x11_test_lock();
        let _display = DISPLAY_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut clipboard = arboard::Clipboard::new().expect("clipboard");
        clipboard.clear().expect("clear");
        let mut reader = SystemClipboard(clipboard);

        assert_eq!(reader.read_text(), Ok(None));
    }
}
