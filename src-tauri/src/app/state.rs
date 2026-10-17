use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use thiserror::Error;

use crate::anki::{default_mining_model, AnkiClient, AnkiError};
use crate::audio::{audio_file, SharedAudio};
use crate::capture::Region;
use crate::cards::{build_note, CardInput, MediaFile};
use crate::config::{Config, ConfigFile, DuplicatePolicy, MiningSettings, DEFAULT_MODEL_NAME};
use crate::dictionary::{Dictionary, DictionaryError, ImportStats, ScanResult};
use crate::hover::{card_input, HoverMemory};
use crate::media::screenshot_file;
use crate::profiles::{resolve, Origin};
use crate::sources::mpv::SubtitleClock;
use crate::sources::now_millis;
use crate::sources::ocr::RegionHandle;
use crate::text_offsets::utf16_offset_to_byte;

/// Smaller areas cannot hold a readable line of text.
pub const MIN_REGION_WIDTH: u32 = 32;
pub const MIN_REGION_HEIGHT: u32 = 12;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Dictionary(#[from] DictionaryError),
    #[error(transparent)]
    Anki(#[from] AnkiError),
    #[error("position {0} is outside the sentence")]
    InvalidPosition(usize),
    #[error("profile `{0}` does not exist")]
    UnknownProfile(String),
    #[error("there is no word to add: look a word up first")]
    NothingToMine,
    #[error("text recognition is not available: {0}")]
    OcrUnavailable(String),
    #[error("the selected area is too small ({width}x{height}); it must be at least {MIN_REGION_WIDTH}x{MIN_REGION_HEIGHT} pixels")]
    RegionTooSmall { width: u32, height: u32 },
    #[error("cannot read `{path}`: {reason}")]
    Io { path: String, reason: String },
    #[error("internal state is unavailable (a thread panicked)")]
    Poisoned,
}

/// What the overlay needs to know when it starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub dictionary_entries: usize,
    pub warnings: Vec<String>,
    pub mining_hotkey: String,
    pub websocket_port: Option<u16>,
    pub default_profile_id: String,
    /// False when the OCR models or the screen could not be loaded.
    pub ocr_available: bool,
    /// Screen area that is being read with OCR, if any.
    pub ocr_region: Option<Region>,
    /// Whether a running mpv is connected (filled in by the command, which knows the source).
    pub mpv_connected: bool,
}

/// Facts about startup that are fixed for the lifetime of the app.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Startup {
    pub warnings: Vec<String>,
    pub websocket_port: Option<u16>,
    /// Why OCR is unavailable; `None` when it is running.
    pub ocr_unavailable: Option<String>,
}

pub struct AppState {
    config: Arc<ConfigFile>,
    dictionary: Mutex<Dictionary>,
    anki: AnkiClient,
    startup: Startup,
    ocr_region: RegionHandle,
    model_ready: AtomicBool,
    audio: Option<SharedAudio>,
    subtitles: Option<SubtitleClock>,
}

/// Whether the text read from the screen is (part of) the subtitle line, ignoring spaces.
fn same_line(subtitle: &str, read: &str) -> bool {
    let squash = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let (subtitle, read) = (squash(subtitle), squash(read));
    !subtitle.is_empty()
        && !read.is_empty()
        && (subtitle.contains(&read) || read.contains(&subtitle))
}

impl AppState {
    pub fn new(
        config: Config,
        dictionary: Dictionary,
        startup: Startup,
        ocr_region: RegionHandle,
    ) -> AppState {
        let anki = AnkiClient::new(&config.anki_connect_url);
        AppState {
            config: Arc::new(ConfigFile::new(None, config)),
            dictionary: Mutex::new(dictionary),
            anki,
            startup,
            ocr_region,
            model_ready: AtomicBool::new(false),
            audio: None,
            subtitles: None,
        }
    }

    /// Shares the settings file with the other parts of the app, so every change is saved there.
    pub fn with_config_file(mut self, file: Arc<ConfigFile>) -> AppState {
        self.config = file;
        self
    }

    /// The settings about cards.
    pub fn settings(&self) -> MiningSettings {
        MiningSettings::from_config(&self.config.get())
    }

    /// Replaces the settings about cards. Nothing changes when they are not valid.
    pub fn save_settings(&self, settings: MiningSettings) -> Result<MiningSettings, String> {
        self.config
            .update_checked(|config| settings.apply_to(config))?;
        Ok(self.settings())
    }

    /// Gives the state the recorded sound that cards take their audio from.
    pub fn with_audio(mut self, audio: SharedAudio) -> AppState {
        self.audio = Some(audio);
        self
    }

    /// Tells the state when mpv's subtitles appear and go away, so a card made from one of them
    /// gets exactly the sound of that line.
    pub fn with_subtitles(mut self, subtitles: SubtitleClock) -> AppState {
        self.subtitles = Some(subtitles);
        self
    }

    fn sentence_audio(&self, sentence: &str) -> Option<MediaFile> {
        let config = self.config.get();
        if !config.audio_on_cards {
            return None;
        }
        let buffer = self.audio.as_ref()?.lock().ok()?;
        let seconds = config.audio_seconds as usize;
        let exact = self
            .subtitles
            .as_ref()
            .and_then(SubtitleClock::current)
            .filter(|span| same_line(&span.text, sentence))
            .and_then(|span| {
                buffer.clip_between(
                    span.started,
                    span.ended.unwrap_or_else(Instant::now),
                    seconds,
                )
            });
        let clip = exact.or_else(|| buffer.clip(seconds))?;
        Some(audio_file(&clip, now_millis()))
    }

    pub fn status(&self) -> Result<Status, AppError> {
        let config = self.config.get();
        Ok(Status {
            dictionary_entries: self.lock_dictionary()?.entry_count()?,
            warnings: self.startup.warnings.clone(),
            mining_hotkey: config.mining_hotkey,
            websocket_port: self.startup.websocket_port,
            default_profile_id: config.default_profile_id,
            ocr_available: self.startup.ocr_unavailable.is_none(),
            ocr_region: self.ocr_region.get(),
            mpv_connected: false,
        })
    }

    /// Starts (or moves) screen reading on `region`, or stops it with `None`.
    pub fn set_ocr_region(&self, region: Option<Region>) -> Result<Option<Region>, AppError> {
        if let Some(reason) = &self.startup.ocr_unavailable {
            return Err(AppError::OcrUnavailable(reason.clone()));
        }
        if let Some(area) = region {
            if area.width < MIN_REGION_WIDTH || area.height < MIN_REGION_HEIGHT {
                return Err(AppError::RegionTooSmall {
                    width: area.width,
                    height: area.height,
                });
            }
        }
        self.ocr_region.set(region);
        Ok(region)
    }

    /// Dictionary matches for the text of `sentence` starting at UTF-16 offset `start`.
    pub fn scan_at(&self, sentence: &str, start: usize) -> Result<Vec<ScanResult>, AppError> {
        let byte_start =
            utf16_offset_to_byte(sentence, start).ok_or(AppError::InvalidPosition(start))?;
        Ok(self.lock_dictionary()?.scan(&sentence[byte_start..])?)
    }

    /// Creates the card in the deck of the chosen profile (the default one when `None`) and
    /// returns the Anki note id. The default note type is created on first use.
    pub fn mine(&self, input: &CardInput, profile_id: Option<&str>) -> Result<u64, AppError> {
        let config = self.config.get();
        let profile = match profile_id {
            Some(id) => config.profile(id),
            None => config.default_profile(),
        }
        .ok_or_else(|| {
            AppError::UnknownProfile(profile_id.unwrap_or(&config.default_profile_id).to_string())
        })?;

        if profile.model_name == DEFAULT_MODEL_NAME && !self.model_ready.load(Ordering::SeqCst) {
            self.anki.ensure_model(&default_mining_model())?;
            self.model_ready.store(true, Ordering::SeqCst);
        }
        let allow_duplicate = config.duplicate_policy == DuplicatePolicy::Allow;
        Ok(self
            .anki
            .add_note_with(&build_note(profile, input), allow_duplicate)?)
    }

    /// Adds the chosen entry of the word that is shown in the popup (see [`HoverMemory`]) and
    /// returns the Anki note id.
    pub fn mine_from_hover(
        &self,
        memory: &HoverMemory,
        result_index: usize,
        entry_index: usize,
    ) -> Result<u64, AppError> {
        let mut input = memory
            .get()
            .and_then(|result| card_input(&result, result_index, entry_index))
            .ok_or(AppError::NothingToMine)?;
        let config = self.config.get();
        if config.screenshot_on_cards {
            input.picture = memory
                .screenshot()
                .and_then(|image| screenshot_file(&image, now_millis()).ok());
        }
        input.audio = self.sentence_audio(&input.sentence);
        let identity = memory.identity();
        let origin = Origin {
            source_id: Some(input.source.clone()),
            app_name: identity.app_name,
            window_title: identity.title,
        };
        let profile_id = resolve(&config.profiles, &config.default_profile_id, &origin)
            .map(|profile| profile.id.clone());
        self.mine(&input, profile_id.as_deref())
    }

    /// Replaces the dictionary with the contents of a `jmdict-simplified` JSON file.
    pub fn import_dictionary(&self, path: &Path) -> Result<ImportStats, AppError> {
        let file = File::open(path).map_err(|error| AppError::Io {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        Ok(self
            .lock_dictionary()?
            .import_jmdict(BufReader::new(file))?)
    }

    fn lock_dictionary(&self) -> Result<std::sync::MutexGuard<'_, Dictionary>, AppError> {
        self.dictionary.lock().map_err(|_| AppError::Poisoned)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::anki::test_support::{failure, ok, MockAnki, Reply};
    use crate::config::{default_profile, Config};
    use crate::types::entry::Entry;
    use crate::types::profile::{Profile, ProfileMatch};

    const JMDICT_FIXTURE: &str = r#"{
        "tags": {},
        "words": [
            { "id": "1467640", "kanji": [{ "common": true, "text": "猫", "tags": [] }],
              "kana": [{ "common": true, "text": "ねこ", "tags": [], "appliesToKanji": ["*"] }],
              "sense": [{ "partOfSpeech": ["n"], "gloss": [{ "text": "cat" }] }] },
            { "id": "1358280", "kanji": [{ "common": true, "text": "食べる", "tags": [] }],
              "kana": [{ "common": true, "text": "たべる", "tags": [], "appliesToKanji": ["*"] }],
              "sense": [{ "partOfSpeech": ["v1", "vt"], "gloss": [{ "text": "to eat" }] }] }
        ]
    }"#;

    fn dictionary() -> Dictionary {
        let mut dictionary = Dictionary::open_in_memory().expect("open");
        dictionary
            .import_jmdict(JMDICT_FIXTURE.as_bytes())
            .expect("import");
        dictionary
    }

    fn state_with(config: Config) -> AppState {
        AppState::new(
            config,
            dictionary(),
            Startup::default(),
            RegionHandle::default(),
        )
    }

    fn state_for(mock: &MockAnki) -> AppState {
        state_with(Config {
            anki_connect_url: mock.url.clone(),
            ..Config::default()
        })
    }

    fn input() -> CardInput {
        CardInput {
            entry: Entry {
                expression: "猫".to_string(),
                reading: "ねこ".to_string(),
                meanings: vec!["cat".to_string()],
                parts_of_speech: vec!["n".to_string()],
                frequency: None,
            },
            sentence: "猫が好き".to_string(),
            highlight: Some((0, 1)),
            source: "clipboard".to_string(),
            audio: None,
            picture: None,
        }
    }

    fn actions(mock: &MockAnki) -> Vec<String> {
        mock.requests()
            .iter()
            .map(|request| request["action"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn status_reports_the_dictionary_size_hotkey_and_startup_facts() {
        let startup = Startup {
            warnings: vec!["clipboard unavailable".to_string()],
            websocket_port: Some(6677),
            ocr_unavailable: None,
        };
        let state = AppState::new(
            Config::default(),
            dictionary(),
            startup,
            RegionHandle::default(),
        );

        assert_eq!(
            state.status().expect("status"),
            Status {
                dictionary_entries: 2,
                warnings: vec!["clipboard unavailable".to_string()],
                mining_hotkey: "Alt+M".to_string(),
                websocket_port: Some(6677),
                default_profile_id: "default".to_string(),
                ocr_available: true,
                ocr_region: None,
                mpv_connected: false,
            }
        );
    }

    #[test]
    fn status_serializes_with_camel_case_keys() {
        let state = state_with(Config::default());

        let json = serde_json::to_value(state.status().expect("status")).expect("serialize");

        assert_eq!(
            json,
            json!({
                "dictionaryEntries": 2,
                "warnings": [],
                "miningHotkey": "Alt+M",
                "websocketPort": null,
                "defaultProfileId": "default",
                "ocrAvailable": true,
                "ocrRegion": null,
                "mpvConnected": false
            })
        );
    }

    #[test]
    fn scan_at_scans_from_the_utf16_offset() {
        let state = state_with(Config::default());

        let results = state.scan_at("私は猫が好き", 2).expect("scan");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "猫");
        assert_eq!(results[0].length, 1);
        assert_eq!(
            results[0].entries[0].entry.meanings,
            vec!["cat".to_string()]
        );
    }

    #[test]
    fn scan_at_resolves_inflected_words() {
        let state = state_with(Config::default());

        let results = state.scan_at("昨日食べました", 2).expect("scan");

        assert_eq!(results[0].source, "食べました");
        assert_eq!(results[0].entries[0].entry.expression, "食べる");
        assert_eq!(results[0].entries[0].reasons, vec!["past", "polite"]);
    }

    #[test]
    fn scan_at_with_an_offset_past_the_end_fails() {
        let state = state_with(Config::default());

        let error = state.scan_at("猫", 5).expect_err("must fail");

        assert!(matches!(error, AppError::InvalidPosition(5)));
        assert_eq!(error.to_string(), "position 5 is outside the sentence");
    }

    #[test]
    fn scan_at_at_the_end_of_the_sentence_finds_nothing() {
        let state = state_with(Config::default());

        assert_eq!(state.scan_at("猫", 1).expect("scan"), vec![]);
    }

    #[test]
    fn mine_creates_the_default_model_once_and_adds_the_note_to_the_profile_deck() {
        let mock = MockAnki::start(vec![
            ok(json!(["Basic"])),
            ok(json!({})),
            ok(json!(111)),
            ok(json!(222)),
        ]);
        let state = state_for(&mock);

        assert_eq!(state.mine(&input(), None).expect("first card"), 111);
        assert_eq!(state.mine(&input(), None).expect("second card"), 222);

        assert_eq!(
            actions(&mock),
            vec!["modelNames", "createModel", "addNote", "addNote"]
        );
        let note = &mock.requests()[2]["params"]["note"];
        assert_eq!(note["deckName"], json!("Mining"));
        assert_eq!(note["modelName"], json!("AnkiMining"));
        assert_eq!(
            note["fields"],
            json!({
                "Expression": "猫",
                "Reading": "ねこ",
                "Meaning": "cat",
                "Sentence": "<b>猫</b>が好き",
                "SentenceAudio": "",
                "Picture": "",
                "Frequency": "",
                "Pitch": "",
                "Source": "clipboard"
            })
        );
        assert_eq!(note["tags"], json!(["anki-mining"]));
    }

    #[test]
    fn mine_does_not_create_the_model_when_it_already_exists() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_for(&mock);

        assert_eq!(state.mine(&input(), None).expect("card"), 5);

        assert_eq!(actions(&mock), vec!["modelNames", "addNote"]);
    }

    #[test]
    fn mine_uses_the_requested_profile_and_skips_model_creation_for_custom_models() {
        let games = Profile {
            id: "games".to_string(),
            name: "Games".to_string(),
            deck_name: "Games".to_string(),
            model_name: "My Model".to_string(),
            ..default_profile()
        };
        let mock = MockAnki::start(vec![ok(json!(9))]);
        let state = state_with(Config {
            anki_connect_url: mock.url.clone(),
            profiles: vec![default_profile(), games],
            ..Config::default()
        });

        assert_eq!(state.mine(&input(), Some("games")).expect("card"), 9);

        assert_eq!(actions(&mock), vec!["addNote"]);
        let note = &mock.requests()[0]["params"]["note"];
        assert_eq!(note["deckName"], json!("Games"));
        assert_eq!(note["modelName"], json!("My Model"));
    }

    #[test]
    fn mine_rejects_an_unknown_profile_without_calling_anki() {
        let mock = MockAnki::start(vec![]);
        let state = state_for(&mock);

        let error = state.mine(&input(), Some("ghost")).expect_err("must fail");

        assert!(matches!(error, AppError::UnknownProfile(ref id) if id == "ghost"));
        assert_eq!(error.to_string(), "profile `ghost` does not exist");
        assert_eq!(mock.requests(), Vec::<serde_json::Value>::new());
    }

    #[test]
    fn mine_reports_duplicates() {
        let mock = MockAnki::start(vec![
            ok(json!(["AnkiMining"])),
            failure("cannot create note because it is a duplicate"),
        ]);
        let state = state_for(&mock);

        let error = state.mine(&input(), None).expect_err("must fail");

        assert!(matches!(error, AppError::Anki(AnkiError::Duplicate)));
        assert_eq!(error.to_string(), "the note is a duplicate");
    }

    #[test]
    fn mine_retries_the_model_check_after_a_failure() {
        let mock = MockAnki::start(vec![
            Reply::Status(500),
            ok(json!(["AnkiMining"])),
            ok(json!(3)),
        ]);
        let state = state_for(&mock);

        assert!(state.mine(&input(), None).is_err());
        assert_eq!(state.mine(&input(), None).expect("card"), 3);

        assert_eq!(actions(&mock), vec!["modelNames", "modelNames", "addNote"]);
    }

    #[test]
    fn import_dictionary_replaces_the_contents_from_a_json_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("jmdict.json");
        std::fs::write(
            &path,
            r#"{ "tags": {}, "words": [
                { "id": "1", "kanji": [{ "common": true, "text": "犬", "tags": [] }],
                  "kana": [{ "common": true, "text": "いぬ", "tags": [], "appliesToKanji": ["*"] }],
                  "sense": [{ "partOfSpeech": ["n"], "gloss": [{ "text": "dog" }] }] }
            ] }"#,
        )
        .expect("write");
        let state = state_with(Config::default());

        let stats = state.import_dictionary(&path).expect("import");

        assert_eq!(
            stats,
            ImportStats {
                entries: 1,
                terms: 2,
                skipped: 0
            }
        );
        assert_eq!(state.status().expect("status").dictionary_entries, 1);
        assert_eq!(state.scan_at("犬", 0).expect("scan").len(), 1);
    }

    #[test]
    fn import_dictionary_reports_a_missing_file() {
        let state = state_with(Config::default());

        let error = state
            .import_dictionary(Path::new("/definitely/missing/jmdict.json"))
            .expect_err("must fail");

        assert!(matches!(error, AppError::Io { .. }));
        assert!(error
            .to_string()
            .starts_with("cannot read `/definitely/missing/jmdict.json`"));
        assert_eq!(state.status().expect("status").dictionary_entries, 2);
    }

    #[test]
    fn import_dictionary_reports_invalid_json_and_keeps_the_old_contents() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("broken.json");
        std::fs::write(&path, "{ not json").expect("write");
        let state = state_with(Config::default());

        let error = state.import_dictionary(&path).expect_err("must fail");

        assert!(matches!(
            error,
            AppError::Dictionary(DictionaryError::InvalidJson(_))
        ));
        assert_eq!(state.status().expect("status").dictionary_entries, 2);
    }

    #[test]
    fn card_fields_use_the_expected_field_names() {
        // Guards the contract between the card builder and the default note type.
        let note = build_note(&default_profile(), &input());

        let expected: BTreeMap<String, String> = default_mining_model()
            .in_order_fields
            .into_iter()
            .map(|name| (name, String::new()))
            .collect();
        assert_eq!(
            note.fields.keys().collect::<Vec<_>>(),
            expected.keys().collect::<Vec<_>>()
        );
    }

    fn region(width: u32, height: u32) -> Region {
        Region {
            x: 10,
            y: 900,
            width,
            height,
        }
    }

    #[test]
    fn setting_a_region_starts_reading_and_is_reported_by_the_status() {
        let handle = RegionHandle::default();
        let state = AppState::new(
            Config::default(),
            dictionary(),
            Startup::default(),
            handle.clone(),
        );

        assert_eq!(
            state.set_ocr_region(Some(region(1200, 120))).expect("set"),
            Some(region(1200, 120))
        );

        assert_eq!(handle.get(), Some(region(1200, 120)));
        assert_eq!(
            state.status().expect("status").ocr_region,
            Some(region(1200, 120))
        );
    }

    #[test]
    fn clearing_the_region_stops_reading() {
        let handle = RegionHandle::default();
        let state = AppState::new(
            Config::default(),
            dictionary(),
            Startup::default(),
            handle.clone(),
        );
        state.set_ocr_region(Some(region(1200, 120))).expect("set");

        assert_eq!(state.set_ocr_region(None).expect("clear"), None);

        assert_eq!(handle.get(), None);
        assert_eq!(state.status().expect("status").ocr_region, None);
    }

    #[test]
    fn regions_that_are_too_small_are_rejected_and_keep_the_previous_one() {
        let handle = RegionHandle::default();
        let state = AppState::new(
            Config::default(),
            dictionary(),
            Startup::default(),
            handle.clone(),
        );
        state.set_ocr_region(Some(region(1200, 120))).expect("set");

        let narrow = state
            .set_ocr_region(Some(region(31, 120)))
            .expect_err("narrow");
        let short = state
            .set_ocr_region(Some(region(1200, 11)))
            .expect_err("short");

        assert!(matches!(
            narrow,
            AppError::RegionTooSmall {
                width: 31,
                height: 120
            }
        ));
        assert!(matches!(
            short,
            AppError::RegionTooSmall {
                width: 1200,
                height: 11
            }
        ));
        assert_eq!(
            narrow.to_string(),
            "the selected area is too small (31x120); it must be at least 32x12 pixels"
        );
        assert_eq!(handle.get(), Some(region(1200, 120)));
    }

    #[test]
    fn the_smallest_allowed_region_is_accepted() {
        let state = state_with(Config::default());

        assert_eq!(
            state.set_ocr_region(Some(region(32, 12))).expect("set"),
            Some(region(32, 12))
        );
    }

    #[test]
    fn selecting_a_region_fails_with_the_reason_when_ocr_is_unavailable() {
        let handle = RegionHandle::default();
        let startup = Startup {
            ocr_unavailable: Some("the OCR models are missing".to_string()),
            ..Startup::default()
        };
        let state = AppState::new(Config::default(), dictionary(), startup, handle.clone());

        let error = state
            .set_ocr_region(Some(region(1200, 120)))
            .expect_err("must fail");

        assert!(matches!(error, AppError::OcrUnavailable(_)));
        assert_eq!(
            error.to_string(),
            "text recognition is not available: the OCR models are missing"
        );
        assert_eq!(handle.get(), None);
        assert!(!state.status().expect("status").ocr_available);
    }

    fn remembered(memory: &HoverMemory) {
        memory.set(crate::hover::HoverResult {
            sentence: "私は猫が好き".to_string(),
            word_start: 2,
            results: vec![ScanResult {
                source: "猫".to_string(),
                length: 1,
                entries: vec![crate::dictionary::DeinflectedEntry {
                    entry: Entry {
                        expression: "猫".to_string(),
                        reading: "ねこ".to_string(),
                        meanings: vec!["cat".to_string()],
                        parts_of_speech: vec!["n".to_string()],
                        frequency: None,
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
    fn mining_from_the_popup_adds_the_remembered_word_with_its_sentence() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(77))]);
        let state = state_for(&mock);
        let memory = HoverMemory::default();
        remembered(&memory);

        let id = state.mine_from_hover(&memory, 0, 0).expect("note");

        assert_eq!(id, 77);
        let note = &mock.requests()[1]["params"]["note"];
        assert_eq!(note["deckName"], json!("Mining"));
        assert_eq!(note["fields"]["Expression"], json!("猫"));
        assert_eq!(note["fields"]["Reading"], json!("ねこ"));
        assert_eq!(note["fields"]["Meaning"], json!("cat"));
        assert_eq!(note["fields"]["Sentence"], json!("私は<b>猫</b>が好き"));
        assert_eq!(note["fields"]["Source"], json!("screen"));
    }

    #[test]
    fn mining_without_a_remembered_word_does_not_call_anki() {
        let mock = MockAnki::start(vec![]);
        let state = state_for(&mock);

        let error = state
            .mine_from_hover(&HoverMemory::default(), 0, 0)
            .expect_err("must fail");

        assert!(matches!(error, AppError::NothingToMine));
        assert_eq!(
            error.to_string(),
            "there is no word to add: look a word up first"
        );
        assert_eq!(mock.requests(), Vec::<serde_json::Value>::new());
    }

    #[test]
    fn mining_an_entry_that_does_not_exist_is_nothing_to_mine() {
        let mock = MockAnki::start(vec![]);
        let state = state_for(&mock);
        let memory = HoverMemory::default();
        remembered(&memory);

        assert!(matches!(
            state.mine_from_hover(&memory, 0, 3),
            Err(AppError::NothingToMine)
        ));
        assert!(matches!(
            state.mine_from_hover(&memory, 4, 0),
            Err(AppError::NothingToMine)
        ));
    }

    #[test]
    fn mining_the_same_word_twice_reports_the_duplicate() {
        let mock = MockAnki::start(vec![
            ok(json!(["AnkiMining"])),
            failure("cannot create note because it is a duplicate"),
        ]);
        let state = state_for(&mock);
        let memory = HoverMemory::default();
        remembered(&memory);

        let error = state.mine_from_hover(&memory, 0, 0).expect_err("must fail");

        assert!(matches!(error, AppError::Anki(AnkiError::Duplicate)));
    }

    #[test]
    fn the_card_gets_the_screenshot_of_the_window_as_a_jpeg() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_for(&mock);
        let memory = HoverMemory::default();
        remembered(&memory);
        let window = image::RgbImage::from_pixel(400, 225, image::Rgb([30, 120, 200]));
        memory.set_with_screenshot(memory.get().expect("remembered"), Some(window));

        state.mine_from_hover(&memory, 0, 0).expect("note");

        let picture = &mock.requests()[1]["params"]["note"]["picture"];
        assert_eq!(picture.as_array().map(Vec::len), Some(1));
        let filename = picture[0]["filename"].as_str().expect("filename");
        assert!(
            filename.starts_with("anki-mining-") && filename.ends_with(".jpg"),
            "{filename}"
        );
        assert_eq!(picture[0]["fields"], json!(["Picture"]));
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            picture[0]["data"].as_str().expect("data"),
        )
        .expect("base64");
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn no_picture_is_sent_when_there_is_no_screenshot() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_for(&mock);
        let memory = HoverMemory::default();
        remembered(&memory);

        state.mine_from_hover(&memory, 0, 0).expect("note");

        assert_eq!(mock.requests()[1]["params"]["note"].get("picture"), None);
    }

    #[test]
    fn the_screenshot_can_be_turned_off_in_the_config() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_with(Config {
            anki_connect_url: mock.url.clone(),
            screenshot_on_cards: false,
            ..Config::default()
        });
        let memory = HoverMemory::default();
        remembered(&memory);
        let window = image::RgbImage::from_pixel(100, 100, image::Rgb([1, 2, 3]));
        memory.set_with_screenshot(memory.get().expect("remembered"), Some(window));

        state.mine_from_hover(&memory, 0, 0).expect("note");

        assert_eq!(mock.requests()[1]["params"]["note"].get("picture"), None);
    }

    fn state_with_sound(config: Config, sound: &[i16]) -> AppState {
        let audio = crate::audio::AudioBuffer::shared(60);
        audio.lock().expect("audio").push(sound);
        state_with(config).with_audio(audio)
    }

    fn speech() -> Vec<i16> {
        vec![4000; 16_000]
    }

    #[test]
    fn the_card_gets_the_sentence_audio_as_a_wav() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_with_sound(
            Config {
                anki_connect_url: mock.url.clone(),
                ..Config::default()
            },
            &speech(),
        );
        let memory = HoverMemory::default();
        remembered(&memory);

        state.mine_from_hover(&memory, 0, 0).expect("note");

        let audio = &mock.requests()[1]["params"]["note"]["audio"];
        assert_eq!(audio.as_array().map(Vec::len), Some(1));
        let filename = audio[0]["filename"].as_str().expect("filename");
        assert!(
            filename.starts_with("anki-mining-") && filename.ends_with(".wav"),
            "{filename}"
        );
        assert_eq!(audio[0]["fields"], json!(["SentenceAudio"]));
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            audio[0]["data"].as_str().expect("data"),
        )
        .expect("base64");
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
    }

    #[test]
    fn no_audio_is_sent_when_nothing_was_heard() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_with_sound(
            Config {
                anki_connect_url: mock.url.clone(),
                ..Config::default()
            },
            &vec![0; 16_000],
        );
        let memory = HoverMemory::default();
        remembered(&memory);

        state.mine_from_hover(&memory, 0, 0).expect("note");

        assert_eq!(mock.requests()[1]["params"]["note"].get("audio"), None);
    }

    #[test]
    fn the_audio_can_be_turned_off_in_the_config() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_with_sound(
            Config {
                anki_connect_url: mock.url.clone(),
                audio_on_cards: false,
                ..Config::default()
            },
            &speech(),
        );
        let memory = HoverMemory::default();
        remembered(&memory);

        state.mine_from_hover(&memory, 0, 0).expect("note");

        assert_eq!(mock.requests()[1]["params"]["note"].get("audio"), None);
    }

    #[test]
    fn the_clip_respects_the_configured_length() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_with_sound(
            Config {
                anki_connect_url: mock.url.clone(),
                audio_seconds: 1,
                ..Config::default()
            },
            &vec![4000; 16_000 * 5],
        );
        let memory = HoverMemory::default();
        remembered(&memory);

        state.mine_from_hover(&memory, 0, 0).expect("note");

        let audio = &mock.requests()[1]["params"]["note"]["audio"];
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            audio[0]["data"].as_str().expect("data"),
        )
        .expect("base64");
        // Exactly 1 s: the padding never reaches past the limit or the end of the sound.
        assert_eq!(bytes.len(), 44 + 16_000 * 2);
    }

    fn profile_for(id: &str, deck: &str, app: Option<&str>, title: Option<&str>) -> Profile {
        Profile {
            id: id.to_string(),
            name: id.to_string(),
            deck_name: deck.to_string(),
            r#match: ProfileMatch {
                source_id: None,
                app_name: app.map(str::to_string),
                window_title: title.map(str::to_string),
            },
            ..crate::config::default_profile()
        }
    }

    fn deck_used_for(identity: crate::pointer::WindowIdentity) -> String {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_with(Config {
            anki_connect_url: mock.url.clone(),
            profiles: vec![
                crate::config::default_profile(),
                profile_for("player", "Anime", Some("mpv"), None),
                profile_for("youtube", "Youtube", Some("Navigator"), Some("YouTube")),
            ],
            ..Config::default()
        });
        let memory = HoverMemory::default();
        remembered(&memory);
        memory.set_with_origin(memory.get().expect("remembered"), None, identity);

        state.mine_from_hover(&memory, 0, 0).expect("note");

        mock.requests()[1]["params"]["note"]["deckName"]
            .as_str()
            .expect("deck")
            .to_string()
    }

    #[test]
    fn the_card_goes_to_the_deck_of_the_application_the_word_was_found_in() {
        let identity = |app: &str, title: &str| crate::pointer::WindowIdentity {
            app_name: Some(app.to_string()),
            title: Some(title.to_string()),
        };

        assert_eq!(deck_used_for(identity("mpv", "episode 1")), "Anime");
        assert_eq!(
            deck_used_for(identity("Navigator", "猫 - YouTube")),
            "Youtube"
        );
        assert_eq!(deck_used_for(identity("Navigator", "Wikipedia")), "Mining");
        assert_eq!(
            deck_used_for(crate::pointer::WindowIdentity::default()),
            "Mining"
        );
    }

    #[test]
    fn duplicates_are_refused_by_default_and_allowed_by_the_policy() {
        let note_options = |policy: DuplicatePolicy| {
            let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
            let state = state_with(Config {
                anki_connect_url: mock.url.clone(),
                duplicate_policy: policy,
                ..Config::default()
            });
            let memory = HoverMemory::default();
            remembered(&memory);
            state.mine_from_hover(&memory, 0, 0).expect("note");
            mock.requests()[1]["params"]["note"]["options"].clone()
        };

        assert_eq!(
            note_options(DuplicatePolicy::Skip),
            json!({ "allowDuplicate": false, "duplicateScope": "deck" })
        );
        assert_eq!(
            note_options(DuplicatePolicy::Allow),
            json!({ "allowDuplicate": true, "duplicateScope": "deck" })
        );
    }

    #[test]
    fn the_settings_start_as_the_config_and_can_be_replaced() {
        let state = state_with(Config::default());
        let mut settings = state.settings();
        assert_eq!(settings, MiningSettings::from_config(&Config::default()));

        settings
            .profiles
            .push(profile_for("player", "Anime", Some("mpv"), None));
        settings.audio_seconds = 4;
        settings.duplicate_policy = DuplicatePolicy::Allow;
        let saved = state.save_settings(settings.clone()).expect("saved");

        assert_eq!(saved, settings);
        assert_eq!(state.settings(), settings);
    }

    #[test]
    fn invalid_settings_are_refused_and_the_old_ones_stay() {
        let state = state_with(Config::default());
        let mut settings = state.settings();
        settings.default_profile_id = "missing".to_string();

        let error = state.save_settings(settings).expect_err("must fail");

        assert!(
            error.contains("defaultProfileId `missing` does not match any profile"),
            "{error}"
        );
        assert_eq!(
            state.settings(),
            MiningSettings::from_config(&Config::default())
        );
    }

    #[test]
    fn new_settings_apply_to_the_next_card() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let state = state_with(Config {
            anki_connect_url: mock.url.clone(),
            ..Config::default()
        });
        let mut settings = state.settings();
        settings.profiles[0].deck_name = "Renamed".to_string();
        state.save_settings(settings).expect("saved");
        let memory = HoverMemory::default();
        remembered(&memory);

        state.mine_from_hover(&memory, 0, 0).expect("note");

        assert_eq!(
            mock.requests()[1]["params"]["note"]["deckName"],
            json!("Renamed")
        );
    }

    #[test]
    fn same_line_ignores_spaces_and_accepts_either_side_containing_the_other() {
        assert!(same_line(
            "昨日、友達と映画を見た。",
            "昨日、友達と 映画を見た。"
        ));
        assert!(same_line("一行目\n二行目", "二行目"));
        assert!(same_line("二行目", "一行目二行目"));
        assert!(!same_line("別の字幕", "一行目"));
        assert!(!same_line("", "一行目"));
        assert!(!same_line("一行目", " "));
    }

    #[test]
    fn a_card_from_an_mpv_subtitle_gets_the_sound_of_that_line() {
        let mock = MockAnki::start(vec![ok(json!(["AnkiMining"])), ok(json!(5))]);
        let audio = crate::audio::AudioBuffer::shared(60);
        let now = Instant::now();
        // 4 s of silence, 2 s of speech (the subtitle), 6 s of louder talk that came after it.
        let mut samples = vec![0_i16; 16_000 * 4];
        samples.extend(vec![4000_i16; 16_000 * 2]);
        samples.extend(vec![9000_i16; 16_000 * 6]);
        audio.lock().expect("audio").push_at(&samples, now);
        let clock = SubtitleClock::default();
        let started = now - std::time::Duration::from_secs(8);
        clock.begin("私は猫が好き", started);
        clock.end(now - std::time::Duration::from_secs(6));
        let state = state_with(Config {
            anki_connect_url: mock.url.clone(),
            ..Config::default()
        })
        .with_audio(audio)
        .with_subtitles(clock);
        let memory = HoverMemory::default();
        remembered(&memory);

        state.mine_from_hover(&memory, 0, 0).expect("note");

        let data = mock.requests()[1]["params"]["note"]["audio"][0]["data"]
            .as_str()
            .expect("data")
            .to_string();
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
            .expect("base64");
        let samples: Vec<i16> = bytes[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes(*pair))
            .collect();
        // The subtitle (2 s) plus 150 ms on each side; only that padding reaches the talk after it.
        assert_eq!(samples.len(), 16_000 * 2 + 2 * 2_400);
        assert_eq!(samples.iter().filter(|sample| **sample == 0).count(), 2_400);
        assert_eq!(
            samples.iter().filter(|sample| **sample == 4000).count(),
            16_000 * 2
        );
        assert_eq!(
            samples.iter().filter(|sample| **sample == 9000).count(),
            2_400
        );
    }
}
