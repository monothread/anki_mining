//! Spoken examples for cards: the word and the sentence read by a neural Japanese voice
//! (VOICEVOX), made offline by a helper program that runs next to the app.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use thiserror::Error;

use crate::cards::MediaFile;

/// File name of the helper inside the TTS resources folder.
#[cfg(unix)]
pub const HELPER: &str = "voicevox_say";
/// File name of the helper inside the TTS resources folder.
#[cfg(windows)]
pub const HELPER: &str = "voicevox_say.exe";
/// The voice used when none is chosen: 春日部つむぎ (ノーマル).
pub const DEFAULT_STYLE_ID: u32 = 8;
/// Credit the VOICEVOX terms of use ask for.
pub const CREDIT: &str = "VOICEVOX:春日部つむぎ";
/// Longer texts are cut: a card does not need a paragraph read aloud.
pub const MAX_CHARS: usize = 200;
/// A synthesis that takes longer than this is given up.
pub const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TtsError {
    #[error("the speech helper `{0}` is missing (run scripts/fetch-tts.sh)")]
    HelperMissing(String),
    #[error("cannot run the speech helper: {0}")]
    Spawn(String),
    #[error("the speech helper failed: {0}")]
    Failed(String),
    #[error("the speech helper took longer than {0} seconds")]
    TimedOut(u64),
    #[error("the speech helper did not produce a sound file")]
    NoSound,
    #[error("there is nothing to say")]
    EmptyText,
    #[error("no program to play sound was found (on Linux: paplay, pw-play or aplay)")]
    NoPlayer,
}

/// Something that turns a text into speech (a WAV file).
pub trait Speaker: Send + Sync {
    fn speak(&self, text: &str) -> Result<Vec<u8>, TtsError>;
}

/// Speaks with the VOICEVOX helper found in `directory`.
pub struct VoicevoxSpeaker {
    directory: PathBuf,
    style_id: u32,
    timeout: Duration,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl VoicevoxSpeaker {
    pub fn new(directory: &Path, style_id: u32) -> Result<VoicevoxSpeaker, TtsError> {
        VoicevoxSpeaker::with_timeout(directory, style_id, TIMEOUT)
    }

    pub fn with_timeout(
        directory: &Path,
        style_id: u32,
        timeout: Duration,
    ) -> Result<VoicevoxSpeaker, TtsError> {
        let helper = directory.join(HELPER);
        if !helper.is_file() {
            return Err(TtsError::HelperMissing(helper.display().to_string()));
        }
        Ok(VoicevoxSpeaker {
            directory: directory.to_path_buf(),
            style_id,
            timeout,
        })
    }

    fn command(&self, text: &str, output: &Path) -> Command {
        let mut command = Command::new(self.directory.join(HELPER));
        command
            .arg(&self.directory)
            .arg(self.style_id.to_string())
            .arg(text)
            .arg(output)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        hide_console_window(&mut command);
        command
    }

    fn output_path() -> PathBuf {
        let number = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "anki-mining-speech-{}-{number}.wav",
            std::process::id()
        ))
    }
}

impl Speaker for VoicevoxSpeaker {
    fn speak(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let text = clip_text(text);
        if text.is_empty() {
            return Err(TtsError::EmptyText);
        }
        let output = VoicevoxSpeaker::output_path();
        let result = self.run(&text, &output);
        let _ = std::fs::remove_file(&output);
        result
    }
}

impl VoicevoxSpeaker {
    fn run(&self, text: &str, output: &Path) -> Result<Vec<u8>, TtsError> {
        let mut child = self
            .command(text, output)
            .spawn()
            .map_err(|error| TtsError::Spawn(error.to_string()))?;
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => return Err(TtsError::Spawn(error.to_string())),
            }
            if started.elapsed() > self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(TtsError::TimedOut(self.timeout.as_secs()));
            }
            thread::sleep(Duration::from_millis(20));
        };
        if !status.success() {
            let mut message = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut stderr, &mut message);
            }
            return Err(TtsError::Failed(last_line(&message)));
        }
        let wav = std::fs::read(output).map_err(|_| TtsError::NoSound)?;
        if wav.len() > 44 && wav.starts_with(b"RIFF") {
            Ok(wav)
        } else {
            Err(TtsError::NoSound)
        }
    }
}

/// Keeps Windows from flashing a console window when a helper program is started.
#[cfg(windows)]
pub fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// Keeps Windows from flashing a console window when a helper program is started (nothing to do
/// on other systems).
#[cfg(not(windows))]
pub fn hide_console_window(_command: &mut Command) {}

/// Programs that can play a WAV file, in order of preference, with the arguments they need.
#[cfg(unix)]
const PLAYERS: [(&str, &[&str]); 3] = [("paplay", &[]), ("pw-play", &[]), ("aplay", &["-q"])];

/// The first player found in the directories of `path` (a `PATH`-style list).
#[cfg(unix)]
pub fn find_player(path: &std::ffi::OsStr) -> Option<(PathBuf, &'static [&'static str])> {
    let directories: Vec<PathBuf> = std::env::split_paths(path).collect();
    PLAYERS.iter().find_map(|(name, arguments)| {
        directories
            .iter()
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
            .map(|found| (found, *arguments))
    })
}

/// Plays a WAV on the speakers without waiting for it to end.
#[cfg(unix)]
pub fn play_wav(wav: &[u8]) -> Result<(), TtsError> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let (player, arguments) = find_player(&path).ok_or(TtsError::NoPlayer)?;
    let file = VoicevoxSpeaker::output_path();
    std::fs::write(&file, wav).map_err(|error| TtsError::Spawn(error.to_string()))?;
    let mut child = Command::new(player)
        .args(arguments)
        .arg(&file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            let _ = std::fs::remove_file(&file);
            TtsError::Spawn(error.to_string())
        })?;
    thread::spawn(move || {
        let _ = child.wait();
        let _ = std::fs::remove_file(&file);
    });
    Ok(())
}

/// Plays a WAV on the speakers without waiting for it to end.
#[cfg(windows)]
pub fn play_wav(wav: &[u8]) -> Result<(), TtsError> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_FILENAME, SND_SYNC};

    let file = VoicevoxSpeaker::output_path();
    std::fs::write(&file, wav).map_err(|error| TtsError::Spawn(error.to_string()))?;
    thread::spawn(move || {
        let path: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `path` is a NUL-terminated wide string that outlives the call, which waits for
        // the sound to end (SND_SYNC) before the file is removed.
        unsafe {
            PlaySoundW(path.as_ptr(), null_mut(), SND_FILENAME | SND_SYNC);
        }
        let _ = std::fs::remove_file(&file);
    });
    Ok(())
}

/// The text without surrounding spaces, cut to [`MAX_CHARS`] characters.
pub fn clip_text(text: &str) -> String {
    text.trim().chars().take(MAX_CHARS).collect()
}

fn last_line(message: &str) -> String {
    message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown error")
        .trim()
        .to_string()
}

/// A named base64 WAV for a card: `anki-mining-<kind>-<timestamp>.wav`.
pub fn speech_file(wav: &[u8], kind: &str, timestamp_ms: u64) -> MediaFile {
    MediaFile {
        filename: format!("anki-mining-{kind}-{timestamp_ms}.wav"),
        data: STANDARD.encode(wav),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    /// A helper script that behaves like `voicevox_say`: `<dir> <style> <text> <output>`.
    fn fake_helper(directory: &TempDir, body: &str) {
        let path = directory.path().join(HELPER);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("mode");
    }

    const WRITES_A_WAV: &str =
        "printf 'RIFF0000WAVEfmt xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' > \"$4\"";

    #[test]
    fn a_missing_helper_is_reported_with_its_path() {
        let directory = tempfile::tempdir().expect("dir");

        let error = VoicevoxSpeaker::new(directory.path(), 8)
            .err()
            .expect("must fail");

        assert_eq!(
            error,
            TtsError::HelperMissing(directory.path().join(HELPER).display().to_string())
        );
    }

    #[test]
    fn the_helper_gets_the_folder_the_style_the_text_and_an_output_file() {
        let directory = tempfile::tempdir().expect("dir");
        fake_helper(
            &directory,
            &format!("echo \"$1|$2|$3\" > \"$(dirname \"$4\")/args-$$.txt\"; {WRITES_A_WAV}"),
        );
        let speaker = VoicevoxSpeaker::new(directory.path(), 10).expect("speaker");

        let wav = speaker.speak("  猫が好き  ").expect("speech");

        assert!(wav.starts_with(b"RIFF"));
        let recorded: Vec<String> = std::fs::read_dir(std::env::temp_dir())
            .expect("temp")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("args-"))
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .collect();
        let expected = format!("{}|10|猫が好き\n", directory.path().display());
        assert!(recorded.contains(&expected), "{recorded:?}");
        for entry in std::fs::read_dir(std::env::temp_dir())
            .expect("temp")
            .flatten()
        {
            if entry.file_name().to_string_lossy().starts_with("args-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    #[test]
    fn the_sound_file_is_removed_after_it_is_read() {
        let directory = tempfile::tempdir().expect("dir");
        fake_helper(
            &directory,
            &format!(
                "{WRITES_A_WAV}; echo \"$4\" > \"{}/last\"",
                directory.path().display()
            ),
        );
        let speaker = VoicevoxSpeaker::new(directory.path(), 8).expect("speaker");

        speaker.speak("猫").expect("speech");

        let output = std::fs::read_to_string(directory.path().join("last")).expect("last");
        assert!(!Path::new(output.trim()).exists());
    }

    #[test]
    fn a_failing_helper_reports_its_last_error_line() {
        let directory = tempfile::tempdir().expect("dir");
        fake_helper(
            &directory,
            "echo 'loading' >&2; echo 'tts: model not loaded' >&2; exit 1",
        );
        let speaker = VoicevoxSpeaker::new(directory.path(), 8).expect("speaker");

        let error = speaker.speak("猫").expect_err("must fail");

        assert_eq!(error, TtsError::Failed("tts: model not loaded".to_string()));
    }

    #[test]
    fn a_helper_that_writes_nothing_or_not_a_wav_is_refused() {
        let directory = tempfile::tempdir().expect("dir");
        let speaker = VoicevoxSpeaker::new(directory.path(), 8);
        assert!(speaker.is_err());

        fake_helper(&directory, "true");
        let speaker = VoicevoxSpeaker::new(directory.path(), 8).expect("speaker");
        assert_eq!(speaker.speak("猫").expect_err("no file"), TtsError::NoSound);

        fake_helper(&directory, "echo not-a-wav > \"$4\"");
        assert_eq!(speaker.speak("猫").expect_err("not wav"), TtsError::NoSound);
    }

    #[test]
    fn a_helper_that_takes_too_long_is_stopped() {
        let directory = tempfile::tempdir().expect("dir");
        fake_helper(&directory, "sleep 5");
        let speaker =
            VoicevoxSpeaker::with_timeout(directory.path(), 8, Duration::from_millis(200))
                .expect("speaker");
        let started = Instant::now();

        let error = speaker.speak("猫").expect_err("must fail");

        assert_eq!(error, TtsError::TimedOut(0));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn an_empty_text_is_not_spoken() {
        let directory = tempfile::tempdir().expect("dir");
        fake_helper(&directory, WRITES_A_WAV);
        let speaker = VoicevoxSpeaker::new(directory.path(), 8).expect("speaker");

        assert_eq!(
            speaker.speak("   ").expect_err("empty"),
            TtsError::EmptyText
        );
        assert_eq!(speaker.speak("").expect_err("empty"), TtsError::EmptyText);
    }

    #[test]
    fn long_texts_are_cut_to_the_limit_by_characters() {
        let long = "あ".repeat(MAX_CHARS + 50);

        assert_eq!(clip_text(&long).chars().count(), MAX_CHARS);
        assert_eq!(clip_text("  猫  "), "猫");
    }

    #[test]
    fn the_speech_file_has_a_kind_a_timestamp_and_base64_contents() {
        let file = speech_file(b"RIFFdata", "word", 1234);

        assert_eq!(file.filename, "anki-mining-word-1234.wav");
        assert_eq!(STANDARD.decode(file.data).expect("base64"), b"RIFFdata");
    }

    #[test]
    fn the_credit_names_the_voice() {
        assert_eq!(CREDIT, "VOICEVOX:春日部つむぎ");
        assert_eq!(DEFAULT_STYLE_ID, 8);
    }

    #[test]
    fn the_first_player_found_is_chosen_in_order_of_preference() {
        let first = tempfile::tempdir().expect("dir");
        let second = tempfile::tempdir().expect("dir");
        std::fs::write(first.path().join("aplay"), "").expect("aplay");
        std::fs::write(second.path().join("pw-play"), "").expect("pw-play");
        let path = std::env::join_paths([first.path(), second.path()]).expect("path");

        let (found, arguments) = find_player(&path).expect("player");

        assert_eq!(found, second.path().join("pw-play"));
        assert!(arguments.is_empty());
    }

    #[test]
    fn aplay_is_run_quietly() {
        let directory = tempfile::tempdir().expect("dir");
        std::fs::write(directory.path().join("aplay"), "").expect("aplay");
        let path = std::env::join_paths([directory.path()]).expect("path");

        let (found, arguments) = find_player(&path).expect("player");

        assert_eq!(found, directory.path().join("aplay"));
        assert_eq!(arguments, ["-q"]);
    }

    #[test]
    fn no_player_is_none() {
        let directory = tempfile::tempdir().expect("dir");
        let path = std::env::join_paths([directory.path()]).expect("path");

        assert!(find_player(&path).is_none());
        assert!(find_player(std::ffi::OsStr::new("")).is_none());
    }
}
