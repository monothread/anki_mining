//! A reader that stops waiting after a while. A read of a Windows named pipe cannot be given a
//! timeout (as a Unix socket can), so a thread does the blocking reads and hands the bytes over;
//! the loop that follows mpv then wakes up often enough to notice that the source was stopped.

use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

const CHUNK_BYTES: usize = 4096;

/// Reads from `reader` on a thread of its own and gives the bytes to whoever calls `read`, with a
/// timeout: when nothing arrives in time `read` fails with `TimedOut`, like a socket would.
pub struct TimedReader {
    chunks: Receiver<io::Result<Vec<u8>>>,
    unread: Vec<u8>,
    timeout: Duration,
}

impl TimedReader {
    pub fn spawn(mut reader: impl Read + Send + 'static, timeout: Duration) -> TimedReader {
        let (sender, chunks) = mpsc::channel();
        thread::spawn(move || {
            let mut buffer = [0_u8; CHUNK_BYTES];
            loop {
                let outcome = match reader.read(&mut buffer) {
                    // An empty chunk tells the other side that the stream ended.
                    Ok(0) => Ok(Vec::new()),
                    Ok(read) => Ok(buffer[..read].to_vec()),
                    Err(error) => Err(error),
                };
                let finished = !matches!(&outcome, Ok(chunk) if !chunk.is_empty());
                if sender.send(outcome).is_err() || finished {
                    return;
                }
            }
        });
        TimedReader {
            chunks,
            unread: Vec::new(),
            timeout,
        }
    }
}

impl Read for TimedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.unread.is_empty() {
            match self.chunks.recv_timeout(self.timeout) {
                Ok(Ok(chunk)) => {
                    if chunk.is_empty() {
                        return Ok(0);
                    }
                    self.unread = chunk;
                }
                Ok(Err(error)) => return Err(error),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(io::Error::new(
                        ErrorKind::TimedOut,
                        "nothing arrived in time",
                    ));
                }
                // The thread is gone: nothing more will come.
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let count = buffer.len().min(self.unread.len());
        buffer[..count].copy_from_slice(&self.unread[..count]);
        self.unread.drain(..count);
        Ok(count)
    }
}

/// The path of the named pipe for a socket setting: a bare name such as `mpvsocket` is the pipe
/// `\\.\pipe\mpvsocket`; a full path is used as it is.
pub fn pipe_path(socket: &Path) -> PathBuf {
    let text = socket.to_string_lossy();
    if text.starts_with(r"\\") || text.contains('/') || text.contains('\\') {
        return socket.to_path_buf();
    }
    PathBuf::from(format!(r"\\.\pipe\{text}"))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::mpsc::Sender;

    use super::*;

    const SHORT: Duration = Duration::from_millis(50);
    const LONG: Duration = Duration::from_secs(3);

    /// A reader that gives what it is sent, one piece per read, and waits for more.
    struct Fed(Receiver<io::Result<Vec<u8>>>);

    impl Read for Fed {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            match self.0.recv() {
                Ok(Ok(bytes)) => {
                    buffer[..bytes.len()].copy_from_slice(&bytes);
                    Ok(bytes.len())
                }
                Ok(Err(error)) => Err(error),
                Err(_) => Ok(0),
            }
        }
    }

    fn fed() -> (Sender<io::Result<Vec<u8>>>, Fed) {
        let (sender, receiver) = mpsc::channel();
        (sender, Fed(receiver))
    }

    #[test]
    fn it_gives_what_the_reader_gives_and_then_the_end() {
        let mut reader = TimedReader::spawn(Cursor::new(b"hello\nworld\n".to_vec()), LONG);
        let mut all = Vec::new();

        let mut buffer = [0_u8; 64];
        loop {
            let read = reader.read(&mut buffer).expect("read");
            if read == 0 {
                break;
            }
            all.extend_from_slice(&buffer[..read]);
        }

        assert_eq!(all, b"hello\nworld\n");
    }

    #[test]
    fn a_chunk_bigger_than_the_buffer_is_given_in_pieces() {
        let mut reader = TimedReader::spawn(Cursor::new(b"abcdefgh".to_vec()), LONG);
        let mut small = [0_u8; 3];

        let first = reader.read(&mut small).expect("first");
        assert_eq!(&small[..first], b"abc");
        let second = reader.read(&mut small).expect("second");
        assert_eq!(&small[..second], b"def");
        let third = reader.read(&mut small).expect("third");
        assert_eq!(&small[..third], b"gh");
        assert_eq!(reader.read(&mut small).expect("end"), 0);
    }

    #[test]
    fn it_times_out_while_nothing_arrives_and_goes_on_when_something_does() {
        let (sender, source) = fed();
        let mut reader = TimedReader::spawn(source, SHORT);
        let mut buffer = [0_u8; 16];

        let waiting = reader.read(&mut buffer).expect_err("must time out");
        assert_eq!(waiting.kind(), ErrorKind::TimedOut);

        sender.send(Ok(b"late".to_vec())).expect("send");
        let mut got = Vec::new();
        for _ in 0..60 {
            match reader.read(&mut buffer) {
                Ok(read) => {
                    got.extend_from_slice(&buffer[..read]);
                    break;
                }
                Err(error) if error.kind() == ErrorKind::TimedOut => {}
                Err(error) => panic!("unexpected error: {error}"),
            }
        }
        assert_eq!(got, b"late");
    }

    #[test]
    fn an_error_of_the_reader_is_passed_on_once_and_then_the_stream_is_over() {
        let (sender, source) = fed();
        let mut reader = TimedReader::spawn(source, LONG);
        let mut buffer = [0_u8; 16];
        sender
            .send(Err(io::Error::new(ErrorKind::BrokenPipe, "mpv closed")))
            .expect("send");

        let error = reader.read(&mut buffer).expect_err("must fail");

        assert_eq!(error.kind(), ErrorKind::BrokenPipe);
        assert_eq!(reader.read(&mut buffer).expect("over"), 0);
    }

    #[test]
    fn a_closed_reader_is_the_end_of_the_stream() {
        let (sender, source) = fed();
        let mut reader = TimedReader::spawn(source, LONG);
        drop(sender);

        assert_eq!(reader.read(&mut [0_u8; 4]).expect("end"), 0);
    }

    #[test]
    fn a_bare_name_is_a_named_pipe_and_a_path_is_kept() {
        assert_eq!(
            pipe_path(Path::new("mpvsocket")),
            PathBuf::from(r"\\.\pipe\mpvsocket")
        );
        assert_eq!(
            pipe_path(Path::new(r"\\.\pipe\other")),
            PathBuf::from(r"\\.\pipe\other")
        );
        assert_eq!(
            pipe_path(Path::new("/tmp/mpvsocket")),
            PathBuf::from("/tmp/mpvsocket")
        );
        assert_eq!(
            pipe_path(Path::new(r"C:\x\sock")),
            PathBuf::from(r"C:\x\sock")
        );
    }
}
