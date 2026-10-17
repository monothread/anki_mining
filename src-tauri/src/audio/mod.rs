//! Sentence audio for cards. The user pauses the video before looking a word up, so the sound has
//! to be recorded beforehand: the default output (its monitor) is recorded all the time into a
//! circular buffer and the last spoken seconds are cut out when a card is made. On Linux the
//! recorder is `parec` (PulseAudio and PipeWire); on Windows the default output is captured with
//! WASAPI loopback.

use std::collections::VecDeque;
use std::io::Read;
#[cfg(unix)]
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::thread;
use std::time::Instant;

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
    /// How many samples were ever pushed (the position just after the newest one).
    total: u64,
    /// When the newest samples were pushed, to turn a point in time into a position.
    last_push: Option<Instant>,
}

pub type SharedAudio = Arc<Mutex<AudioBuffer>>;

impl AudioBuffer {
    pub fn new(seconds: usize) -> AudioBuffer {
        AudioBuffer {
            samples: VecDeque::new(),
            capacity: seconds * SAMPLE_RATE as usize,
            total: 0,
            last_push: None,
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
        self.push_at(samples, Instant::now());
    }

    /// Adds samples that arrived at `at` (the newest sample is taken as heard at that moment).
    pub fn push_at(&mut self, samples: &[i16], at: Instant) {
        self.total += samples.len() as u64;
        self.last_push = Some(at);
        self.samples.extend(samples.iter().copied());
        while self.samples.len() > self.capacity {
            self.samples.pop_front();
        }
    }

    /// The sound heard between `start` and `end` (with a little around it), at most
    /// `max_seconds` long. `None` when that time is no longer in the buffer or was silent.
    pub fn clip_between(
        &self,
        start: Instant,
        end: Instant,
        max_seconds: usize,
    ) -> Option<Vec<i16>> {
        let newest = self.last_push?;
        let position = |at: Instant| -> i64 {
            let behind = if newest >= at {
                -(newest.duration_since(at).as_secs_f64())
            } else {
                at.duration_since(newest).as_secs_f64()
            };
            self.samples.len() as i64 + (behind * f64::from(SAMPLE_RATE)).round() as i64
        };
        let padding = (PADDING_MS * SAMPLE_RATE as usize / 1000) as i64;
        let length = self.samples.len() as i64;
        let from = (position(start) - padding).clamp(0, length);
        let mut to = (position(end) + padding).clamp(0, length);
        to = to.min(from + (max_seconds * SAMPLE_RATE as usize) as i64);
        if from >= to {
            return None;
        }
        let clip: Vec<i16> = self
            .samples
            .iter()
            .skip(from as usize)
            .take((to - from) as usize)
            .copied()
            .collect();
        clip.iter().any(|sample| is_sound(*sample)).then_some(clip)
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

/// Turns interleaved samples of any channel count and rate (what a sound card gives) into the mono
/// 16 kHz samples of the buffer, keeping what it needs between calls so chunks join smoothly.
pub struct MonoResampler {
    channels: usize,
    /// How many input samples go by for each output sample.
    step: f64,
    /// Where the next output sample is, counted from the start of the next chunk (the last sample
    /// of the previous chunk is at -1).
    position: f64,
    last: f32,
}

impl MonoResampler {
    pub fn new(channels: usize, input_rate: u32) -> MonoResampler {
        MonoResampler {
            channels: channels.max(1),
            step: f64::from(input_rate.max(1)) / f64::from(SAMPLE_RATE),
            position: 0.0,
            last: 0.0,
        }
    }

    /// The mono 16 kHz samples that `interleaved` (frames of `channels` samples in -1.0..=1.0) adds.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<i16> {
        let mono: Vec<f32> = interleaved
            .chunks_exact(self.channels)
            .map(|frame| frame.iter().sum::<f32>() / self.channels as f32)
            .collect();
        let Some(newest) = mono.last().copied() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity((mono.len() as f64 / self.step) as usize + 1);
        while self.position < (mono.len() - 1) as f64 {
            let index = self.position.floor();
            let before = if index < 0.0 {
                self.last
            } else {
                mono[index as usize]
            };
            let after = mono[(index + 1.0) as usize];
            let fraction = (self.position - index) as f32;
            let value = before + (after - before) * fraction;
            out.push((value * 32767.0).round().clamp(-32768.0, 32767.0) as i16);
            self.position += self.step;
        }
        self.position -= mono.len() as f64;
        self.last = newest;
        out
    }
}

/// The command that records what the computer plays (PulseAudio and PipeWire's Pulse layer).
#[cfg(unix)]
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
#[cfg(unix)]
pub struct AudioRecorder {
    child: Child,
}

#[cfg(unix)]
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

#[cfg(unix)]
impl Drop for AudioRecorder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(windows)]
mod loopback;
#[cfg(windows)]
pub use loopback::AudioRecorder;

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

    #[cfg(unix)]
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

    #[cfg(unix)]
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

    #[cfg(unix)]
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

    fn timed_buffer() -> (AudioBuffer, Instant) {
        // 10 s of sound that ended at `now`: silence 0-4 s, speech 4-6 s, silence 6-10 s.
        let mut buffer = AudioBuffer::new(BUFFER_SECONDS);
        let now = Instant::now();
        let mut samples = silence(4.0);
        samples.extend(sound(2.0));
        samples.extend(silence(4.0));
        buffer.push_at(&samples, now);
        (buffer, now)
    }

    #[test]
    fn a_clip_between_two_moments_is_the_sound_heard_then() {
        let (buffer, now) = timed_buffer();
        let padding = PADDING_MS * SAMPLE_RATE as usize / 1000;

        let clip = buffer
            .clip_between(
                now - std::time::Duration::from_secs(6),
                now - std::time::Duration::from_secs(4),
                30,
            )
            .expect("clip");

        assert_eq!(clip.len(), seconds(2.0) + 2 * padding);
        assert!(clip[padding..padding + seconds(2.0)]
            .iter()
            .all(|sample| *sample == LOUD));
        assert_eq!(clip[0], 0);
    }

    #[test]
    fn a_clip_between_two_moments_of_silence_is_nothing() {
        let (buffer, now) = timed_buffer();

        let clip = buffer.clip_between(
            now - std::time::Duration::from_secs(3),
            now - std::time::Duration::from_secs(1),
            30,
        );

        assert_eq!(clip, None);
    }

    #[test]
    fn moments_that_the_buffer_no_longer_holds_give_no_clip() {
        let (buffer, now) = timed_buffer();

        let clip = buffer.clip_between(
            now - std::time::Duration::from_secs(40),
            now - std::time::Duration::from_secs(30),
            30,
        );

        assert_eq!(clip, None);
    }

    #[test]
    fn a_clip_may_end_slightly_after_the_newest_sound() {
        let mut buffer = AudioBuffer::new(BUFFER_SECONDS);
        let now = Instant::now();
        buffer.push_at(&sound(2.0), now);

        let clip = buffer
            .clip_between(
                now - std::time::Duration::from_secs(1),
                now + std::time::Duration::from_millis(80),
                30,
            )
            .expect("clip");

        assert_eq!(
            clip.len(),
            seconds(1.0) + PADDING_MS * SAMPLE_RATE as usize / 1000
        );
    }

    #[test]
    fn a_clip_between_two_moments_respects_the_limit() {
        let mut buffer = AudioBuffer::new(BUFFER_SECONDS);
        let now = Instant::now();
        buffer.push_at(&sound(20.0), now);

        let clip = buffer
            .clip_between(now - std::time::Duration::from_secs(15), now, 3)
            .expect("clip");

        assert_eq!(clip.len(), seconds(3.0));
    }

    #[test]
    fn an_empty_buffer_has_no_clip_between_moments() {
        let buffer = AudioBuffer::new(5);
        let now = Instant::now();

        assert_eq!(buffer.clip_between(now, now, 5), None);
    }

    #[test]
    fn a_resampler_halves_the_rate_of_a_32_khz_signal_and_keeps_its_level() {
        let mut resampler = MonoResampler::new(1, 32_000);

        let out = resampler.push(&vec![0.5_f32; 3_200]);

        assert!((1_599..=1_601).contains(&out.len()), "{}", out.len());
        assert!(out.iter().all(|sample| (*sample - 16_384).abs() <= 1));
    }

    #[test]
    fn a_resampler_averages_the_channels() {
        let mut resampler = MonoResampler::new(2, 16_000);
        let frames: Vec<f32> = (0..100).flat_map(|_| [1.0_f32, -1.0_f32]).collect();

        let out = resampler.push(&frames);

        assert!(out.iter().all(|sample| *sample == 0));
        assert!(out.len() >= 98);
    }

    #[test]
    fn a_resampler_clamps_what_goes_beyond_full_scale() {
        let mut resampler = MonoResampler::new(1, 16_000);

        let out = resampler.push(&[2.0, 2.0, -2.0, -2.0]);

        assert!(out.contains(&32_767));
        assert!(out.iter().all(|sample| *sample >= -32_768));
    }

    #[test]
    fn chunks_pushed_one_after_the_other_give_the_same_sound_as_one_big_chunk() {
        let signal: Vec<f32> = (0..4_800)
            .map(|index| (index as f32 / 40.0).sin() * 0.8)
            .collect();
        let mut whole = MonoResampler::new(1, 48_000);
        let together = whole.push(&signal);
        let mut parts = MonoResampler::new(1, 48_000);
        let mut apart = Vec::new();
        for chunk in signal.chunks(480) {
            apart.extend(parts.push(chunk));
        }

        assert!(together.len().abs_diff(apart.len()) <= 1);
        for (left, right) in together.iter().zip(&apart) {
            assert!((i32::from(*left) - i32::from(*right)).abs() <= 2);
        }
    }

    #[test]
    fn an_empty_chunk_gives_nothing_and_a_partial_frame_is_ignored() {
        let mut resampler = MonoResampler::new(2, 16_000);

        assert!(resampler.push(&[]).is_empty());
        assert!(resampler.push(&[0.3]).is_empty());
    }
}
