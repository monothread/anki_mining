//! Application layer: state, the sentence pipeline and the Tauri glue.

pub mod hover;
pub mod pipeline;
pub mod state;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::capture::{Region, X11Capture};
use crate::cards::CardInput;
use crate::config::Config;
use crate::dictionary::{Dictionary, ImportStats, ScanResult};
use crate::hover::trigger::TriggerMode;
use crate::normalizer::Normalizer;
use crate::ocr::meiki::{DETECTION_MODEL, RECOGNITION_MODEL, RUNTIME_LIBRARY};
use crate::ocr::MeikiEngine;
use crate::sources::clipboard::ClipboardSource;
use crate::sources::mpv::{MpvConnection, MpvSource};
use crate::sources::ocr::{OcrSource, RegionHandle, DEFAULT_POLL_INTERVAL as OCR_POLL_INTERVAL};
use crate::sources::websocket::WebSocketSource;
use crate::tokenizer::Tokenizer;
use crate::types::text_source::{TextSource, TextSourceEvent};

use self::hover::{HoverControl, HoverStatus};
use self::state::{AppState, Startup, Status};

const CONFIG_FILE: &str = "config.json";
const DICTIONARY_FILE: &str = "jmdict.db";
const SENTENCE_EVENT: &str = "sentence";
const MINE_EVENT: &str = "mine-hotkey";
const ERROR_EVENT: &str = "app-error";
const OCR_REGION_EVENT: &str = "ocr-region";
const MANUAL_SOURCE_ID: &str = "manual";
const SELECTOR_LABEL: &str = "selector";
const SELECTOR_PAGE: &str = "selector.html";
/// Folder (inside the bundled resources) that holds the OCR models and the runtime library.
const OCR_RESOURCE_DIR: &str = "ocr";
/// Overrides where the OCR files are looked for (development builds have no bundled resources).
const OCR_DIR_VARIABLE: &str = "ANKI_MINING_OCR_DIR";

/// Lets commands push text into the same pipeline the sources feed.
struct TextSink(Sender<TextSourceEvent>);

/// Tells whether the mpv source is connected; `None` when the source is off.
struct MpvStatus(Option<MpvConnection>);

/// Keeps the running sources alive for the lifetime of the app.
struct RunningSources(#[allow(dead_code)] Mutex<Vec<Box<dyn TextSource>>>);

#[tauri::command]
fn get_status(state: State<'_, AppState>, mpv: State<'_, MpvStatus>) -> Result<Status, String> {
    let mut status = state.status().map_err(|error| error.to_string())?;
    status.mpv_connected = mpv.0.as_ref().is_some_and(MpvConnection::is_connected);
    Ok(status)
}

#[tauri::command]
fn get_hover(control: State<'_, Arc<HoverControl>>) -> HoverStatus {
    control.status()
}

#[tauri::command]
fn set_hover_mode(
    app: AppHandle,
    control: State<'_, Arc<HoverControl>>,
    mode: TriggerMode,
) -> HoverStatus {
    report_save_error(&app, control.set_mode(mode));
    control.status()
}

#[tauri::command]
fn set_hover_enabled(
    app: AppHandle,
    control: State<'_, Arc<HoverControl>>,
    enabled: bool,
) -> HoverStatus {
    report_save_error(&app, control.set_enabled(enabled));
    control.status()
}

/// A setting that could not be saved still applies for this run; the user is told why it will
/// not be remembered.
fn report_save_error(app: &AppHandle, outcome: Result<(), String>) {
    if let Err(message) = outcome {
        let _ = app.emit(ERROR_EVENT, message);
    }
}

#[tauri::command]
fn scan_at(
    state: State<'_, AppState>,
    sentence: String,
    start: usize,
) -> Result<Vec<ScanResult>, String> {
    state
        .scan_at(&sentence, start)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn mine(
    state: State<'_, AppState>,
    input: CardInput,
    profile_id: Option<String>,
) -> Result<u64, String> {
    state
        .mine(&input, profile_id.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_dictionary(state: State<'_, AppState>, path: String) -> Result<ImportStats, String> {
    state
        .import_dictionary(Path::new(&path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn submit_text(sink: State<'_, TextSink>, text: String) -> Result<(), String> {
    let event = TextSourceEvent {
        text,
        source_id: MANUAL_SOURCE_ID.to_string(),
        timestamp: crate::sources::now_millis(),
        window_info: None,
    };
    sink.0.send(event).map_err(|error| error.to_string())
}

#[tauri::command]
fn set_ocr_region(
    app: AppHandle,
    state: State<'_, AppState>,
    region: Option<Region>,
) -> Result<(), String> {
    let applied = state
        .set_ocr_region(region)
        .map_err(|error| error.to_string())?;
    close_selector(&app);
    let _ = app.emit(OCR_REGION_EVENT, applied);
    Ok(())
}

#[tauri::command]
fn cancel_region_selection(app: AppHandle) {
    close_selector(&app);
}

#[tauri::command]
fn open_region_selector(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let status = state.status().map_err(|error| error.to_string())?;
    if !status.ocr_available {
        return Err("text recognition is not available (see the warnings)".to_string());
    }
    if let Some(existing) = app.get_webview_window(SELECTOR_LABEL) {
        return existing.set_focus().map_err(|error| error.to_string());
    }

    let monitor = app
        .primary_monitor()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "no monitor found".to_string())?;
    let window =
        WebviewWindowBuilder::new(&app, SELECTOR_LABEL, WebviewUrl::App(SELECTOR_PAGE.into()))
            .title("Select the area to read")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .build()
            .map_err(|error| error.to_string())?;
    window
        .set_position(PhysicalPosition::new(
            monitor.position().x,
            monitor.position().y,
        ))
        .map_err(|error| error.to_string())?;
    window
        .set_size(PhysicalSize::new(
            monitor.size().width,
            monitor.size().height,
        ))
        .map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

fn close_selector(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SELECTOR_LABEL) {
        let _ = window.close();
    }
}

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let _ = app.emit(MINE_EVENT, ());
                    }
                })
                .build(),
        )
        .setup(|app| {
            setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            scan_at,
            mine,
            import_dictionary,
            submit_text,
            get_hover,
            set_hover_mode,
            set_hover_enabled,
            set_ocr_region,
            cancel_region_selection,
            open_region_selector
        ])
        .run(tauri::generate_context!())
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let mut warnings = Vec::new();
    let config_dir = app.path().app_config_dir()?;
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let config_path = config_dir.join(CONFIG_FILE);
    let (config, config_loaded) = load_config(&config_path, &mut warnings);
    let dictionary = Dictionary::open(&data_dir.join(DICTIONARY_FILE))?;
    let tokenizer = Arc::new(Tokenizer::new()?);

    let (sender, receiver) = mpsc::channel::<TextSourceEvent>();
    let (mut sources, websocket_port, mpv_connection) =
        start_sources(&config, &sender, &mut warnings);
    let ocr_region = RegionHandle::default();
    let ocr_unavailable = match start_ocr(app, &sender, &ocr_region) {
        Ok(source) => {
            sources.push(Box::new(source));
            None
        }
        Err(reason) => {
            warnings.push(format!("Text recognition is off: {reason}"));
            Some(reason)
        }
    };
    register_hotkey(app, &config.mining_hotkey, &mut warnings);

    // A settings file that could not be read is never overwritten by a change made in the app.
    let mut control = HoverControl::new(&config);
    if config_loaded {
        control = control.with_persistence(&config_path, config.clone());
    }
    let hover_control = Arc::new(control);
    spawn_pipeline(app.clone(), receiver, Arc::clone(&tokenizer));
    app.manage(TextSink(sender));
    app.manage(RunningSources(Mutex::new(sources)));
    app.manage(MpvStatus(mpv_connection));
    app.manage(Arc::clone(&hover_control));
    app.manage(AppState::new(
        config,
        dictionary,
        Startup {
            warnings,
            websocket_port,
            ocr_unavailable,
        },
        ocr_region,
    ));
    start_hover(app, &hover_control, &tokenizer);
    Ok(())
}

/// The settings, and whether they came from a readable file (or there was none yet).
fn load_config(path: &Path, warnings: &mut Vec<String>) -> (Config, bool) {
    match Config::load(path) {
        Ok(config) => (config, true),
        Err(error) => {
            warnings.push(format!("{error}; using the default settings"));
            (Config::default(), false)
        }
    }
}

fn start_sources(
    config: &Config,
    sender: &Sender<TextSourceEvent>,
    warnings: &mut Vec<String>,
) -> (Vec<Box<dyn TextSource>>, Option<u16>, Option<MpvConnection>) {
    let mut running: Vec<Box<dyn TextSource>> = Vec::new();
    let mut websocket_port = None;

    let mut websocket = WebSocketSource::new(config.websocket_port);
    match websocket.start(sender.clone()) {
        Ok(()) => {
            websocket_port = websocket.local_addr().map(|address| address.port());
            running.push(Box::new(websocket));
        }
        Err(error) => warnings.push(error.to_string()),
    }

    let mut clipboard = ClipboardSource::system();
    match clipboard.start(sender.clone()) {
        Ok(()) => running.push(Box::new(clipboard)),
        Err(error) => warnings.push(error.to_string()),
    }

    let mut mpv_connection = None;
    if !config.mpv_socket.is_empty() {
        let mut mpv = MpvSource::new(Path::new(&config.mpv_socket));
        match mpv.start(sender.clone()) {
            Ok(()) => {
                mpv_connection = Some(mpv.connection());
                running.push(Box::new(mpv));
            }
            Err(error) => warnings.push(error.to_string()),
        }
    }
    (running, websocket_port, mpv_connection)
}

/// Starts the hover lookup when the OCR files are there; otherwise tells the user why not.
fn start_hover(app: &AppHandle, control: &Arc<HoverControl>, tokenizer: &Arc<Tokenizer>) {
    match find_ocr_directory(app) {
        Ok(directory) => {
            if let Err(error) = hover::create_popup(app) {
                let _ = app.emit(ERROR_EVENT, format!("Cannot create the popup: {error}"));
                return;
            }
            hover::spawn(
                app.clone(),
                Arc::clone(control),
                Arc::clone(tokenizer),
                directory,
            );
        }
        Err(reason) => {
            let _ = app.emit(ERROR_EVENT, format!("Hover lookup is off: {reason}"));
        }
    }
}

/// Where the OCR models and the ONNX Runtime library are, if they are all there.
fn find_ocr_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let candidate = match std::env::var_os(OCR_DIR_VARIABLE) {
        Some(directory) => PathBuf::from(directory),
        None => app
            .path()
            .resource_dir()
            .map_err(|error| error.to_string())?
            .join(OCR_RESOURCE_DIR),
    };
    let missing: Vec<&str> = [RUNTIME_LIBRARY, DETECTION_MODEL, RECOGNITION_MODEL]
        .into_iter()
        .filter(|file| !candidate.join(file).is_file())
        .collect();
    if missing.is_empty() {
        Ok(candidate)
    } else {
        Err(format!(
            "missing {} in {} (run scripts/fetch-ocr-models.sh)",
            missing.join(", "),
            candidate.display()
        ))
    }
}

/// Starts the screen-reading source; it stays idle until a region is selected.
fn start_ocr(
    app: &AppHandle,
    sender: &Sender<TextSourceEvent>,
    region: &RegionHandle,
) -> Result<OcrSource, String> {
    let directory = find_ocr_directory(app)?;
    let mut source = OcrSource::new(
        || {
            X11Capture::connect()
                .map(|capture| Box::new(capture) as Box<dyn crate::capture::ScreenCapture>)
                .map_err(|error| error.to_string())
        },
        move || {
            MeikiEngine::from_directory(&directory)
                .map(|engine| Box::new(engine) as Box<dyn crate::ocr::OcrEngine>)
                .map_err(|error| error.to_string())
        },
        OCR_POLL_INTERVAL,
    )
    // Share the caller's handle so the app state and the source see the same region.
    .with_region_handle(region.clone());
    source
        .start(sender.clone())
        .map_err(|error| error.to_string())?;
    Ok(source)
}

fn register_hotkey(app: &AppHandle, hotkey: &str, warnings: &mut Vec<String>) {
    if let Err(error) = app.global_shortcut().register(hotkey) {
        warnings.push(format!("cannot register the shortcut `{hotkey}`: {error}"));
    }
}

fn spawn_pipeline(
    app: AppHandle,
    receiver: mpsc::Receiver<TextSourceEvent>,
    tokenizer: Arc<Tokenizer>,
) {
    thread::spawn(move || {
        let error_app = app.clone();
        pipeline::run(
            &receiver,
            Normalizer::default(),
            &tokenizer,
            |sentence| {
                let _ = app.emit(SENTENCE_EVENT, sentence);
            },
            |message| {
                let _ = error_app.emit(ERROR_EVENT, message);
            },
        );
    });
}
