//! Sentence audio for cards. The user pauses the video before looking a word up, so the sound has
//! to be recorded beforehand: the default output (its monitor) is recorded all the time into a
//! circular buffer and the last spoken seconds are cut out when a card is made.

use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use thiserror::Error;

use crate::cards::MediaFile;

/// Mono, 16 kHz, signed 16-bit: plenty for speech and small (32 KB per second).
pub const SAMPLE_RATE: u32 = 16_000;
/// How much sound is kept, so a long pause does not lose the sentence that was just heard.
pub const BUFFER_SECONDS: usize = 60;
/// Samples below this level (of 32768) count as silence.
pub const SILENCE_LEVEL: i16 = 300;
/// Pauses shorter than this are part of the sentence, not its end.
pub const SILENCE_GAP_MS: usize = 600;
/// A little sound is kept around the speech so words are not cut.
pub const PADDING_MS: usize = 150;
const CHUNK_BYTES: usize = 3200;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("cannot start the recorder `{program}`: {reason}")]
    Spawn { program: String, reason: String },
}

/// The most recent sound, oldest first.
#[derive(Debug)]
pub struct AudioBuffer {
    samples: VecDeque<i16>,
    capacity: usize,
}

pub type SharedAudio = Arc<Mutex<AudioBuffer>>;

impl AudioBuffer {
    pub fn new(seconds: usize) -> AudioBuffer {
        AudioBuffer {
            samples: VecDeque::new(),
            capacity: seconds * SAMPLE_RATE as usize,
        }
    }

    pub fn shared(seconds: usize) -> SharedAudio {
        Arc::new(Mutex::new(AudioBuffer::new(seconds)))
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn push(&mut self, samples: &[i16]) {
        self.samples.extend(samples.iter().copied());
        while self.samples.len() > self.capacity {
            self.samples.pop_front();
        }
    }

    /// The last stretch of speech, at most `max_seconds` long. The silence at the end (the video
    /// is paused) is skipped and the clip starts after the last pause longer than
    /// [`SILENCE_GAP_MS`]. `None` when there was no sound at all.
    pub fn clip(&self, max_seconds: usize) -> Option<Vec<i16>> {
        let samples: Vec<i16> = self.samples.iter().copied().collect();
        let end = samples.iter().rposition(|sample| is_sound(*sample))? + 1;
        let max_len = max_seconds * SAMPLE_RATE as usize;
        let gap = SILENCE_GAP_MS * SAMPLE_RATE as usize / 1000;
        let padding = PADDING_MS * SAMPLE_RATE as usize / 1000;

        let floor = end.saturating_sub(max_len);
        let mut start = end;
        let mut quiet = 0;
        let mut index = end;
        while index > floor {
            index -= 1;
            if is_sound(samples[index]) {
                start = index;
                quiet = 0;
            } else {
                quiet += 1;
                if quiet >= gap {
                    break;
                }
            }
        }
        let start = start.saturating_sub(padding).max(floor);
        let end = (end + padding).min(samples.len());
        Some(samples[start..end].to_vec())
    }
}

fn is_sound(sample: i16) -> bool {
    sample.unsigned_abs() >= SILENCE_LEVEL.unsigned_abs()
}

/// The sound as a mono WAV file (Anki plays it everywhere and no encoder library is needed).
pub fn encode_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + samples.len() * 2);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

pub fn audio_file(samples: &[i16], timestamp_ms: u64) -> MediaFile {
    MediaFile {
        filename: format!("anki-mining-{timestamp_ms}.wav"),
        data: STANDARD.encode(encode_wav(samples, SAMPLE_RATE)),
    }
}

/// Reads little-endian 16-bit samples from `reader` into the buffer until it ends.
pub fn feed_from_reader(reader: &mut dyn Read, buffer: &SharedAudio) {
    let mut bytes = [0_u8; CHUNK_BYTES];
    let mut pending: Option<u8> = None;
    loop {
        let read = match reader.read(&mut bytes) {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        let mut samples = Vec::with_capacity(read / 2 + 1);
        let mut chunk = &bytes[..read];
        if let Some(low) = pending.take() {
            samples.push(i16::from_le_bytes([low, chunk[0]]));
            chunk = &chunk[1..];
        }
        let (pairs, remainder) = chunk.as_chunks::<2>();
        if let [low] = remainder {
            pending = Some(*low);
        }
        samples.extend(pairs.iter().map(|pair| i16::from_le_bytes(*pair)));
        if let Ok(mut buffer) = buffer.lock() {
            buffer.push(&samples);
        }
    }
}

/// The command that records what the computer plays (PulseAudio and PipeWire's Pulse layer).
pub fn recorder_command() -> Command {
    let mut command = Command::new("parec");
    command
        .args([
            "--device=@DEFAULT_MONITOR@",
            "--format=s16le",
            "--channels=1",
            &format!("--rate={SAMPLE_RATE}"),
            "--latency-msec=100",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    command
}

/// Keeps the recorder running and the buffer filled; the recorder stops with this value.
pub struct AudioRecorder {
    child: Child,
}

impl AudioRecorder {
    pub fn start(buffer: SharedAudio) -> Result<AudioRecorder, AudioError> {
        AudioRecorder::start_with(recorder_command(), buffer)
    }

    pub fn start_with(
        mut command: Command,
        buffer: SharedAudio,
    ) -> Result<AudioRecorder, AudioError> {
        let program = command.get_program().to_string_lossy().into_owned();
        let mut child = command.spawn().map_err(|error| AudioError::Spawn {
            program,
            reason: error.to_string(),
        })?;
        if let Some(mut stdout) = child.stdout.take() {
            thread::spawn(move || {
                feed_from_reader(&mut stdout, &buffer);
            });
        }
        Ok(AudioRecorder { child })
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const LOUD: i16 = 5000;

    fn seconds(count: f32) -> usize {
        (count * SAMPLE_RATE as f32) as usize
    }

    fn sound(count: f32) -> Vec<i16> {
        vec![LOUD; seconds(count)]
    }

    fn silence(count: f32) -> Vec<i16> {
        vec![0; seconds(count)]
    }

    fn buffer_with(parts: &[Vec<i16>]) -> AudioBuffer {
        let mut buffer = AudioBuffer::new(BUFFER_SECONDS);
        for part in parts {
            buffer.push(part);
        }
        buffer
    }

    #[test]
    fn an_empty_buffer_has_no_clip() {
        let buffer = AudioBuffer::new(5);

        assert!(buffer.is_empty());
        assert_eq!(buffer.clip(8), None);
    }

    #[test]
    fn a_buffer_of_silence_has_no_clip() {
        let buffer = buffer_with(&[silence(3.0)]);

        assert_eq!(buffer.clip(8), None);
    }

    #[test]
    fn the_buffer_forgets_the_oldest_sound_beyond_its_capacity() {
        let mut buffer = AudioBuffer::new(1);
        buffer.push(&vec![1; SAMPLE_RATE as usize]);
        buffer.push(&[2, 3]);

        assert_eq!(buffer.len(), SAMPLE_RATE as usize);
        assert_eq!(buffer.samples.back(), Some(&3));
        assert_eq!(buffer.samples.front(), Some(&1));
        assert_eq!(
            buffer.samples.iter().filter(|sample| **sample == 1).count(),
            SAMPLE_RATE as usize - 2
        );
    }

    #[test]
    fn silence_after_the_speech_is_skipped() {
        let buffer = buffer_with(&[sound(2.0), silence(20.0)]);

        let clip = buffer.clip(8).expect("clip");

        let padding = PADDING_MS * SAMPLE_RATE as usize / 1000;
        assert_eq!(clip.len(), seconds(2.0) + padding);
        assert!(clip[..seconds(2.0)].iter().all(|sample| *sample == LOUD));
        assert!(clip[seconds(2.0)..].iter().all(|sample| *sample == 0));
    }

    #[test]
    fn the_clip_starts_after_the_last_long_pause() {
        let buffer = buffer_with(&[sound(3.0), silence(1.5), sound(2.0), silence(5.0)]);

        let clip = buffer.clip(30).expect("clip");

        let padding = PADDING_MS * SAMPLE_RATE as usize / 1000;
        assert_eq!(clip.len(), padding + seconds(2.0) + padding);
        assert_eq!(clip[padding], LOUD);
    }

    #[test]
    fn short_pauses_stay_inside_the_clip() {
        let buffer = buffer_with(&[silence(4.0), sound(1.0), silence(0.3), sound(1.0)]);

        let clip = buffer.clip(30).expect("clip");

        let padding = PADDING_MS * SAMPLE_RATE as usize / 1000;
        assert_eq!(clip.len(), padding + seconds(2.3));
    }

    #[test]
    fn the_clip_is_never_longer_than_the_limit() {
        let buffer = buffer_with(&[sound(20.0), silence(3.0)]);

        let clip = buffer.clip(5).expect("clip");

        assert_eq!(
            clip.len(),
            seconds(5.0) + PADDING_MS * SAMPLE_RATE as usize / 1000
        );
    }

    #[test]
    fn negative_samples_count_as_sound() {
        let buffer = buffer_with(&[silence(1.0), vec![-LOUD; 800], silence(1.0)]);

        assert!(buffer.clip(8).is_some());
    }

    #[test]
    fn the_wav_header_describes_mono_16_bit_samples() {
        let wav = encode_wav(&[1, -2, 3], 16_000);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 36 + 6);
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wav[16..20].try_into().unwrap()), 16);
        assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 32_000);
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
        assert_eq!(&wav[44..], &[1, 0, 254, 255, 3, 0]);
    }

    #[test]
    fn the_audio_file_is_a_named_base64_wav() {
        let file = audio_file(&[7, 8], 1234);

        assert_eq!(file.filename, "anki-mining-1234.wav");
        assert_eq!(
            STANDARD.decode(file.data).expect("base64"),
            encode_wav(&[7, 8], SAMPLE_RATE)
        );
    }

    #[test]
    fn samples_split_across_reads_are_put_back_together() {
        struct OneByteAtATime(Cursor<Vec<u8>>);
        impl Read for OneByteAtATime {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.0.read(&mut buffer[..1])
            }
        }
        let bytes: Vec<u8> = [1_i16, -2, 300, -32768]
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        let buffer = AudioBuffer::shared(1);

        feed_from_reader(&mut OneByteAtATime(Cursor::new(bytes)), &buffer);

        let stored: Vec<i16> = buffer.lock().unwrap().samples.iter().copied().collect();
        assert_eq!(stored, vec![1, -2, 300, -32768]);
    }

    #[test]
    fn a_trailing_odd_byte_is_ignored() {
        let buffer = AudioBuffer::shared(1);

        feed_from_reader(&mut Cursor::new(vec![5, 0, 9]), &buffer);

        let stored: Vec<i16> = buffer.lock().unwrap().samples.iter().copied().collect();
        assert_eq!(stored, vec![5]);
    }

    #[test]
    fn the_recorder_command_records_the_default_output_as_mono_16_khz() {
        let command = recorder_command();

        assert_eq!(command.get_program(), "parec");
        let arguments: Vec<_> = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            arguments,
            vec![
                "--device=@DEFAULT_MONITOR@",
                "--format=s16le",
                "--channels=1",
                "--rate=16000",
                "--latency-msec=100"
            ]
        );
    }

    #[test]
    fn a_missing_recorder_is_reported_with_its_name() {
        let result = AudioRecorder::start_with(
            Command::new("anki-mining-no-such-recorder"),
            AudioBuffer::shared(1),
        );

        match result {
            Err(AudioError::Spawn { program, .. }) => {
                assert_eq!(program, "anki-mining-no-such-recorder");
            }
            Ok(_) => panic!("the recorder cannot exist"),
        }
    }

    #[test]
    fn the_recorder_fills_the_buffer_from_the_program_output() {
        let mut command = Command::new("printf");
        command.arg("\\001\\000\\002\\000").stdout(Stdio::piped());
        let buffer = AudioBuffer::shared(1);

        let recorder = AudioRecorder::start_with(command, buffer.clone()).expect("start");
        for _ in 0..100 {
            if buffer.lock().unwrap().len() == 2 {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(20));
        }
        drop(recorder);

        let stored: Vec<i16> = buffer.lock().unwrap().samples.iter().copied().collect();
        assert_eq!(stored, vec![1, 2]);
    }
}
