//! Text source that reads the subtitles of a running mpv through its JSON IPC socket.
//!
//! mpv has to be started with `--input-ipc-server=<path>` (or `input-ipc-server=<path>` in
//! `mpv.conf`). The text is exact, with no OCR, and arrives when the subtitle changes.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use super::worker::{is_stopped, Worker};
use crate::types::text_source::{SourceError, TextSource, TextSourceEvent, WindowInfo};

pub const SOURCE_ID: &str = "mpv";
pub const APP_NAME: &str = "mpv";
pub const DEFAULT_SOCKET: &str = "/tmp/mpvsocket";
/// How often a connection to mpv is retried while mpv is not running.
pub const RECONNECT_INTERVAL: Duration = Duration::from_millis(1_000);

const SUBTITLE_PROPERTY: &str = "sub-text";
const TITLE_PROPERTY: &str = "media-title";
const SUBTITLE_OBSERVER_ID: u64 = 1;
const TITLE_OBSERVER_ID: u64 = 2;
const READ_TIMEOUT: Duration = Duration::from_millis(100);
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(20);
/// A line longer than this is discarded: no subtitle needs more than a few kilobytes.
const MAX_LINE_BYTES: usize = 1 << 20;

/// Whether the source is connected to a running mpv; shared with whoever shows the status.
#[derive(Debug, Clone, Default)]
pub struct MpvConnection(Arc<AtomicBool>);

impl MpvConnection {
    pub fn is_connected(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn set(&self, connected: bool) {
        self.0.store(connected, Ordering::SeqCst);
    }
}

/// Follows the subtitles of the mpv listening on `socket`, reconnecting when mpv is restarted.
pub struct MpvSource {
    socket: PathBuf,
    reconnect_interval: Duration,
    connection: MpvConnection,
    worker: Option<Worker>,
}

impl MpvSource {
    pub fn new(socket: &Path) -> MpvSource {
        MpvSource::with_reconnect_interval(socket, RECONNECT_INTERVAL)
    }

    pub fn with_reconnect_interval(socket: &Path, reconnect_interval: Duration) -> MpvSource {
        MpvSource {
            socket: socket.to_path_buf(),
            reconnect_interval,
            connection: MpvConnection::default(),
            worker: None,
        }
    }

    /// Handle that tells whether mpv is connected.
    pub fn connection(&self) -> MpvConnection {
        self.connection.clone()
    }
}

impl TextSource for MpvSource {
    fn id(&self) -> &str {
        SOURCE_ID
    }

    fn start(&mut self, sink: Sender<TextSourceEvent>) -> Result<(), SourceError> {
        if self.worker.is_some() {
            return Err(SourceError::AlreadyRunning(SOURCE_ID.to_string()));
        }
        let socket = self.socket.clone();
        let reconnect_interval = self.reconnect_interval;
        let connection = self.connection.clone();

        let worker = Worker::spawn("mpv-source", move |stop| {
            while !is_stopped(&stop) {
                if let Ok(stream) = UnixStream::connect(&socket) {
                    connection.set(true);
                    let keep_going = follow(stream, &sink, &stop);
                    connection.set(false);
                    if !keep_going {
                        return;
                    }
                }
                sleep_unless_stopped(reconnect_interval, &stop);
            }
        })
        .map_err(|error| SourceError::Failed {
            id: SOURCE_ID.to_string(),
            reason: error.to_string(),
        })?;

        self.worker = Some(worker);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), SourceError> {
        match self.worker.take() {
            Some(worker) => {
                worker.stop();
                self.connection.set(false);
                Ok(())
            }
            None => Err(SourceError::NotRunning(SOURCE_ID.to_string())),
        }
    }
}

fn sleep_unless_stopped(duration: Duration, stop: &AtomicBool) {
    let mut waited = Duration::ZERO;
    while waited < duration && !is_stopped(stop) {
        thread::sleep(STOP_CHECK_INTERVAL);
        waited += STOP_CHECK_INTERVAL;
    }
}

/// Talks to one mpv connection until it closes or the source is stopped. Returns false when
/// nobody is listening for the text any more, so the caller can stop for good.
fn follow(mut stream: UnixStream, sink: &Sender<TextSourceEvent>, stop: &AtomicBool) -> bool {
    if stream.set_read_timeout(Some(READ_TIMEOUT)).is_err() || send_observers(&mut stream).is_err()
    {
        return true;
    }

    let mut title: Option<String> = None;
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0_u8; 4096];
    while !is_stopped(stop) {
        match stream.read(&mut chunk) {
            Ok(0) => return true,
            Ok(read) => pending.extend_from_slice(&chunk[..read]),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                continue;
            }
            Err(_) => return true,
        }

        for line in take_lines(&mut pending) {
            if let Some(text) = handle_message(&line, &mut title) {
                if sink.send(subtitle_event(text, title.as_deref())).is_err() {
                    return false;
                }
            }
        }
    }
    true
}

fn send_observers(stream: &mut UnixStream) -> std::io::Result<()> {
    for (id, property) in [
        (SUBTITLE_OBSERVER_ID, SUBTITLE_PROPERTY),
        (TITLE_OBSERVER_ID, TITLE_PROPERTY),
    ] {
        let command = json!({ "command": ["observe_property", id, property] });
        writeln!(stream, "{command}")?;
    }
    stream.flush()
}

/// Removes and returns the complete lines of `buffer`, keeping a trailing partial line. A line
/// that grows beyond [`MAX_LINE_BYTES`] without a newline is dropped.
fn take_lines(buffer: &mut Vec<u8>) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = buffer.drain(..=position).collect();
        lines.push(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned());
    }
    if buffer.len() > MAX_LINE_BYTES {
        buffer.clear();
    }
    lines
}

/// Handles one message from mpv. Returns the new subtitle text when the message announces one;
/// the media title is remembered in `title`.
fn handle_message(line: &str, title: &mut Option<String>) -> Option<String> {
    let message: Value = serde_json::from_str(line).ok()?;
    if message.get("event")?.as_str()? != "property-change" {
        return None;
    }
    let data = message.get("data").and_then(Value::as_str);
    match message.get("name")?.as_str()? {
        TITLE_PROPERTY => {
            *title = data.map(str::to_string).filter(|value| !value.is_empty());
            None
        }
        SUBTITLE_PROPERTY => {
            let text = data?.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
        _ => None,
    }
}

fn subtitle_event(text: String, title: Option<&str>) -> TextSourceEvent {
    TextSourceEvent {
        text,
        source_id: SOURCE_ID.to_string(),
        timestamp: super::now_millis(),
        window_info: Some(WindowInfo {
            title: title.unwrap_or(APP_NAME).to_string(),
            app_name: Some(APP_NAME.to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc::{self, Receiver};

    use tempfile::TempDir;

    use super::*;

    const WAIT: Duration = Duration::from_secs(3);
    const FAST_RECONNECT: Duration = Duration::from_millis(30);

    fn socket_in(directory: &TempDir) -> PathBuf {
        directory.path().join("mpv.sock")
    }

    fn property(name: &str, data: Value) -> String {
        json!({ "event": "property-change", "id": 1, "name": name, "data": data }).to_string()
    }

    /// Accepts one connection and returns the stream plus the lines mpv would have received.
    fn accept(listener: &UnixListener, commands: usize) -> (UnixStream, Vec<String>) {
        let (stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let received = (0..commands)
            .map(|_| {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read command");
                line.trim_end().to_string()
            })
            .collect();
        (stream, received)
    }

    fn send(stream: &mut UnixStream, line: &str) {
        writeln!(stream, "{line}").expect("send");
    }

    fn started(path: &Path) -> (MpvSource, Receiver<TextSourceEvent>) {
        let (sender, receiver) = mpsc::channel();
        let mut source = MpvSource::with_reconnect_interval(path, FAST_RECONNECT);
        source.start(sender).expect("start");
        (source, receiver)
    }

    #[test]
    fn identifies_itself_as_the_mpv_source() {
        assert_eq!(MpvSource::new(Path::new("/tmp/x")).id(), "mpv");
    }

    #[test]
    fn asks_mpv_to_report_the_subtitle_and_the_media_title() {
        let directory = TempDir::new().expect("temp dir");
        let listener = UnixListener::bind(socket_in(&directory)).expect("bind");
        let (mut source, _receiver) = started(&socket_in(&directory));

        let (_stream, commands) = accept(&listener, 2);

        assert_eq!(
            commands,
            vec![
                r#"{"command":["observe_property",1,"sub-text"]}"#,
                r#"{"command":["observe_property",2,"media-title"]}"#,
            ]
        );
        source.stop().expect("stop");
    }

    #[test]
    fn forwards_subtitles_in_order_with_the_media_title() {
        let directory = TempDir::new().expect("temp dir");
        let listener = UnixListener::bind(socket_in(&directory)).expect("bind");
        let (mut source, receiver) = started(&socket_in(&directory));
        let (mut stream, _) = accept(&listener, 2);
        let before = super::super::now_millis();

        send(&mut stream, &property("media-title", json!("第1話")));
        send(
            &mut stream,
            &property("sub-text", json!("昨日、友達と映画を見た。")),
        );
        send(
            &mut stream,
            &property("sub-text", json!("とても楽しかった。")),
        );
        let first = receiver.recv_timeout(WAIT).expect("first");
        let second = receiver.recv_timeout(WAIT).expect("second");

        assert_eq!(first.text, "昨日、友達と映画を見た。");
        assert_eq!(second.text, "とても楽しかった。");
        assert_eq!(first.source_id, "mpv");
        assert_eq!(
            first.window_info,
            Some(WindowInfo {
                title: "第1話".to_string(),
                app_name: Some("mpv".to_string()),
            })
        );
        assert!(first.timestamp >= before);
        source.stop().expect("stop");
    }

    #[test]
    fn uses_the_application_name_as_title_until_the_media_title_is_known() {
        let directory = TempDir::new().expect("temp dir");
        let listener = UnixListener::bind(socket_in(&directory)).expect("bind");
        let (mut source, receiver) = started(&socket_in(&directory));
        let (mut stream, _) = accept(&listener, 2);

        send(&mut stream, &property("sub-text", json!("字幕")));

        let event = receiver.recv_timeout(WAIT).expect("event");
        assert_eq!(event.window_info.expect("window").title, "mpv");
        source.stop().expect("stop");
    }

    #[test]
    fn keeps_multi_line_subtitles_and_trims_the_ends() {
        let directory = TempDir::new().expect("temp dir");
        let listener = UnixListener::bind(socket_in(&directory)).expect("bind");
        let (mut source, receiver) = started(&socket_in(&directory));
        let (mut stream, _) = accept(&listener, 2);

        send(
            &mut stream,
            &property("sub-text", json!("  一行目\n二行目\n")),
        );

        assert_eq!(
            receiver.recv_timeout(WAIT).expect("event").text,
            "一行目\n二行目"
        );
        source.stop().expect("stop");
    }

    #[test]
    fn ignores_empty_subtitles_other_properties_replies_and_garbage() {
        let directory = TempDir::new().expect("temp dir");
        let listener = UnixListener::bind(socket_in(&directory)).expect("bind");
        let (mut source, receiver) = started(&socket_in(&directory));
        let (mut stream, _) = accept(&listener, 2);

        send(&mut stream, &property("sub-text", json!("")));
        send(&mut stream, &property("sub-text", json!("   ")));
        send(&mut stream, &property("sub-text", Value::Null));
        send(&mut stream, &property("sub-text", json!(42)));
        send(&mut stream, &property("volume", json!("90")));
        send(&mut stream, r#"{"error":"success"}"#);
        send(&mut stream, r#"{"event":"end-file","reason":"eof"}"#);
        send(&mut stream, r#"{"event":"property-change","id":9}"#);
        send(&mut stream, "this is not json");
        send(&mut stream, "[1, 2, 3]");
        send(&mut stream, &property("sub-text", json!("本物")));

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "本物");
        assert!(receiver.try_recv().is_err());
        source.stop().expect("stop");
    }

    #[test]
    fn handles_a_message_split_across_writes_and_several_in_one_write() {
        let directory = TempDir::new().expect("temp dir");
        let listener = UnixListener::bind(socket_in(&directory)).expect("bind");
        let (mut source, receiver) = started(&socket_in(&directory));
        let (mut stream, _) = accept(&listener, 2);
        let whole = format!("{}\n", property("sub-text", json!("分割された字幕")));
        // Cut inside the JSON text, at a character boundary of the ASCII part.
        let (head, tail) = whole.as_bytes().split_at(20);

        stream.write_all(head).expect("head");
        stream.flush().expect("flush");
        thread::sleep(Duration::from_millis(150));
        stream.write_all(tail).expect("tail");
        let first = receiver.recv_timeout(WAIT).expect("first");
        let both = format!(
            "{}\n{}\n",
            property("sub-text", json!("一")),
            property("sub-text", json!("二"))
        );
        stream.write_all(both.as_bytes()).expect("both");

        assert_eq!(first.text, "分割された字幕");
        assert_eq!(receiver.recv_timeout(WAIT).expect("one").text, "一");
        assert_eq!(receiver.recv_timeout(WAIT).expect("two").text, "二");
        source.stop().expect("stop");
    }

    #[test]
    fn a_multi_byte_character_split_across_reads_is_not_corrupted() {
        let directory = TempDir::new().expect("temp dir");
        let listener = UnixListener::bind(socket_in(&directory)).expect("bind");
        let (mut source, receiver) = started(&socket_in(&directory));
        let (mut stream, _) = accept(&listener, 2);
        let whole = format!("{}\n", property("sub-text", json!("猫")));
        let bytes = whole.as_bytes();
        // Cut in the middle of the three bytes of 猫.
        let cut = bytes
            .iter()
            .position(|byte| *byte == 0xE7)
            .expect("lead byte")
            + 1;

        stream.write_all(&bytes[..cut]).expect("head");
        stream.flush().expect("flush");
        thread::sleep(Duration::from_millis(150));
        stream.write_all(&bytes[cut..]).expect("tail");

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "猫");
        source.stop().expect("stop");
    }

    #[test]
    fn waits_for_mpv_to_start_and_connects_when_the_socket_appears() {
        let directory = TempDir::new().expect("temp dir");
        let path = socket_in(&directory);
        let (mut source, receiver) = started(&path);
        let connection = source.connection();
        thread::sleep(Duration::from_millis(100));
        assert!(!connection.is_connected());

        let listener = UnixListener::bind(&path).expect("bind");
        let (mut stream, _) = accept(&listener, 2);
        send(&mut stream, &property("sub-text", json!("遅れて起動")));

        assert_eq!(
            receiver.recv_timeout(WAIT).expect("event").text,
            "遅れて起動"
        );
        source.stop().expect("stop");
    }

    #[test]
    fn reconnects_and_observes_again_after_mpv_restarts() {
        let directory = TempDir::new().expect("temp dir");
        let path = socket_in(&directory);
        let listener = UnixListener::bind(&path).expect("bind");
        let (mut source, receiver) = started(&path);
        let (mut first, _) = accept(&listener, 2);
        send(&mut first, &property("sub-text", json!("一回目")));
        assert_eq!(receiver.recv_timeout(WAIT).expect("first").text, "一回目");

        drop(first);
        let (mut second, commands) = accept(&listener, 2);
        send(&mut second, &property("sub-text", json!("二回目")));

        assert_eq!(commands.len(), 2);
        assert_eq!(receiver.recv_timeout(WAIT).expect("second").text, "二回目");
        source.stop().expect("stop");
    }

    #[test]
    fn reports_whether_mpv_is_connected() {
        let directory = TempDir::new().expect("temp dir");
        let path = socket_in(&directory);
        let listener = UnixListener::bind(&path).expect("bind");
        let (mut source, _receiver) = started(&path);
        let connection = source.connection();

        let (stream, _) = accept(&listener, 2);
        wait_until(|| connection.is_connected());
        drop(stream);
        wait_until(|| !connection.is_connected());

        source.stop().expect("stop");
        assert!(!connection.is_connected());
    }

    fn wait_until(condition: impl Fn() -> bool) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("the condition was not met in time");
    }

    #[test]
    fn stops_promptly_while_connected_and_while_waiting_for_mpv() {
        let directory = TempDir::new().expect("temp dir");
        let path = socket_in(&directory);
        let listener = UnixListener::bind(&path).expect("bind");
        let (mut connected, _receiver) = started(&path);
        let (_stream, _) = accept(&listener, 2);
        let (mut waiting, _other) = started(&directory.path().join("missing.sock"));

        let started_at = std::time::Instant::now();
        connected.stop().expect("stop connected");
        waiting.stop().expect("stop waiting");

        assert!(started_at.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn stops_for_good_when_the_receiver_is_dropped() {
        let directory = TempDir::new().expect("temp dir");
        let listener = UnixListener::bind(socket_in(&directory)).expect("bind");
        let (mut source, receiver) = started(&socket_in(&directory));
        let (mut stream, _) = accept(&listener, 2);
        drop(receiver);

        send(&mut stream, &property("sub-text", json!("誰もいない")));
        thread::sleep(Duration::from_millis(200));

        assert!(source.stop().is_ok());
    }

    #[test]
    fn cannot_be_started_twice_or_stopped_when_idle() {
        let directory = TempDir::new().expect("temp dir");
        let path = socket_in(&directory);
        let mut source = MpvSource::with_reconnect_interval(&path, FAST_RECONNECT);
        let (first, _first_receiver) = mpsc::channel();
        let (second, _second_receiver) = mpsc::channel();

        assert_eq!(
            source.stop(),
            Err(SourceError::NotRunning("mpv".to_string()))
        );
        source.start(first).expect("start");
        assert_eq!(
            source.start(second),
            Err(SourceError::AlreadyRunning("mpv".to_string()))
        );
        assert_eq!(source.stop(), Ok(()));
    }

    #[test]
    fn can_be_restarted_after_stopping() {
        let directory = TempDir::new().expect("temp dir");
        let path = socket_in(&directory);
        let listener = UnixListener::bind(&path).expect("bind");
        let (mut source, _first) = started(&path);
        let (_stream, _) = accept(&listener, 2);
        source.stop().expect("stop");
        let (sender, receiver) = mpsc::channel();

        source.start(sender).expect("restart");
        let (mut stream, _) = accept(&listener, 2);
        send(&mut stream, &property("sub-text", json!("再開")));

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "再開");
        source.stop().expect("stop");
    }

    #[test]
    fn take_lines_keeps_the_partial_line_and_drops_a_runaway_one() {
        let mut buffer = b"one\ntwo\npart".to_vec();

        assert_eq!(
            take_lines(&mut buffer),
            vec!["one".to_string(), "two".to_string()]
        );
        assert_eq!(buffer, b"part".to_vec());

        buffer.extend_from_slice(b"ial\n");
        assert_eq!(take_lines(&mut buffer), vec!["partial".to_string()]);

        let mut runaway = vec![b'x'; MAX_LINE_BYTES + 1];
        assert_eq!(take_lines(&mut runaway), Vec::<String>::new());
        assert!(runaway.is_empty());
    }

    #[test]
    fn handle_message_distinguishes_subtitles_titles_and_everything_else() {
        let mut title = None;

        assert_eq!(
            handle_message(&property("media-title", json!("題名")), &mut title),
            None
        );
        assert_eq!(title.as_deref(), Some("題名"));
        assert_eq!(
            handle_message(&property("sub-text", json!(" 字幕 ")), &mut title),
            Some("字幕".to_string())
        );
        assert_eq!(
            handle_message(&property("media-title", Value::Null), &mut title),
            None
        );
        assert_eq!(title, None);
        assert_eq!(
            handle_message(&property("media-title", json!("")), &mut title),
            None
        );
        assert_eq!(title, None);
        assert_eq!(handle_message("{}", &mut title), None);
    }

    #[test]
    fn subtitle_events_carry_the_title_or_the_application_name() {
        assert_eq!(
            subtitle_event("字幕".to_string(), Some("映画")).window_info,
            Some(WindowInfo {
                title: "映画".to_string(),
                app_name: Some("mpv".to_string())
            })
        );
        assert_eq!(
            subtitle_event("字幕".to_string(), None).window_info,
            Some(WindowInfo {
                title: "mpv".to_string(),
                app_name: Some("mpv".to_string())
            })
        );
    }
}
