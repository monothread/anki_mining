//! Text source that reads a region of the screen with OCR.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use image::RgbImage;

use super::text_event;
use super::worker::{is_stopped, Worker};
use crate::capture::{Region, ScreenCapture};
use crate::ocr::{lines_to_text, OcrEngine};
use crate::types::text_source::{SourceError, TextSource, TextSourceEvent};

pub const SOURCE_ID: &str = "ocr";
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Mean brightness difference (0-255) above which a region counts as changed.
pub const CHANGE_THRESHOLD: f32 = 1.5;
/// How many extra reads are made while the text keeps changing before the latest one is used.
pub const MAX_CONFIRMATION_READS: u32 = 4;

const FINGERPRINT_WIDTH: usize = 48;
const FINGERPRINT_HEIGHT: usize = 12;

type CaptureFactory = Arc<dyn Fn() -> Result<Box<dyn ScreenCapture>, String> + Send + Sync>;
type EngineFactory = Arc<dyn Fn() -> Result<Box<dyn OcrEngine>, String> + Send + Sync>;

/// Shared handle to the screen region that is being read; `None` pauses the source.
#[derive(Debug, Clone, Default)]
pub struct RegionHandle(Arc<Mutex<Option<Region>>>);

impl RegionHandle {
    pub fn set(&self, region: Option<Region>) {
        if let Ok(mut current) = self.0.lock() {
            *current = region;
        }
    }

    pub fn get(&self) -> Option<Region> {
        self.0.lock().ok().and_then(|current| *current)
    }
}

/// Reads the selected region when its pixels change and emits the text once two reads agree.
pub struct OcrSource {
    capture_factory: CaptureFactory,
    engine_factory: EngineFactory,
    region: RegionHandle,
    poll_interval: Duration,
    worker: Option<Worker>,
}

impl OcrSource {
    /// The capture and the engine are created on the worker thread, so they do not need to be
    /// `Send`.
    pub fn new<C, E>(capture_factory: C, engine_factory: E, poll_interval: Duration) -> OcrSource
    where
        C: Fn() -> Result<Box<dyn ScreenCapture>, String> + Send + Sync + 'static,
        E: Fn() -> Result<Box<dyn OcrEngine>, String> + Send + Sync + 'static,
    {
        OcrSource {
            capture_factory: Arc::new(capture_factory),
            engine_factory: Arc::new(engine_factory),
            region: RegionHandle::default(),
            poll_interval,
            worker: None,
        }
    }

    pub fn region_handle(&self) -> RegionHandle {
        self.region.clone()
    }

    /// Makes the source read the region of `handle`, so one handle can be shared with other code.
    pub fn with_region_handle(mut self, handle: RegionHandle) -> OcrSource {
        self.region = handle;
        self
    }
}

impl TextSource for OcrSource {
    fn id(&self) -> &str {
        SOURCE_ID
    }

    fn start(&mut self, sink: Sender<TextSourceEvent>) -> Result<(), SourceError> {
        if self.worker.is_some() {
            return Err(SourceError::AlreadyRunning(SOURCE_ID.to_string()));
        }

        let capture_factory = Arc::clone(&self.capture_factory);
        let engine_factory = Arc::clone(&self.engine_factory);
        let region = self.region.clone();
        let poll_interval = self.poll_interval;
        let (ready_sender, ready_receiver) = mpsc::channel::<Result<(), String>>();

        let worker = Worker::spawn("ocr-source", move |stop| {
            let (mut capture, mut engine) = match (capture_factory(), engine_factory()) {
                (Ok(capture), Ok(engine)) => (capture, engine),
                (Err(reason), _) | (_, Err(reason)) => {
                    let _ = ready_sender.send(Err(reason));
                    return;
                }
            };
            let _ = ready_sender.send(Ok(()));

            let mut reader = RegionReader::default();
            let mut last_region = None;
            while !is_stopped(&stop) {
                thread::sleep(poll_interval);
                let current = region.get();
                if current != last_region {
                    // A new region (or none) starts from scratch, even if it looks the same.
                    reader.reset();
                    last_region = current;
                }
                let Some(current) = current else {
                    continue;
                };
                let Ok(image) = capture.capture(current) else {
                    continue;
                };
                if let Some(text) = reader.read(&image, engine.as_mut()) {
                    if sink.send(text_event(SOURCE_ID, text)).is_err() {
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
                Err(failed("the OCR thread ended unexpectedly".to_string()))
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

/// Decides when a captured frame needs OCR and when a reading is final.
#[derive(Debug, Default)]
struct RegionReader {
    /// Fingerprint of the frame that was last read.
    last_fingerprint: Option<Vec<u8>>,
    /// Text that was last emitted (empty after a frame without text).
    emitted: String,
    /// A reading that differs from `emitted` and waits for a second matching read.
    candidate: Option<String>,
    confirmation_reads: u32,
}

impl RegionReader {
    fn reset(&mut self) {
        *self = RegionReader::default();
    }

    /// Returns the text to emit for this frame, if any.
    fn read(&mut self, image: &RgbImage, engine: &mut dyn OcrEngine) -> Option<String> {
        let fingerprint = fingerprint(image);
        let changed = self
            .last_fingerprint
            .as_ref()
            .is_none_or(|previous| difference(previous, &fingerprint) > CHANGE_THRESHOLD);
        if !changed && self.candidate.is_none() {
            return None;
        }
        self.last_fingerprint = Some(fingerprint);

        let text = engine
            .recognize(image)
            .map(|lines| lines_to_text(&lines))
            .unwrap_or_default();
        self.settle(text)
    }

    fn settle(&mut self, text: String) -> Option<String> {
        if text.is_empty() {
            self.emitted.clear();
            self.candidate = None;
            return None;
        }
        if text == self.emitted {
            self.candidate = None;
            return None;
        }

        let confirmed = self.candidate.as_deref() == Some(text.as_str());
        self.confirmation_reads += 1;
        if confirmed || self.confirmation_reads > MAX_CONFIRMATION_READS {
            self.emitted.clone_from(&text);
            self.candidate = None;
            self.confirmation_reads = 0;
            return Some(text);
        }
        self.candidate = Some(text);
        None
    }
}

/// A coarse brightness grid of the image, cheap to compare.
fn fingerprint(image: &RgbImage) -> Vec<u8> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let mut grid = Vec::with_capacity(FINGERPRINT_WIDTH * FINGERPRINT_HEIGHT);
    for cell_y in 0..FINGERPRINT_HEIGHT {
        for cell_x in 0..FINGERPRINT_WIDTH {
            let x0 = cell_x * width / FINGERPRINT_WIDTH;
            let x1 = ((cell_x + 1) * width / FINGERPRINT_WIDTH)
                .max(x0 + 1)
                .min(width);
            let y0 = cell_y * height / FINGERPRINT_HEIGHT;
            let y1 = ((cell_y + 1) * height / FINGERPRINT_HEIGHT)
                .max(y0 + 1)
                .min(height);
            let (mut sum, mut count) = (0_u64, 0_u64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let pixel = image.get_pixel(x as u32, y as u32).0;
                    sum += (u64::from(pixel[0]) * 299
                        + u64::from(pixel[1]) * 587
                        + u64::from(pixel[2]) * 114)
                        / 1000;
                    count += 1;
                }
            }
            grid.push(sum.checked_div(count).unwrap_or(0) as u8);
        }
    }
    grid
}

/// Mean absolute difference between two fingerprints (255 when their sizes differ).
fn difference(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 255.0;
    }
    let total: u32 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u32::from(x.abs_diff(*y)))
        .sum();
    total as f32 / a.len() as f32
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use image::Rgb;

    use super::*;
    use crate::capture::CaptureError;
    use crate::ocr::{OcrError, OcrLine};

    const WAIT: Duration = Duration::from_secs(3);
    const FAST_POLL: Duration = Duration::from_millis(2);

    fn frame(shade: u8) -> RgbImage {
        RgbImage::from_pixel(96, 24, Rgb([shade, shade, shade]))
    }

    fn line(text: &str) -> Vec<OcrLine> {
        if text.is_empty() {
            return vec![];
        }
        vec![OcrLine {
            text: text.to_string(),
            bbox: [0, 0, 10, 10],
            chars: vec![],
        }]
    }

    /// Reads the text from a table keyed by the frame's gray level and counts the calls.
    struct TableEngine {
        texts: Vec<(u8, &'static str)>,
        calls: Arc<Mutex<u32>>,
    }

    impl OcrEngine for TableEngine {
        fn recognize(&mut self, image: &RgbImage) -> Result<Vec<OcrLine>, OcrError> {
            *self.calls.lock().expect("lock") += 1;
            let shade = image.get_pixel(0, 0).0[0];
            Ok(line(
                self.texts
                    .iter()
                    .find(|(candidate, _)| *candidate == shade)
                    .map_or("", |(_, text)| text),
            ))
        }
    }

    struct FailingEngine;

    impl OcrEngine for FailingEngine {
        fn recognize(&mut self, _image: &RgbImage) -> Result<Vec<OcrLine>, OcrError> {
            Err(OcrError::Inference("boom".to_string()))
        }
    }

    fn table_engine(texts: Vec<(u8, &'static str)>) -> (TableEngine, Arc<Mutex<u32>>) {
        let calls = Arc::new(Mutex::new(0));
        (
            TableEngine {
                texts,
                calls: Arc::clone(&calls),
            },
            calls,
        )
    }

    // --- RegionReader ---

    #[test]
    fn the_first_frame_is_read_and_needs_a_second_matching_read_to_be_emitted() {
        let (mut engine, calls) = table_engine(vec![(10, "猫が好き")]);
        let mut reader = RegionReader::default();

        // First read only becomes a candidate; the same static frame is read again to confirm it.
        assert_eq!(reader.read(&frame(10), &mut engine), None);
        assert_eq!(
            reader.read(&frame(10), &mut engine),
            Some("猫が好き".to_string())
        );
        assert_eq!(*calls.lock().expect("lock"), 2);
    }

    #[test]
    fn a_static_frame_is_not_read_again_after_its_text_was_emitted() {
        let (mut engine, calls) = table_engine(vec![(10, "猫が好き")]);
        let mut reader = RegionReader::default();
        reader.read(&frame(10), &mut engine);
        reader.read(&frame(10), &mut engine);

        for _ in 0..5 {
            assert_eq!(reader.read(&frame(10), &mut engine), None);
        }

        assert_eq!(*calls.lock().expect("lock"), 2);
    }

    #[test]
    fn a_new_subtitle_is_emitted_after_it_is_confirmed() {
        let (mut engine, _) = table_engine(vec![(10, "一つ目"), (200, "二つ目")]);
        let mut reader = RegionReader::default();
        reader.read(&frame(10), &mut engine);
        assert_eq!(
            reader.read(&frame(10), &mut engine),
            Some("一つ目".to_string())
        );

        assert_eq!(reader.read(&frame(200), &mut engine), None);
        assert_eq!(
            reader.read(&frame(200), &mut engine),
            Some("二つ目".to_string())
        );
    }

    #[test]
    fn a_reading_that_keeps_changing_is_emitted_after_the_maximum_number_of_reads() {
        // Every frame gives different text, as during a fade.
        let texts: Vec<(u8, &'static str)> = vec![
            (10, "あ"),
            (60, "あい"),
            (110, "あいう"),
            (160, "あいうえ"),
            (210, "あいうえお"),
        ];
        let (mut engine, _) = table_engine(texts);
        let mut reader = RegionReader::default();
        let mut emitted = Vec::new();

        for shade in [10, 60, 110, 160, 210] {
            emitted.extend(reader.read(&frame(shade), &mut engine));
        }

        assert_eq!(emitted, vec!["あいうえお".to_string()]);
    }

    #[test]
    fn an_empty_read_clears_the_state_so_the_same_subtitle_can_appear_again() {
        let (mut engine, _) = table_engine(vec![(10, "猫が好き")]);
        let mut reader = RegionReader::default();
        reader.read(&frame(10), &mut engine);
        assert_eq!(
            reader.read(&frame(10), &mut engine),
            Some("猫が好き".to_string())
        );

        assert_eq!(reader.read(&frame(90), &mut engine), None);
        reader.read(&frame(10), &mut engine);
        assert_eq!(
            reader.read(&frame(10), &mut engine),
            Some("猫が好き".to_string())
        );
    }

    #[test]
    fn small_changes_below_the_threshold_do_not_trigger_a_new_read() {
        let (mut engine, calls) = table_engine(vec![(10, "猫"), (11, "猫")]);
        let mut reader = RegionReader::default();
        reader.read(&frame(10), &mut engine);
        reader.read(&frame(10), &mut engine);
        let before = *calls.lock().expect("lock");

        assert_eq!(reader.read(&frame(11), &mut engine), None);

        assert_eq!(*calls.lock().expect("lock"), before);
    }

    #[test]
    fn an_ocr_failure_counts_as_no_text() {
        let mut reader = RegionReader::default();

        assert_eq!(reader.read(&frame(10), &mut FailingEngine), None);
        assert_eq!(reader.read(&frame(200), &mut FailingEngine), None);
    }

    #[test]
    fn reset_forgets_everything() {
        let (mut engine, calls) = table_engine(vec![(10, "猫")]);
        let mut reader = RegionReader::default();
        reader.read(&frame(10), &mut engine);
        reader.reset();

        reader.read(&frame(10), &mut engine);
        assert_eq!(reader.read(&frame(10), &mut engine), Some("猫".to_string()));
        assert_eq!(*calls.lock().expect("lock"), 3);
    }

    // --- fingerprint / difference ---

    #[test]
    fn identical_images_have_no_difference_and_different_ones_do() {
        assert_eq!(
            difference(&fingerprint(&frame(40)), &fingerprint(&frame(40))),
            0.0
        );
        assert_eq!(
            difference(&fingerprint(&frame(40)), &fingerprint(&frame(100))),
            60.0
        );
    }

    #[test]
    fn fingerprints_of_different_sizes_always_differ() {
        assert_eq!(difference(&[1, 2, 3], &[1, 2]), 255.0);
        assert_eq!(difference(&[], &[]), 255.0);
    }

    #[test]
    fn a_local_change_such_as_new_text_raises_the_difference_above_the_threshold() {
        let plain = frame(0);
        let mut with_text = frame(0);
        for y in 8..16 {
            for x in 10..86 {
                with_text.put_pixel(x, y, Rgb([255, 255, 255]));
            }
        }

        assert!(difference(&fingerprint(&plain), &fingerprint(&with_text)) > CHANGE_THRESHOLD);
    }

    #[test]
    fn fingerprints_handle_images_smaller_than_the_grid() {
        assert_eq!(
            fingerprint(&RgbImage::from_pixel(3, 2, Rgb([90, 90, 90]))),
            vec![90; 48 * 12]
        );
    }

    // --- RegionHandle ---

    #[test]
    fn the_region_handle_is_shared_between_clones() {
        let handle = RegionHandle::default();
        let clone = handle.clone();
        assert_eq!(handle.get(), None);

        clone.set(Some(Region {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
        }));
        assert_eq!(
            handle.get(),
            Some(Region {
                x: 1,
                y: 2,
                width: 3,
                height: 4
            })
        );

        handle.set(None);
        assert_eq!(clone.get(), None);
    }

    // --- OcrSource ---

    struct ScriptedCapture {
        frames: Arc<Mutex<VecDeque<RgbImage>>>,
        last: RgbImage,
    }

    impl ScreenCapture for ScriptedCapture {
        fn screen_size(&self) -> (u32, u32) {
            (1920, 1080)
        }

        fn capture(&mut self, _region: Region) -> Result<RgbImage, CaptureError> {
            if let Some(next) = self.frames.lock().expect("lock").pop_front() {
                self.last = next;
            }
            Ok(self.last.clone())
        }
    }

    type Frames = Arc<Mutex<VecDeque<RgbImage>>>;

    fn source(texts: Vec<(u8, &'static str)>) -> (OcrSource, Frames) {
        let frames: Frames = Arc::new(Mutex::new(VecDeque::new()));
        let capture_frames = Arc::clone(&frames);
        let source = OcrSource::new(
            move || {
                Ok(Box::new(ScriptedCapture {
                    frames: Arc::clone(&capture_frames),
                    last: frame(0),
                }) as Box<dyn ScreenCapture>)
            },
            move || {
                let (engine, _) = table_engine(texts.clone());
                Ok(Box::new(engine) as Box<dyn OcrEngine>)
            },
            FAST_POLL,
        );
        (source, frames)
    }

    const REGION: Region = Region {
        x: 0,
        y: 900,
        width: 1920,
        height: 150,
    };

    #[test]
    fn identifies_itself_as_the_ocr_source() {
        let (source, _) = source(vec![]);

        assert_eq!(source.id(), "ocr");
    }

    #[test]
    fn emits_the_text_of_the_region_once_with_the_ocr_source_id() {
        let (mut source, frames) = source(vec![(10, "猫が好きです")]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");
        source.region_handle().set(Some(REGION));
        let before = super::super::now_millis();

        frames.lock().expect("lock").push_back(frame(10));
        let event = receiver.recv_timeout(WAIT).expect("event");
        thread::sleep(Duration::from_millis(60));

        assert_eq!(event.text, "猫が好きです");
        assert_eq!(event.source_id, "ocr");
        assert_eq!(event.window_info, None);
        assert!(event.timestamp >= before);
        assert!(receiver.try_recv().is_err());
        source.stop().expect("stop");
    }

    #[test]
    fn emits_each_new_subtitle_in_order() {
        let (mut source, frames) = source(vec![(10, "一つ目"), (200, "二つ目")]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");
        source.region_handle().set(Some(REGION));

        frames.lock().expect("lock").push_back(frame(10));
        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "一つ目");
        frames.lock().expect("lock").push_back(frame(200));
        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "二つ目");
        source.stop().expect("stop");
    }

    #[test]
    fn selecting_another_region_reads_it_again_even_if_it_looks_the_same() {
        let (mut source, frames) = source(vec![(10, "猫が好き")]);
        let handle = source.region_handle();
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");
        handle.set(Some(REGION));
        frames.lock().expect("lock").push_back(frame(10));
        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "猫が好き");

        handle.set(Some(Region {
            x: 100,
            y: 100,
            width: 600,
            height: 80,
        }));

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "猫が好き");
        source.stop().expect("stop");
    }

    #[test]
    fn does_nothing_while_no_region_is_selected() {
        let (mut source, frames) = source(vec![(10, "猫")]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");

        frames.lock().expect("lock").push_back(frame(10));
        thread::sleep(Duration::from_millis(80));

        assert!(receiver.try_recv().is_err());
        source.stop().expect("stop");
    }

    #[test]
    fn stops_reading_when_the_region_is_cleared() {
        let (mut source, frames) = source(vec![(10, "猫"), (200, "犬")]);
        let handle = source.region_handle();
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");
        handle.set(Some(REGION));
        frames.lock().expect("lock").push_back(frame(10));
        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "猫");

        handle.set(None);
        thread::sleep(Duration::from_millis(30));
        frames.lock().expect("lock").push_back(frame(200));
        thread::sleep(Duration::from_millis(80));

        assert!(receiver.try_recv().is_err());
        source.stop().expect("stop");
    }

    #[test]
    fn reports_a_failure_when_the_capture_cannot_be_created() {
        let mut source = OcrSource::new(
            || Err("no display".to_string()),
            || Ok(Box::new(FailingEngine) as Box<dyn OcrEngine>),
            FAST_POLL,
        );
        let (sender, _receiver) = mpsc::channel();

        assert_eq!(
            source.start(sender),
            Err(SourceError::Failed {
                id: "ocr".to_string(),
                reason: "no display".to_string()
            })
        );
        assert_eq!(
            source.stop(),
            Err(SourceError::NotRunning("ocr".to_string()))
        );
    }

    #[test]
    fn reports_a_failure_when_the_engine_cannot_be_created() {
        let mut source = OcrSource::new(
            || {
                Ok(Box::new(ScriptedCapture {
                    frames: Arc::default(),
                    last: frame(0),
                }) as Box<dyn ScreenCapture>)
            },
            || Err("missing model".to_string()),
            FAST_POLL,
        );
        let (sender, _receiver) = mpsc::channel();

        assert_eq!(
            source.start(sender),
            Err(SourceError::Failed {
                id: "ocr".to_string(),
                reason: "missing model".to_string()
            })
        );
    }

    #[test]
    fn cannot_be_started_twice_or_stopped_when_idle() {
        let (mut source, _) = source(vec![]);
        let (first, _first_receiver) = mpsc::channel();
        let (second, _second_receiver) = mpsc::channel();

        assert_eq!(
            source.stop(),
            Err(SourceError::NotRunning("ocr".to_string()))
        );
        source.start(first).expect("start");
        assert_eq!(
            source.start(second),
            Err(SourceError::AlreadyRunning("ocr".to_string()))
        );
        assert_eq!(source.stop(), Ok(()));
    }

    #[test]
    fn stops_when_the_receiver_is_dropped() {
        let (mut source, frames) = source(vec![(10, "猫")]);
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");
        source.region_handle().set(Some(REGION));
        drop(receiver);

        frames.lock().expect("lock").push_back(frame(10));
        thread::sleep(Duration::from_millis(80));

        assert!(source.stop().is_ok());
    }

    #[test]
    fn a_shared_handle_selects_the_region_the_source_reads() {
        let (source, frames) = source(vec![(10, "猫が好き")]);
        let shared = RegionHandle::default();
        let mut source = source.with_region_handle(shared.clone());
        let (sender, receiver) = mpsc::channel();
        source.start(sender).expect("start");

        shared.set(Some(REGION));
        frames.lock().expect("lock").push_back(frame(10));

        assert_eq!(receiver.recv_timeout(WAIT).expect("event").text, "猫が好き");
        assert_eq!(source.region_handle().get(), Some(REGION));
        source.stop().expect("stop");
    }
}
