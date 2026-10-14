//! Prints the subtitles of a running mpv for a few seconds.
//!
//! Usage: mpv_listen <socket path> <seconds>
//! Start mpv with `--input-ipc-server=<socket path>` first.

use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anki_mining_lib::sources::mpv::MpvSource;
use anki_mining_lib::types::text_source::TextSource;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [socket, seconds] = arguments.as_slice() else {
        eprintln!("usage: mpv_listen <socket path> <seconds>");
        return ExitCode::from(2);
    };
    let Ok(seconds) = seconds.parse::<u64>() else {
        eprintln!("seconds must be a number");
        return ExitCode::from(2);
    };

    let (sender, receiver) = mpsc::channel();
    let mut source =
        MpvSource::with_reconnect_interval(Path::new(socket), Duration::from_millis(200));
    if let Err(error) = source.start(sender) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut count = 0;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if let Ok(event) = receiver.recv_timeout(left) {
            count += 1;
            println!(
                "subtitle {count}: {:?} (title: {:?}, connected: {})",
                event.text,
                event.window_info.map(|info| info.title),
                source.connection().is_connected()
            );
        }
    }
    let _ = source.stop();
    println!("{count} subtitles");
    ExitCode::SUCCESS
}
