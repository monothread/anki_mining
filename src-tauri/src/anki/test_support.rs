//! A tiny AnkiConnect stand-in for tests: scripted replies, recorded request bodies.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};

/// Answers each HTTP request with the next scripted reply and records the JSON bodies.
pub(crate) struct MockAnki {
    pub url: String,
    requests: Arc<Mutex<Vec<Value>>>,
}

pub(crate) enum Reply {
    Body(String),
    Status(u16),
}

impl MockAnki {
    pub fn start(replies: Vec<Reply>) -> MockAnki {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let mut replies: VecDeque<Reply> = replies.into();

        thread::spawn(move || {
            while let Some(reply) = replies.pop_front() {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                if let Some(body) = read_request_body(&stream) {
                    recorded.lock().expect("lock").push(body);
                }
                write_reply(stream, &reply);
            }
        });
        MockAnki { url, requests }
    }

    pub fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("lock").clone()
    }
}

pub(crate) fn read_request_body(stream: &TcpStream) -> Option<Value> {
    let mut reader = BufReader::new(stream);
    let mut content_length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().ok()?;
            }
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

pub(crate) fn write_reply(mut stream: TcpStream, reply: &Reply) {
    let (status, body) = match reply {
        Reply::Body(body) => ("200 OK".to_string(), body.clone()),
        Reply::Status(code) => (format!("{code} Error"), String::new()),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

pub(crate) fn ok(result: Value) -> Reply {
    Reply::Body(json!({ "result": result, "error": null }).to_string())
}

pub(crate) fn failure(message: &str) -> Reply {
    Reply::Body(json!({ "result": null, "error": message }).to_string())
}
