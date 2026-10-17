//! Records what the computer plays on Windows: WASAPI loopback of the default output device.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

use super::{AudioError, MonoResampler, SharedAudio};

const PROGRAM: &str = "WASAPI loopback";
/// How long to wait for the audio device to start.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// Keeps the recording running and the buffer filled; it stops with this value.
pub struct AudioRecorder {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl AudioRecorder {
    pub fn start(buffer: SharedAudio) -> Result<AudioRecorder, AudioError> {
        let (ready_sender, ready) = mpsc::channel::<Result<(), String>>();
        let (stop, stopped) = mpsc::channel::<()>();
        // The stream must live on the thread that made it, so that thread keeps it.
        let thread = thread::spawn(move || {
            run(buffer, &ready_sender, &stopped);
        });
        match ready.recv_timeout(START_TIMEOUT) {
            Ok(Ok(())) => Ok(AudioRecorder {
                stop: Some(stop),
                thread: Some(thread),
            }),
            Ok(Err(reason)) => Err(AudioError::Spawn {
                program: PROGRAM.to_string(),
                reason,
            }),
            Err(_) => Err(AudioError::Spawn {
                program: PROGRAM.to_string(),
                reason: "the audio device did not start in time".to_string(),
            }),
        }
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        // Dropping the sender wakes the thread, which then drops the stream.
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(buffer: SharedAudio, ready: &Sender<Result<(), String>>, stopped: &Receiver<()>) {
    match open_stream(buffer) {
        Ok(stream) => {
            let _ = ready.send(Ok(()));
            // Blocks until the recorder is dropped.
            let _ = stopped.recv();
            drop(stream);
        }
        Err(reason) => {
            let _ = ready.send(Err(reason));
        }
    }
}

fn open_stream(buffer: SharedAudio) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "there is no audio output device".to_string())?;
    let supported = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let format = supported.sample_format();
    let config = supported.config();
    let mut resampler = MonoResampler::new(usize::from(config.channels), config.sample_rate.0);
    let error_callback = |error| eprintln!("audio: {error}");

    // An input stream on an output device is the loopback of what that device plays.
    let stream = match format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                push(&buffer, &resampler.push(data));
            },
            error_callback,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                let floats: Vec<f32> = data
                    .iter()
                    .map(|sample| f32::from(*sample) / 32768.0)
                    .collect();
                push(&buffer, &resampler.push(&floats));
            },
            error_callback,
            None,
        ),
        other => return Err(format!("the sample format {other:?} is not supported")),
    }
    .map_err(|error| error.to_string())?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

fn push(buffer: &SharedAudio, samples: &[i16]) {
    if let Ok(mut buffer) = buffer.lock() {
        buffer.push(samples);
    }
}
