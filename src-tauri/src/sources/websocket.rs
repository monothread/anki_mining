use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tungstenite::protocol::WebSocketConfig;
use tungstenite::{accept_with_config, HandshakeError, Message, ServerHandshake, WebSocket};

use super::text_event;
use super::worker::{is_stopped, Worker};
use crate::types::text_source::{SourceError, TextSource, TextSourceEvent};

pub const SOURCE_ID: &str = "websocket";
/// Larger messages close the connection; a subtitle line is far smaller than this.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(20);
const READ_TIMEOUT: Duration = Duration::from_millis(100);

type Handshake =
    HandshakeError<ServerHandshake<TcpStream, tungstenite::handshake::server::NoCallback>>;

/// Receives text over a local websocket (Textractor, owocr, asbplayer, ...).
///
/// Every text message becomes one event. The server only listens on the loopback interface.
pub struct WebSocketSource {
    port: u16,
    worker: Option<Worker>,
    local_addr: Option<SocketAddr>,
}

impl WebSocketSource {
    /// `port` 0 asks the OS for a free port; see [`WebSocketSource::local_addr`].
    pub fn new(port: u16) -> WebSocketSource {
        WebSocketSource {
            port,
            worker: None,
            local_addr: None,
        }
    }

    /// Address the server is listening on while the source is running.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }
}

impl TextSource for WebSocketSource {
    fn id(&self) -> &str {
        SOURCE_ID
    }

    fn start(&mut self, sink: Sender<TextSourceEvent>) -> Result<(), SourceError> {
        if self.worker.is_some() {
            return Err(SourceError::AlreadyRunning(SOURCE_ID.to_string()));
        }

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, self.port)).map_err(failed)?;
        listener.set_nonblocking(true).map_err(failed)?;
        let local_addr = listener.local_addr().map_err(failed)?;

        let worker = Worker::spawn("websocket-source", move |stop| {
            accept_connections(&listener, &sink, &stop);
        })
        .map_err(failed)?;

        self.worker = Some(worker);
        self.local_addr = Some(local_addr);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), SourceError> {
        match self.worker.take() {
            Some(worker) => {
                worker.stop();
                self.local_addr = None;
                Ok(())
            }
            None => Err(SourceError::NotRunning(SOURCE_ID.to_string())),
        }
    }
}

fn failed(error: io::Error) -> SourceError {
    SourceError::Failed {
        id: SOURCE_ID.to_string(),
        reason: error.to_string(),
    }
}

fn accept_connections(
    listener: &TcpListener,
    sink: &Sender<TextSourceEvent>,
    stop: &Arc<AtomicBool>,
) {
    let mut connections: Vec<JoinHandle<()>> = Vec::new();

    while !is_stopped(stop) {
        match listener.accept() {
            Ok((stream, _)) => {
                let sink = sink.clone();
                let stop = Arc::clone(stop);
                connections.push(thread::spawn(move || {
                    serve_connection(stream, &sink, &stop);
                }));
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(_) => break,
        }
        connections.retain(|connection| !connection.is_finished());
    }

    for connection in connections {
        let _ = connection.join();
    }
}

fn serve_connection(stream: TcpStream, sink: &Sender<TextSourceEvent>, stop: &AtomicBool) {
    if stream.set_nonblocking(false).is_err()
        || stream.set_read_timeout(Some(READ_TIMEOUT)).is_err()
    {
        return;
    }
    let Some(mut socket) = handshake(stream, stop) else {
        return;
    };

    while !is_stopped(stop) {
        match socket.read() {
            Ok(Message::Text(text)) => {
                let trimmed = text.as_str().trim();
                if !trimmed.is_empty()
                    && sink
                        .send(text_event(SOURCE_ID, trimmed.to_string()))
                        .is_err()
                {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error)) if is_timeout(&error) => {}
            Err(_) => break,
        }
    }
    let _ = socket.close(None);
}

fn handshake(stream: TcpStream, stop: &AtomicBool) -> Option<WebSocket<TcpStream>> {
    let config = WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE_BYTES));
    let mut attempt: Result<WebSocket<TcpStream>, Handshake> =
        accept_with_config(stream, Some(config));

    loop {
        match attempt {
            Ok(socket) => return Some(socket),
            Err(HandshakeError::Interrupted(middle)) => {
                if is_stopped(stop) {
                    return None;
                }
                attempt = middle.handshake();
            }
            Err(HandshakeError::Failure(_)) => return None,
        }
    }
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::mpsc::{self, Receiver};

    use tungstenite::stream::MaybeTlsStream;

    use super::*;

    const WAIT: Duration = Duration::from_secs(3);

    type Client = WebSocket<MaybeTlsStream<TcpStream>>;

    fn started_source() -> (WebSocketSource, Receiver<TextSourceEvent>) {
        let (sender, receiver) = mpsc::channel();
        let mut source = WebSocketSource::new(0);
        source.start(sender).expect("start");
        (source, receiver)
    }

    fn connect(source: &WebSocketSource) -> Client {
        let address = source.local_addr().expect("running source has an address");
        let (client, _) = tungstenite::connect(format!("ws://{address}")).expect("connect");
        client
    }

    #[test]
    fn identifies_itself_as_the_websocket_source() {
        assert_eq!(WebSocketSource::new(0).id(), "websocket");
    }

    #[test]
    fn listens_only_on_the_loopback_interface_on_a_free_port() {
        let (mut source, _receiver) = started_source();

        let address = source.local_addr().expect("address");

        assert!(address.ip().is_loopback());
        assert_ne!(address.port(), 0);
        source.stop().expect("stop");
    }

    #[test]
    fn delivers_a_text_message_as_an_event() {
        let (mut source, receiver) = started_source();
        let mut client = connect(&source);
        let before = super::super::now_millis();

        client.send(Message::text("猫が好きです")).expect("send");
        let event = receiver.recv_timeout(WAIT).expect("event");

        assert_eq!(event.text, "猫が好きです");
        assert_eq!(event.source_id, "websocket");
        assert_eq!(event.window_info, None);
        assert!(event.timestamp >= before);
        source.stop().expect("stop");
    }

    #[test]
    fn delivers_messages_in_order_and_trims_whitespace() {
        let (mut source, receiver) = started_source();
        let mut client = connect(&source);

        client.send(Message::text("  一つ目\n")).expect("send");
        client.send(Message::text("二つ目")).expect("send");
        let texts: Vec<String> = (0..2)
            .map(|_| receiver.recv_timeout(WAIT).expect("event").text)
            .collect();

        assert_eq!(texts, vec!["一つ目".to_string(), "二つ目".to_string()]);
        source.stop().expect("stop");
    }

    #[test]
    fn ignores_blank_and_binary_messages() {
        let (mut source, receiver) = started_source();
        let mut client = connect(&source);

        client.send(Message::text("   \n")).expect("send");
        client
            .send(Message::binary(vec![1_u8, 2, 3]))
            .expect("send");
        client.send(Message::text("有効")).expect("send");

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "有効");
        assert!(receiver.try_recv().is_err());
        source.stop().expect("stop");
    }

    #[test]
    fn serves_several_clients_at_once() {
        let (mut source, receiver) = started_source();
        let mut first = connect(&source);
        let mut second = connect(&source);

        first.send(Message::text("最初")).expect("send");
        second.send(Message::text("二番目")).expect("send");
        let mut texts: Vec<String> = (0..2)
            .map(|_| receiver.recv_timeout(WAIT).expect("event").text)
            .collect();
        texts.sort();

        let mut expected = vec!["最初".to_string(), "二番目".to_string()];
        expected.sort();
        assert_eq!(texts, expected);
        source.stop().expect("stop");
    }

    #[test]
    fn closes_the_connection_of_a_client_that_sends_an_oversized_message() {
        let (mut source, receiver) = started_source();
        let mut client = connect(&source);

        let oversized = "あ".repeat(MAX_MESSAGE_BYTES);
        let _ = client.send(Message::text(oversized));
        let closed = (0..50).any(|_| {
            thread::sleep(Duration::from_millis(50));
            client.read().is_err()
        });

        assert!(closed);
        assert!(receiver.try_recv().is_err());
        source.stop().expect("stop");
    }

    #[test]
    fn survives_a_client_that_does_not_speak_websocket() {
        let (mut source, receiver) = started_source();
        let address = source.local_addr().expect("address");
        let mut garbage = TcpStream::connect(address).expect("tcp connect");
        garbage
            .write_all(b"not a websocket handshake\r\n\r\n")
            .expect("write");

        let mut client = connect(&source);
        client.send(Message::text("まだ動く")).expect("send");

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "まだ動く");
        source.stop().expect("stop");
    }

    #[test]
    fn keeps_running_after_the_receiver_is_dropped() {
        let (mut source, receiver) = started_source();
        let mut client = connect(&source);
        drop(receiver);

        let _ = client.send(Message::text("誰もいない"));
        thread::sleep(Duration::from_millis(200));

        assert!(source.stop().is_ok());
    }

    #[test]
    fn stop_closes_the_port_and_clears_the_address() {
        let (mut source, _receiver) = started_source();
        let address = source.local_addr().expect("address");

        source.stop().expect("stop");

        assert_eq!(source.local_addr(), None);
        assert!(TcpStream::connect(address).is_err());
    }

    #[test]
    fn can_be_restarted_after_stopping() {
        let (mut source, _first_receiver) = started_source();
        source.stop().expect("stop");
        let (sender, receiver) = mpsc::channel();

        source.start(sender).expect("restart");
        let mut client = connect(&source);
        client.send(Message::text("再開")).expect("send");

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "再開");
        source.stop().expect("stop");
    }

    #[test]
    fn cannot_be_started_twice_or_stopped_when_idle() {
        let (mut source, _receiver) = started_source();
        let (second_sender, _second_receiver) = mpsc::channel();

        assert_eq!(
            source.start(second_sender),
            Err(SourceError::AlreadyRunning("websocket".to_string()))
        );
        source.stop().expect("stop");
        assert_eq!(
            source.stop(),
            Err(SourceError::NotRunning("websocket".to_string()))
        );
    }

    #[test]
    fn reports_a_failure_when_the_port_is_already_taken() {
        let (mut running, _receiver) = started_source();
        let taken_port = running.local_addr().expect("address").port();
        let (sender, _other_receiver) = mpsc::channel();
        let mut conflicting = WebSocketSource::new(taken_port);

        let error = conflicting.start(sender).expect_err("must fail");

        assert!(matches!(error, SourceError::Failed { ref id, .. } if id == "websocket"));
        assert_eq!(conflicting.local_addr(), None);
        running.stop().expect("stop");
    }

    #[test]
    fn is_timeout_only_matches_timeout_like_errors() {
        assert!(is_timeout(&io::Error::from(ErrorKind::WouldBlock)));
        assert!(is_timeout(&io::Error::from(ErrorKind::TimedOut)));
        assert!(!is_timeout(&io::Error::from(ErrorKind::ConnectionReset)));
    }
}
