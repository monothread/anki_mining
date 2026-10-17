//! Runs the hover lookup: watches the mouse, reads the screen when triggered and shows the popup.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

use crate::capture::{screen_origin, Region, ScreenCapture, SystemCapture};
use crate::config::{Config, ConfigFile};
use crate::hover::placement::{popup_position, popup_position_beside, Rect};
use crate::hover::trigger::{PointerSample, Trigger, TriggerKey, TriggerMode};
use crate::hover::{is_word, lookup_in, HoverMemory, LookupOutcome};
use crate::media::crop_to_region;
use crate::ocr::{MeikiEngine, OcrEngine};
use crate::pointer::WindowIdentity;
use crate::pointer::{PointerTracker, SystemPointer};
use crate::sources::now_millis;
use crate::tokenizer::Tokenizer;
use crate::types::token::Token;
use image::RgbImage;

use super::state::AppState;

pub const POPUP_LABEL: &str = "popup";
const POPUP_PAGE: &str = "popup.html";
const RESULT_EVENT: &str = "hover-result";
const ERROR_EVENT: &str = "app-error";
const INFO_EVENT: &str = "hover-info";
const KNOWN_EVENT: &str = "hover-known";
const POPUP_WIDTH: i32 = 460;
const POPUP_HEIGHT: i32 = 340;
const POLL_INTERVAL: Duration = Duration::from_millis(30);
/// Time the window manager needs to remove the popup from the screen before it is captured.
const HIDE_SETTLE: Duration = Duration::from_millis(90);
/// How far the cursor may leave the word and the popup before the popup goes away.
const KEEP_MARGIN: i32 = 24;

/// What the user can change while the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverStatus {
    pub enabled: bool,
    pub mode: TriggerMode,
    pub key: TriggerKey,
    pub dwell_ms: u64,
}

/// Shared between the hover thread and the commands that change its behavior.
pub struct HoverControl {
    enabled: AtomicBool,
    trigger: Mutex<Trigger>,
    key: TriggerKey,
    dwell_ms: u64,
    store: Option<Arc<ConfigFile>>,
}

impl HoverControl {
    pub fn new(config: &Config) -> HoverControl {
        HoverControl {
            enabled: AtomicBool::new(config.hover_enabled),
            trigger: Mutex::new(Trigger::new(config.hover_mode, config.hover_dwell_ms)),
            key: config.hover_key,
            dwell_ms: config.hover_dwell_ms,
            store: None,
        }
    }

    /// Saves every later change to `path`, merged into `config` (the settings the app started
    /// with), so the choice survives a restart.
    pub fn with_persistence(mut self, path: &Path, config: Config) -> HoverControl {
        self.store = Some(Arc::new(ConfigFile::new(Some(path.to_path_buf()), config)));
        self
    }

    /// Saves every later change through `file`, which other parts of the app share.
    pub fn with_config_file(mut self, file: Arc<ConfigFile>) -> HoverControl {
        self.store = Some(file);
        self
    }

    pub fn status(&self) -> HoverStatus {
        HoverStatus {
            enabled: self.enabled.load(Ordering::SeqCst),
            mode: self.mode(),
            key: self.key,
            dwell_ms: self.dwell_ms,
        }
    }

    pub fn key(&self) -> TriggerKey {
        self.key
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Turns the lookup on or off. The change is applied even when saving it fails; the error
    /// says why it will not survive a restart.
    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.enabled.store(enabled, Ordering::SeqCst);
        self.persist(|config| config.hover_enabled = enabled)
    }

    pub fn mode(&self) -> TriggerMode {
        self.trigger
            .lock()
            .map_or(TriggerMode::Hold, |trigger| trigger.mode())
    }

    /// Switches the trigger. The change is applied even when saving it fails; the error says why
    /// it will not survive a restart.
    pub fn set_mode(&self, mode: TriggerMode) -> Result<(), String> {
        if let Ok(mut trigger) = self.trigger.lock() {
            trigger.set_mode(mode);
        }
        self.persist(|config| config.hover_mode = mode)
    }

    fn persist(&self, change: impl FnOnce(&mut Config)) -> Result<(), String> {
        match &self.store {
            Some(store) => store.update(change),
            None => Ok(()),
        }
    }

    /// Feeds a pointer reading to the trigger.
    pub fn update(&self, sample: PointerSample) -> Option<(i32, i32)> {
        self.trigger.lock().ok()?.update(sample)
    }
}

/// Creates the (hidden) popup window; it is shown next to the word when there is a result.
pub fn create_popup(app: &AppHandle) -> Result<(), String> {
    WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::App(POPUP_PAGE.into()))
        .title("AnkiMining popup")
        .inner_size(f64::from(POPUP_WIDTH), f64::from(POPUP_HEIGHT))
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Starts the thread that handles hover lookups for the rest of the app's life.
pub fn spawn(
    app: AppHandle,
    control: Arc<HoverControl>,
    tokenizer: Arc<Tokenizer>,
    ocr_dir: PathBuf,
) {
    thread::spawn(move || {
        let parts = (
            SystemPointer::connect().map_err(|error| error.to_string()),
            SystemCapture::connect().map_err(|error| error.to_string()),
            MeikiEngine::from_directory(&ocr_dir).map_err(|error| error.to_string()),
        );
        match parts {
            (Ok(pointer), Ok(capture), Ok(engine)) => {
                watch(&app, &control, &tokenizer, pointer, capture, engine);
            }
            (pointer, capture, engine) => {
                let reason = [pointer.err(), capture.err(), engine.err()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("; ");
                let _ = app.emit(ERROR_EVENT, format!("Hover lookup is off: {reason}"));
            }
        }
    });
}

/// What is on screen after a successful lookup: where the cursor may stay without closing it.
struct Shown {
    keep: [Rect; 2],
    popup: Rect,
}

impl Shown {
    fn keeps(&self, x: i32, y: i32) -> bool {
        self.keep.iter().any(|rect| rect.contains(x, y))
    }
}

fn watch(
    app: &AppHandle,
    control: &HoverControl,
    tokenizer: &Tokenizer,
    mut pointer: SystemPointer,
    mut capture: SystemCapture,
    mut engine: MeikiEngine,
) {
    let mut shown: Option<Shown> = None;
    loop {
        thread::sleep(POLL_INTERVAL);
        if !control.is_enabled() {
            if shown.take().is_some() {
                hide_popup(app);
            }
            continue;
        }
        let Ok(reading) = pointer.read() else {
            continue;
        };
        if let Some(current) = &shown {
            if !current.keeps(reading.x, reading.y) {
                hide_popup(app);
                shown = None;
            }
        }

        let sample = PointerSample {
            x: reading.x,
            y: reading.y,
            key_held: reading.is_held(control.key()),
            time_ms: now_millis(),
        };
        let Some(cursor) = control.update(sample) else {
            continue;
        };
        if shown
            .as_ref()
            .is_some_and(|current| current.popup.contains(cursor.0, cursor.1))
        {
            continue;
        }
        if is_over_app_window(app, cursor) {
            // The user is using the app itself, not reading something under it.
            continue;
        }
        if shown.take().is_some() {
            // The popup must not be part of the screenshot that is read.
            hide_popup(app);
            thread::sleep(HIDE_SETTLE);
        }
        let window = pointer.window_under_pointer().ok().flatten();
        let identity = pointer.identify_window_under_pointer().unwrap_or_default();
        shown = look_up(
            app,
            tokenizer,
            &mut capture,
            &mut engine,
            cursor,
            (window, identity),
        );
    }
}

fn look_up(
    app: &AppHandle,
    tokenizer: &Tokenizer,
    capture: &mut SystemCapture,
    engine: &mut MeikiEngine,
    cursor: (i32, i32),
    (window, identity): (Option<Region>, WindowIdentity),
) -> Option<Shown> {
    let scan = |sentence: &str, start: usize| {
        app.state::<AppState>()
            .scan_at(sentence, start)
            .map_err(|error| error.to_string())
    };
    let (screen_width, screen_height) = capture.screen_size();
    let screen = match capture.capture(Region {
        x: 0,
        y: 0,
        width: screen_width,
        height: screen_height,
    }) {
        Ok(screen) => screen,
        Err(error) => {
            let _ = app.emit(INFO_EVENT, error.to_string());
            return None;
        }
    };
    let outcome = lookup_in(
        &screen,
        engine as &mut dyn OcrEngine,
        tokenizer,
        &scan,
        cursor,
    );
    // Over the bare desktop there is no window to crop to, so the picture is the whole screen.
    let picture_area = window.unwrap_or(Region {
        x: 0,
        y: 0,
        width: screen_width,
        height: screen_height,
    });
    let screenshot = crop_to_region(&screen, picture_area);
    match outcome {
        Ok(LookupOutcome::Found(result)) if !result.results.is_empty() => {
            let (screen_width, screen_height) = (screen_width as i32, screen_height as i32);
            let (x, y) = if result.vertical {
                popup_position_beside(
                    result.line_bbox,
                    cursor.1,
                    POPUP_WIDTH,
                    POPUP_HEIGHT,
                    screen_width,
                    screen_height,
                )
            } else {
                popup_position(
                    result.line_bbox,
                    cursor.0,
                    POPUP_WIDTH,
                    POPUP_HEIGHT,
                    screen_width,
                    screen_height,
                )
            };
            let popup = Rect {
                x,
                y,
                width: POPUP_WIDTH,
                height: POPUP_HEIGHT,
            };
            let [left, top, right, bottom] = result.line_bbox;
            let line = Rect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            };
            show_popup(app, popup, &result, screenshot, identity);
            Some(Shown {
                keep: [popup.grow(KEEP_MARGIN), line.grow(KEEP_MARGIN)],
                popup,
            })
        }
        Ok(other) => {
            if let Some(explanation) = other.explanation() {
                let _ = app.emit(INFO_EVENT, explanation);
            }
            None
        }
        Err(error) => {
            let _ = app.emit(ERROR_EVENT, error.to_string());
            None
        }
    }
}

/// Whether `cursor` (screen pixels) is on one of the app's own windows.
fn is_over_app_window(app: &AppHandle, cursor: (i32, i32)) -> bool {
    app.webview_windows().values().any(|window| {
        if window.label() == POPUP_LABEL || !window.is_visible().unwrap_or(false) {
            return false;
        }
        let origin = screen_origin();
        match (window.outer_position(), window.outer_size()) {
            (Ok(position), Ok(size)) => Rect {
                x: position.x - origin.0,
                y: position.y - origin.1,
                width: size.width as i32,
                height: size.height as i32,
            }
            .contains(cursor.0, cursor.1),
            _ => false,
        }
    })
}

/// Which words of the sentence of the popup already have a card in Anki.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownWords {
    pub sentence: String,
    pub words: Vec<KnownWord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownWord {
    /// Where the word starts in the sentence (UTF-16 code units).
    pub start: usize,
    pub known: bool,
}

/// The words of `tokens` that are worth checking and the dictionary form of each.
pub fn known_candidates(tokens: &[Token]) -> Vec<(usize, String)> {
    tokens
        .iter()
        .filter(|token| is_word(token))
        .map(|token| (token.start, token.lemma.clone()))
        .collect()
}

/// Asks Anki which words of the popup's sentence it already has and tells the popup, without
/// making the popup wait for it.
fn check_known_words(
    app: &AppHandle,
    result: &crate::hover::HoverResult,
    identity: WindowIdentity,
) {
    let candidates = known_candidates(&result.tokens);
    if candidates.is_empty() {
        return;
    }
    let app = app.clone();
    let sentence = result.sentence.clone();
    thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let lemmas: Vec<String> = candidates.iter().map(|(_, lemma)| lemma.clone()).collect();
        let Ok(known) = state.known_words(&lemmas, Some(&identity)) else {
            return;
        };
        let words = candidates
            .iter()
            .zip(known)
            .map(|((start, _), known)| KnownWord {
                start: *start,
                known,
            })
            .collect();
        let _ = app.emit_to(POPUP_LABEL, KNOWN_EVENT, KnownWords { sentence, words });
    });
}

fn show_popup(
    app: &AppHandle,
    popup: Rect,
    result: &crate::hover::HoverResult,
    screenshot: Option<RgbImage>,
    identity: WindowIdentity,
) {
    let Some(window) = app.get_webview_window(POPUP_LABEL) else {
        return;
    };
    let _ = window.set_size(PhysicalSize::new(popup.width as u32, popup.height as u32));
    let origin = screen_origin();
    let _ = window.set_position(PhysicalPosition::new(
        popup.x + origin.0,
        popup.y + origin.1,
    ));
    let _ = app.emit_to(POPUP_LABEL, RESULT_EVENT, result);
    if let Some(memory) = app.try_state::<HoverMemory>() {
        memory.set_with_origin(result.clone(), screenshot, identity.clone());
    }
    let _ = window.show();
    check_known_words(app, result, identity);
}

fn hide_popup(app: &AppHandle) {
    if let Some(memory) = app.try_state::<HoverMemory>() {
        memory.clear();
    }
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        let _ = window.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hover::trigger::DEFAULT_DWELL_MS;

    fn control() -> HoverControl {
        HoverControl::new(&Config::default())
    }

    #[test]
    fn the_control_starts_enabled_with_the_configured_trigger() {
        assert_eq!(
            control().status(),
            HoverStatus {
                enabled: true,
                mode: TriggerMode::Hold,
                key: TriggerKey::Shift,
                dwell_ms: DEFAULT_DWELL_MS,
            }
        );
    }

    #[test]
    fn the_control_uses_the_modes_keys_and_dwell_of_the_config() {
        let config = Config {
            hover_mode: TriggerMode::Auto,
            hover_key: TriggerKey::Alt,
            hover_dwell_ms: 450,
            ..Config::default()
        };

        let status = HoverControl::new(&config).status();

        assert_eq!(status.mode, TriggerMode::Auto);
        assert_eq!(status.key, TriggerKey::Alt);
        assert_eq!(status.dwell_ms, 450);
    }

    #[test]
    fn it_can_be_disabled_and_enabled_again() {
        let control = control();

        control
            .set_enabled(false)
            .expect("no store, nothing to save");
        assert!(!control.is_enabled());
        assert!(!control.status().enabled);

        control
            .set_enabled(true)
            .expect("no store, nothing to save");
        assert!(control.is_enabled());
    }

    #[test]
    fn changing_the_mode_changes_how_the_trigger_reacts() {
        let control = control();
        let rest = |time_ms| PointerSample {
            x: 10,
            y: 10,
            key_held: false,
            time_ms,
        };

        // Hold mode ignores a resting cursor.
        assert_eq!(control.update(rest(0)), None);
        assert_eq!(control.update(rest(1_000)), None);

        control
            .set_mode(TriggerMode::Auto)
            .expect("no store, nothing to save");
        assert_eq!(control.mode(), TriggerMode::Auto);
        assert_eq!(control.update(rest(2_000)), None);
        assert_eq!(
            control.update(rest(2_000 + DEFAULT_DWELL_MS)),
            Some((10, 10))
        );
    }

    #[test]
    fn the_key_is_the_one_from_the_config() {
        assert_eq!(control().key(), TriggerKey::Shift);
    }

    #[test]
    fn the_status_serializes_with_camel_case_keys() {
        assert_eq!(
            serde_json::to_value(control().status()).expect("serialize"),
            serde_json::json!({ "enabled": true, "mode": "hold", "key": "shift", "dwellMs": 300 })
        );
    }

    #[test]
    fn the_popup_stays_open_inside_its_own_area_and_around_the_line() {
        let popup = Rect {
            x: 400,
            y: 400,
            width: 460,
            height: 340,
        };
        let line = Rect {
            x: 100,
            y: 300,
            width: 500,
            height: 50,
        };
        let shown = Shown {
            keep: [popup.grow(KEEP_MARGIN), line.grow(KEEP_MARGIN)],
            popup,
        };

        assert!(shown.keeps(500, 500));
        assert!(shown.keeps(150, 320));
        assert!(shown.keeps(380, 380));
        assert!(!shown.keeps(10, 10));
        assert!(!shown.keeps(1200, 900));
    }

    // --- persistence ---

    fn saved(path: &Path) -> Config {
        Config::load(path).expect("saved config")
    }

    #[test]
    fn the_trigger_mode_is_saved_and_the_other_settings_are_kept() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        let started_with = Config {
            websocket_port: 7123,
            hover_key: TriggerKey::Alt,
            ..Config::default()
        };
        let control =
            HoverControl::new(&started_with).with_persistence(&path, started_with.clone());

        control.set_mode(TriggerMode::Auto).expect("save");

        assert_eq!(
            saved(&path),
            Config {
                hover_mode: TriggerMode::Auto,
                ..started_with
            }
        );
    }

    #[test]
    fn switching_the_lookup_off_and_on_is_saved() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        let control =
            HoverControl::new(&Config::default()).with_persistence(&path, Config::default());

        control.set_enabled(false).expect("save");
        assert!(!saved(&path).hover_enabled);

        control.set_enabled(true).expect("save");
        assert!(saved(&path).hover_enabled);
    }

    #[test]
    fn changes_accumulate_in_the_saved_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        let control =
            HoverControl::new(&Config::default()).with_persistence(&path, Config::default());

        control.set_mode(TriggerMode::Auto).expect("save");
        control.set_enabled(false).expect("save");

        let config = saved(&path);
        assert_eq!(config.hover_mode, TriggerMode::Auto);
        assert!(!config.hover_enabled);
    }

    #[test]
    fn the_saved_settings_are_used_by_the_next_start() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        let first =
            HoverControl::new(&Config::default()).with_persistence(&path, Config::default());
        first.set_mode(TriggerMode::Auto).expect("save");
        first.set_enabled(false).expect("save");

        let second = HoverControl::new(&saved(&path));

        assert_eq!(
            second.status(),
            HoverStatus {
                enabled: false,
                mode: TriggerMode::Auto,
                key: TriggerKey::Shift,
                dwell_ms: DEFAULT_DWELL_MS,
            }
        );
    }

    #[test]
    fn nothing_is_written_without_persistence() {
        let directory = tempfile::tempdir().expect("temp dir");
        let control = HoverControl::new(&Config::default());

        control
            .set_mode(TriggerMode::Auto)
            .expect("nothing to save");
        control.set_enabled(false).expect("nothing to save");

        assert_eq!(std::fs::read_dir(directory.path()).expect("dir").count(), 0);
    }

    #[test]
    fn a_failed_save_is_reported_but_the_change_still_applies() {
        let directory = tempfile::tempdir().expect("temp dir");
        let blocker = directory.path().join("blocker");
        std::fs::write(&blocker, "x").expect("write");
        let path = blocker.join("config.json");
        let control =
            HoverControl::new(&Config::default()).with_persistence(&path, Config::default());

        let mode = control.set_mode(TriggerMode::Auto);
        let enabled = control.set_enabled(false);

        assert!(mode
            .expect_err("mode")
            .starts_with("Could not save the setting:"));
        assert!(enabled
            .expect_err("enabled")
            .starts_with("Could not save the setting:"));
        assert_eq!(control.mode(), TriggerMode::Auto);
        assert!(!control.is_enabled());
    }

    #[test]
    fn a_change_that_makes_the_config_invalid_is_not_written() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        let invalid = Config {
            websocket_port: 0,
            ..Config::default()
        };
        let control = HoverControl::new(&invalid).with_persistence(&path, invalid.clone());

        let outcome = control.set_mode(TriggerMode::Auto);

        assert!(outcome.is_err());
        assert!(!path.exists());
    }

    #[test]
    fn the_lookup_starts_off_when_the_config_says_so() {
        let config = Config {
            hover_enabled: false,
            ..Config::default()
        };

        assert!(!HoverControl::new(&config).is_enabled());
    }

    fn token(surface: &str, lemma: &str, pos: &str, start: usize, end: usize) -> Token {
        Token {
            surface: surface.to_string(),
            lemma: lemma.to_string(),
            reading: String::new(),
            pos: pos.to_string(),
            start,
            end,
        }
    }

    #[test]
    fn only_words_are_checked_in_anki_with_their_dictionary_form() {
        let tokens = [
            token("食べた", "食べる", "動詞", 0, 3),
            token("、", "、", "補助記号", 3, 4),
            token("猫", "猫", "名詞", 4, 5),
            token(" ", " ", "空白", 5, 6),
        ];

        assert_eq!(
            known_candidates(&tokens),
            vec![(0, "食べる".to_string()), (4, "猫".to_string())]
        );
        assert_eq!(known_candidates(&[]), Vec::new());
    }

    #[test]
    fn the_known_words_event_serializes_with_camel_case_keys() {
        let event = KnownWords {
            sentence: "猫が好き".to_string(),
            words: vec![
                KnownWord {
                    start: 0,
                    known: true,
                },
                KnownWord {
                    start: 1,
                    known: false,
                },
            ],
        };

        assert_eq!(
            serde_json::to_value(event).expect("json"),
            serde_json::json!({
                "sentence": "猫が好き",
                "words": [{ "start": 0, "known": true }, { "start": 1, "known": false }]
            })
        );
    }
}
