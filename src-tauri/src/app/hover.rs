//! Runs the hover lookup: watches the mouse, reads the screen when triggered and shows the popup.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

use crate::capture::{ScreenCapture, X11Capture};
use crate::config::Config;
use crate::hover::placement::{popup_position, Rect};
use crate::hover::trigger::{PointerSample, Trigger, TriggerKey, TriggerMode};
use crate::hover::{lookup_at, LookupOutcome};
use crate::ocr::{MeikiEngine, OcrEngine};
use crate::pointer::{PointerTracker, X11Pointer};
use crate::sources::now_millis;
use crate::tokenizer::Tokenizer;

use super::state::AppState;

pub const POPUP_LABEL: &str = "popup";
const POPUP_PAGE: &str = "popup.html";
const RESULT_EVENT: &str = "hover-result";
const ERROR_EVENT: &str = "app-error";
const INFO_EVENT: &str = "hover-info";
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
}

impl HoverControl {
    pub fn new(config: &Config) -> HoverControl {
        HoverControl {
            enabled: AtomicBool::new(true),
            trigger: Mutex::new(Trigger::new(config.hover_mode, config.hover_dwell_ms)),
            key: config.hover_key,
            dwell_ms: config.hover_dwell_ms,
        }
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

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn mode(&self) -> TriggerMode {
        self.trigger
            .lock()
            .map_or(TriggerMode::Hold, |trigger| trigger.mode())
    }

    pub fn set_mode(&self, mode: TriggerMode) {
        if let Ok(mut trigger) = self.trigger.lock() {
            trigger.set_mode(mode);
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
            X11Pointer::connect().map_err(|error| error.to_string()),
            X11Capture::connect().map_err(|error| error.to_string()),
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
    mut pointer: X11Pointer,
    mut capture: X11Capture,
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
        shown = look_up(app, tokenizer, &mut capture, &mut engine, cursor);
    }
}

fn look_up(
    app: &AppHandle,
    tokenizer: &Tokenizer,
    capture: &mut X11Capture,
    engine: &mut MeikiEngine,
    cursor: (i32, i32),
) -> Option<Shown> {
    let scan = |sentence: &str, start: usize| {
        app.state::<AppState>()
            .scan_at(sentence, start)
            .map_err(|error| error.to_string())
    };
    let outcome = lookup_at(
        capture as &mut dyn ScreenCapture,
        engine as &mut dyn OcrEngine,
        tokenizer,
        &scan,
        cursor,
    );
    match outcome {
        Ok(LookupOutcome::Found(result)) if !result.results.is_empty() => {
            let (screen_width, screen_height) = capture.screen_size();
            let (x, y) = popup_position(
                result.line_bbox,
                cursor.0,
                POPUP_WIDTH,
                POPUP_HEIGHT,
                screen_width as i32,
                screen_height as i32,
            );
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
            show_popup(app, popup, &result);
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
        match (window.outer_position(), window.outer_size()) {
            (Ok(position), Ok(size)) => Rect {
                x: position.x,
                y: position.y,
                width: size.width as i32,
                height: size.height as i32,
            }
            .contains(cursor.0, cursor.1),
            _ => false,
        }
    })
}

fn show_popup(app: &AppHandle, popup: Rect, result: &crate::hover::HoverResult) {
    let Some(window) = app.get_webview_window(POPUP_LABEL) else {
        return;
    };
    let _ = window.set_size(PhysicalSize::new(popup.width as u32, popup.height as u32));
    let _ = window.set_position(PhysicalPosition::new(popup.x, popup.y));
    let _ = app.emit_to(POPUP_LABEL, RESULT_EVENT, result);
    let _ = window.show();
}

fn hide_popup(app: &AppHandle) {
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

        control.set_enabled(false);
        assert!(!control.is_enabled());
        assert!(!control.status().enabled);

        control.set_enabled(true);
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

        control.set_mode(TriggerMode::Auto);
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
}
