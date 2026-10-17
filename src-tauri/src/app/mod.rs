//! Application layer: state, the sentence pipeline and the Tauri glue.

pub mod hover;
pub mod pipeline;
pub mod selftest;
pub mod state;
mod tray;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, State, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::audio::{AudioBuffer, AudioRecorder, BUFFER_SECONDS};
use crate::capture::{screen_origin, Region, SystemCapture};
use crate::cards::CardInput;
use crate::config::{Config, ConfigFile, MiningSettings};
use crate::dictionary::{install_bundled, Dictionary, ImportStats, MetadataStats, ScanResult};
use crate::hover::trigger::TriggerMode;
use crate::hover::{card_input, HoverMemory};
use crate::normalizer::Normalizer;
use crate::ocr::meiki::{DETECTION_MODEL, RECOGNITION_MODEL, RUNTIME_LIBRARY};
use crate::ocr::MeikiEngine;
use crate::sources::clipboard::ClipboardSource;
use crate::sources::mpv::{MpvConnection, MpvSource, SubtitleClock};
use crate::sources::ocr::{OcrSource, RegionHandle, DEFAULT_POLL_INTERVAL as OCR_POLL_INTERVAL};
use crate::sources::websocket::WebSocketSource;
use crate::tokenizer::Tokenizer;
use crate::tts::{Speaker, VoicevoxSpeaker};
use crate::types::text_source::{TextSource, TextSourceEvent};

use self::hover::{HoverControl, HoverStatus};
use self::state::{AppState, Startup, Status};

const CONFIG_FILE: &str = "config.json";
const DICTIONARY_FILE: &str = "jmdict.db";
/// Folder (inside the bundled resources) that holds the speech helper, its voice and runtime.
const TTS_RESOURCE_DIR: &str = "tts";
/// Overrides where the speech files are looked for (development builds have no resources).
const TTS_DIR_VARIABLE: &str = "ANKI_MINING_TTS_DIR";
/// Folder (inside the bundled resources) that holds the dictionary.
const DICTIONARY_RESOURCE_DIR: &str = "dictionary";
/// Overrides where the bundled dictionary is looked for (development builds have no resources).
const DICTIONARY_DIR_VARIABLE: &str = "ANKI_MINING_DICTIONARY_DIR";
const SENTENCE_EVENT: &str = "sentence";
const MINE_EVENT: &str = "mine-hotkey";
const MINE_RESULT_EVENT: &str = "mine-result";
/// Sent when the shortcut starts adding a card, so the windows can show that it is being added.
const MINE_STARTED_EVENT: &str = "mine-started";
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

/// The sources that started, the websocket port, mpv's connection and the clock of its subtitles.
type StartedSources = (
    Vec<Box<dyn TextSource>>,
    Option<u16>,
    Option<MpvConnection>,
    Option<SubtitleClock>,
);

/// Tells whether the mpv source is connected; `None` when the source is off.
struct MpvStatus(Option<MpvConnection>);

/// Keeps the running sources alive for the lifetime of the app.
struct RunningSources(Mutex<Vec<Box<dyn TextSource>>>);

/// The program that records the sound; it is stopped when the app closes.
struct RecorderHandle(Mutex<Option<AudioRecorder>>);

const MAIN_LABEL: &str = "main";

/// Stops everything that runs in the background: the text sources and the sound recorder (a
/// separate program that would otherwise keep running).
fn shutdown(app: &AppHandle) {
    if let Some(recorder) = app.try_state::<RecorderHandle>() {
        if let Ok(mut recorder) = recorder.0.lock() {
            recorder.take();
        }
    }
    if let Some(sources) = app.try_state::<RunningSources>() {
        if let Ok(mut sources) = sources.0.lock() {
            for source in sources.iter_mut() {
                let _ = source.stop();
            }
        }
    }
}

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
fn get_settings(state: State<'_, AppState>) -> MiningSettings {
    state.settings()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: MiningSettings,
) -> Result<MiningSettings, String> {
    let saved = state.save_settings(settings)?;
    tray::sync_tray_on_main_thread(&app, saved.close_to_tray);
    Ok(saved)
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

/// What happened when a word was added from the popup or with the shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct MineResult {
    success: bool,
    message: String,
}

/// Adds the chosen entry of the word shown in the popup to Anki.
fn mine_remembered(
    state: &AppState,
    memory: &HoverMemory,
    result_index: usize,
    entry_index: usize,
) -> MineResult {
    let word = memory
        .get()
        .and_then(|result| card_input(&result, result_index, entry_index))
        .map(|input| input.entry.expression);
    match state.mine_from_hover(memory, result_index, entry_index) {
        Ok(note_id) => MineResult {
            success: true,
            message: format!("Added {} (note {note_id})", word.unwrap_or_default()),
        },
        Err(error) => MineResult {
            success: false,
            message: error.to_string(),
        },
    }
}

/// Runs slow work off the main thread. A command that is not `async` runs on the main thread, and
/// while it works (talking to Anki, making the voice) every window stops: the button would not even
/// be redrawn as "adding".
async fn off_main_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    tauri::async_runtime::spawn_blocking(work).await.ok()
}

const INTERNAL_ERROR: &str = "internal error: the work stopped unexpectedly";

#[tauri::command]
async fn known_words(app: AppHandle, words: Vec<String>) -> Result<Vec<bool>, String> {
    off_main_thread(move || {
        app.state::<AppState>()
            .known_words(&words, None)
            .map_err(|error| error.to_string())
    })
    .await
    .unwrap_or_else(|| Err(INTERNAL_ERROR.to_string()))
}

#[tauri::command]
async fn speak_text(app: AppHandle, text: String) -> Result<(), String> {
    off_main_thread(move || app.state::<AppState>().speak_aloud(&text))
        .await
        .unwrap_or_else(|| Err(INTERNAL_ERROR.to_string()))
}

#[tauri::command]
async fn mine_hover(app: AppHandle, result_index: usize, entry_index: usize) -> MineResult {
    off_main_thread(move || {
        mine_remembered(
            &app.state::<AppState>(),
            &app.state::<HoverMemory>(),
            result_index,
            entry_index,
        )
    })
    .await
    .unwrap_or_else(|| MineResult {
        success: false,
        message: INTERNAL_ERROR.to_string(),
    })
}

/// The mining shortcut: adds the word shown in the popup, or, when there is none, tells the main
/// window (which mines the word selected in its own sentence view). The work is done on a thread
/// of its own, so the windows keep drawing while the card is made.
fn handle_mine_shortcut(app: &AppHandle) {
    let popup_word = app
        .try_state::<HoverMemory>()
        .and_then(|memory| memory.get());
    if popup_word.is_none() || app.try_state::<AppState>().is_none() {
        let _ = app.emit(MINE_EVENT, ());
        return;
    }
    let _ = app.emit(MINE_STARTED_EVENT, ());
    let app = app.clone();
    thread::spawn(move || {
        let outcome = mine_remembered(&app.state::<AppState>(), &app.state::<HoverMemory>(), 0, 0);
        let _ = app.emit(MINE_RESULT_EVENT, outcome);
    });
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
async fn mine(app: AppHandle, input: CardInput, profile_id: Option<String>) -> Result<u64, String> {
    off_main_thread(move || {
        app.state::<AppState>()
            .mine(&input, profile_id.as_deref())
            .map_err(|error| error.to_string())
    })
    .await
    .unwrap_or_else(|| Err(INTERNAL_ERROR.to_string()))
}

#[tauri::command]
async fn import_dictionary(app: AppHandle, path: String) -> Result<ImportStats, String> {
    off_main_thread(move || {
        app.state::<AppState>()
            .import_dictionary(Path::new(&path))
            .map_err(|error| error.to_string())
    })
    .await
    .unwrap_or_else(|| Err(INTERNAL_ERROR.to_string()))
}

#[tauri::command]
async fn import_yomitan(app: AppHandle, path: String) -> Result<MetadataStats, String> {
    off_main_thread(move || {
        app.state::<AppState>()
            .import_yomitan(Path::new(&path))
            .map_err(|error| error.to_string())
    })
    .await
    .unwrap_or_else(|| Err(INTERNAL_ERROR.to_string()))
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
    // The selector covers the main monitor; its region is measured from that monitor's corner.
    let region = region.map(|region| in_screen_coordinates(&app, region));
    let applied = state
        .set_ocr_region(region)
        .map_err(|error| error.to_string())?;
    close_selector(&app);
    let _ = app.emit(OCR_REGION_EVENT, applied);
    Ok(())
}

/// A region measured from the corner of the main monitor, measured from the corner of the whole
/// screen instead (they differ when another monitor lies left of or above the main one).
fn in_screen_coordinates(app: &AppHandle, region: Region) -> Region {
    let origin = screen_origin();
    let Ok(Some(monitor)) = app.primary_monitor() else {
        return region;
    };
    Region {
        x: region.x + monitor.position().x - origin.0,
        y: region.y + monitor.position().y - origin.1,
        ..region
    }
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
    let arguments: Vec<String> = std::env::args().collect();
    if let Some(code) = selftest::run_if_requested(&arguments) {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        handle_mine_shortcut(app);
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
            import_yomitan,
            submit_text,
            get_hover,
            mine_hover,
            speak_text,
            known_words,
            set_hover_mode,
            set_hover_enabled,
            get_settings,
            save_settings,
            set_ocr_region,
            cancel_region_selection,
            open_region_selector
        ])
        .on_window_event(|window, event| {
            // The popup and the other windows are hidden helpers: closing the main window
            // closes the app and everything it started.
            let WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            if window.label() != MAIN_LABEL {
                return;
            }
            let app = window.app_handle();
            // Only hide the window when the tray icon is there to bring it back.
            let keep_running = tray::is_active(app)
                && app
                    .try_state::<AppState>()
                    .is_some_and(|state| state.settings().close_to_tray);
            if keep_running {
                api.prevent_close();
                tray::hide_main_window(app);
            } else {
                shutdown(app);
                app.exit(0);
            }
        })
        .build(tauri::generate_context!())?
        .run(|app, event| {
            if matches!(event, RunEvent::Exit) {
                shutdown(app);
            }
        });
    Ok(())
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(HoverMemory::default());
    let mut warnings = Vec::new();
    let config_dir = app.path().app_config_dir()?;
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let config_path = config_dir.join(CONFIG_FILE);
    let (config, config_loaded) = load_config(&config_path, &mut warnings);
    let dictionary_path = data_dir.join(DICTIONARY_FILE);
    if let Err(reason) = install_bundled_dictionary(app, &dictionary_path) {
        warnings.push(reason);
    }
    let dictionary = Dictionary::open(&dictionary_path)?;
    let tokenizer = Arc::new(Tokenizer::new()?);

    let (sender, receiver) = mpsc::channel::<TextSourceEvent>();
    let (mut sources, websocket_port, mpv_connection, subtitle_clock) =
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
    let config_file = Arc::new(ConfigFile::new(
        config_loaded.then_some(config_path),
        config.clone(),
    ));
    control = control.with_config_file(Arc::clone(&config_file));
    let hover_control = Arc::new(control);
    spawn_pipeline(app.clone(), receiver, Arc::clone(&tokenizer));
    app.manage(TextSink(sender));
    app.manage(RunningSources(Mutex::new(sources)));
    app.manage(MpvStatus(mpv_connection));
    app.manage(Arc::clone(&hover_control));
    let audio = AudioBuffer::shared(BUFFER_SECONDS);
    if config.audio_on_cards {
        match AudioRecorder::start(Arc::clone(&audio)) {
            Ok(recorder) => {
                app.manage(RecorderHandle(Mutex::new(Some(recorder))));
            }
            Err(error) => {
                warnings.push(format!("Cards will have no audio: {error}"));
            }
        }
    }
    let close_to_tray = config.close_to_tray;
    let speaker = start_speaker(app, &config, &mut warnings);
    let mut state = AppState::new(
        config,
        dictionary,
        Startup {
            warnings,
            websocket_port,
            ocr_unavailable,
        },
        ocr_region,
    )
    .with_config_file(config_file)
    .with_audio(audio)
    .with_subtitles(subtitle_clock.unwrap_or_default());
    if let Some(speaker) = speaker {
        state = state.with_speaker(speaker);
    }
    app.manage(state);
    start_hover(app, &hover_control, &tokenizer);
    tray::sync_tray_on_main_thread(app, close_to_tray);
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
) -> StartedSources {
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
    let mut subtitle_clock = None;
    if !config.mpv_socket.is_empty() {
        let mut mpv = MpvSource::new(Path::new(&config.mpv_socket));
        match mpv.start(sender.clone()) {
            Ok(()) => {
                mpv_connection = Some(mpv.connection());
                subtitle_clock = Some(mpv.clock());
                running.push(Box::new(mpv));
            }
            Err(error) => warnings.push(error.to_string()),
        }
    }
    (running, websocket_port, mpv_connection, subtitle_clock)
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

/// The voice that reads words and sentences for the cards, when its files are there.
fn start_speaker(
    app: &AppHandle,
    config: &Config,
    warnings: &mut Vec<String>,
) -> Option<Arc<dyn Speaker>> {
    let directory = match std::env::var_os(TTS_DIR_VARIABLE) {
        Some(directory) => PathBuf::from(directory),
        None => match app.path().resource_dir() {
            Ok(resources) => resources.join(TTS_RESOURCE_DIR),
            Err(error) => {
                warnings.push(format!("Cards will have no spoken audio: {error}"));
                return None;
            }
        },
    };
    match VoicevoxSpeaker::new(&directory, config.tts_style_id) {
        Ok(speaker) => Some(Arc::new(speaker)),
        Err(error) => {
            warnings.push(format!("Cards will have no spoken audio: {error}"));
            None
        }
    }
}

/// Puts the dictionary that comes with the app in the data folder when there is none there.
fn install_bundled_dictionary(app: &AppHandle, target: &Path) -> Result<(), String> {
    let directory = match std::env::var_os(DICTIONARY_DIR_VARIABLE) {
        Some(directory) => PathBuf::from(directory),
        None => app
            .path()
            .resource_dir()
            .map_err(|error| error.to_string())?
            .join(DICTIONARY_RESOURCE_DIR),
    };
    let bundled = directory.join(DICTIONARY_FILE);
    if !bundled.is_file() {
        return Err(format!(
            "The bundled dictionary is missing ({}); import one in the setup box",
            bundled.display()
        ));
    }
    install_bundled(&bundled, target)
        .map(|_| ())
        .map_err(|error| format!("Could not install the bundled dictionary: {error}"))
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
            SystemCapture::connect()
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::anki::test_support::{failure, ok, MockAnki};
    use crate::config::Config;
    use crate::dictionary::{DeinflectedEntry, Dictionary, ScanResult};
    use crate::hover::HoverResult;
    use crate::sources::ocr::RegionHandle;
    use crate::types::entry::Entry;

    use super::state::Startup;

    fn state_for(mock: &MockAnki) -> AppState {
        AppState::new(
            Config {
                anki_connect_url: mock.url.clone(),
                ..Config::default()
            },
            Dictionary::open_in_memory().expect("dictionary"),
            Startup::default(),
            RegionHandle::default(),
        )
    }

    fn remember(memory: &HoverMemory) {
        memory.set(HoverResult {
            tokens: Vec::new(),
            sentence: "猫が好き".to_string(),
            word_start: 0,
            results: vec![ScanResult {
                source: "猫".to_string(),
                length: 1,
                entries: vec![DeinflectedEntry {
                    entry: Entry {
                        expression: "猫".to_string(),
                        reading: "ねこ".to_string(),
                        meanings: vec!["cat".to_string()],
                        parts_of_speech: vec!["n".to_string()],
                        frequency: None,
                        pitch: Vec::new(),
                    },
                    source: "猫".to_string(),
                    reasons: vec![],
                }],
            }],
            char_bbox: [0, 0, 1, 1],
            line_bbox: [0, 0, 10, 2],
            vertical: false,
        });
    }

    #[test]
    fn a_successful_card_names_the_word_and_the_note() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(4242))]);
        let state = state_for(&mock);
        let memory = HoverMemory::default();
        remember(&memory);

        assert_eq!(
            mine_remembered(&state, &memory, 0, 0),
            MineResult {
                success: true,
                message: "Added 猫 (note 4242)".to_string()
            }
        );
    }

    #[test]
    fn a_failed_card_reports_the_reason() {
        let mock = MockAnki::start(vec![
            ok(json!(["AnkiMining"])),
            failure("cannot create note because it is a duplicate"),
        ]);
        let state = state_for(&mock);
        let memory = HoverMemory::default();
        remember(&memory);

        assert_eq!(
            mine_remembered(&state, &memory, 0, 0),
            MineResult {
                success: false,
                message: "the note is a duplicate".to_string()
            }
        );
    }

    #[test]
    fn mining_with_nothing_shown_says_so() {
        let mock = MockAnki::start(vec![]);
        let state = state_for(&mock);

        assert_eq!(
            mine_remembered(&state, &HoverMemory::default(), 0, 0),
            MineResult {
                success: false,
                message: "there is no word to add: look a word up first".to_string()
            }
        );
    }

    #[test]
    fn the_result_serializes_with_camel_case_keys() {
        let result = MineResult {
            success: true,
            message: "Added".to_string(),
        };

        assert_eq!(
            serde_json::to_value(result).expect("serialize"),
            json!({ "success": true, "message": "Added" })
        );
    }
}
