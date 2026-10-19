use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

mod file;

pub use file::ConfigFile;
use thiserror::Error;

use crate::hover::trigger::{TriggerKey, TriggerMode, DEFAULT_DWELL_MS};
use crate::sources::mpv::DEFAULT_SOCKET as DEFAULT_MPV_SOCKET;
use crate::tts::DEFAULT_STYLE_ID;
use crate::types::profile::{default_field_mapping, MiningField, Profile, ProfileMatch};

pub const DEFAULT_ANKI_CONNECT_URL: &str = "http://127.0.0.1:8765";
/// Same port used by Textractor's websocket extension, so existing setups work unchanged.
pub const DEFAULT_WEBSOCKET_PORT: u16 = 6677;
pub const DEFAULT_PROFILE_ID: &str = "default";
pub const DEFAULT_MINING_HOTKEY: &str = "Alt+M";
pub const MIN_DWELL_MS: u64 = 100;
pub const MAX_DWELL_MS: u64 = 3_000;
pub const DEFAULT_DECK_NAME: &str = "Mining";
pub const DEFAULT_MODEL_NAME: &str = "AnkiMining";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to access config file `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("config file `{path}` is not valid JSON: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid config: {}", .0.join("; "))]
    Invalid(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub anki_connect_url: String,
    pub websocket_port: u16,
    /// Path of mpv's IPC socket (`input-ipc-server` in `mpv.conf`); empty turns the mpv source off.
    pub mpv_socket: String,
    /// Global shortcut that mines the word currently selected in the overlay.
    pub mining_hotkey: String,
    /// How the lookup under the mouse cursor starts: hold a key, or let the cursor rest.
    pub hover_mode: TriggerMode,
    /// The key to hold in `hold` mode.
    pub hover_key: TriggerKey,
    /// How long the cursor must rest in `auto` mode, in milliseconds.
    pub hover_dwell_ms: u64,
    /// Whether the lookup under the mouse cursor is on when the app starts.
    pub hover_enabled: bool,
    /// Whether cards get a screenshot of the window the word was found in.
    pub screenshot_on_cards: bool,
    /// Whether cards get the sentence audio (needs `parec`, from PulseAudio or PipeWire).
    pub audio_on_cards: bool,
    /// The longest audio clip put on a card, in seconds.
    pub audio_seconds: u32,
    /// What to do when the card already exists in the deck.
    pub duplicate_policy: DuplicatePolicy,
    /// Whether closing the window keeps the app running in the system tray.
    pub close_to_tray: bool,
    /// Whether cards get the word and the sentence read by a synthesized voice.
    pub tts_on_cards: bool,
    /// The VOICEVOX voice style used for it (see `tts::DEFAULT_STYLE_ID`).
    pub tts_style_id: u32,
    /// Whether the app looks for a newer version on GitHub a little after it starts.
    pub auto_update: bool,
    /// Whether the main window stays above the other windows (otherwise the system decides).
    pub always_on_top: bool,
    pub default_profile_id: String,
    pub profiles: Vec<Profile>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            anki_connect_url: DEFAULT_ANKI_CONNECT_URL.to_string(),
            websocket_port: DEFAULT_WEBSOCKET_PORT,
            mpv_socket: DEFAULT_MPV_SOCKET.to_string(),
            mining_hotkey: DEFAULT_MINING_HOTKEY.to_string(),
            hover_mode: TriggerMode::Hold,
            hover_key: TriggerKey::Shift,
            hover_dwell_ms: DEFAULT_DWELL_MS,
            hover_enabled: true,
            screenshot_on_cards: true,
            audio_on_cards: true,
            audio_seconds: 8,
            duplicate_policy: DuplicatePolicy::Skip,
            close_to_tray: false,
            tts_on_cards: true,
            tts_style_id: DEFAULT_STYLE_ID,
            auto_update: true,
            always_on_top: true,
            default_profile_id: DEFAULT_PROFILE_ID.to_string(),
            profiles: vec![default_profile()],
        }
    }
}

/// The settings about cards that can be changed while the app runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiningSettings {
    pub default_profile_id: String,
    pub profiles: Vec<Profile>,
    pub screenshot_on_cards: bool,
    pub audio_on_cards: bool,
    pub audio_seconds: u32,
    pub duplicate_policy: DuplicatePolicy,
    pub close_to_tray: bool,
    pub tts_on_cards: bool,
    pub auto_update: bool,
    pub always_on_top: bool,
}

impl MiningSettings {
    pub fn from_config(config: &Config) -> MiningSettings {
        MiningSettings {
            default_profile_id: config.default_profile_id.clone(),
            profiles: config.profiles.clone(),
            screenshot_on_cards: config.screenshot_on_cards,
            audio_on_cards: config.audio_on_cards,
            audio_seconds: config.audio_seconds,
            duplicate_policy: config.duplicate_policy,
            close_to_tray: config.close_to_tray,
            tts_on_cards: config.tts_on_cards,
            auto_update: config.auto_update,
            always_on_top: config.always_on_top,
        }
    }

    pub fn apply_to(self, config: &mut Config) {
        config.default_profile_id = self.default_profile_id;
        config.profiles = self.profiles;
        config.screenshot_on_cards = self.screenshot_on_cards;
        config.audio_on_cards = self.audio_on_cards;
        config.audio_seconds = self.audio_seconds;
        config.duplicate_policy = self.duplicate_policy;
        config.close_to_tray = self.close_to_tray;
        config.tts_on_cards = self.tts_on_cards;
        config.auto_update = self.auto_update;
        config.always_on_top = self.always_on_top;
    }
}

/// What happens when the card to add is already in the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DuplicatePolicy {
    /// The card is not added and the user is told.
    Skip,
    /// The card is added again.
    Allow,
}

/// Whether `socket` can be where mpv listens: an absolute path to a socket on Linux; on Windows
/// the name of a named pipe (`mpvsocket`) or the path of one (`\\.\pipe\mpvsocket`).
pub fn is_valid_mpv_socket(socket: &str, windows: bool) -> bool {
    if !windows {
        // A plain text check: `Path::is_absolute` would call this path relative when the program
        // itself runs on Windows (as the tests of the Linux rule do).
        return socket.starts_with('/');
    }
    let bare_name = !socket.contains('/') && !socket.contains('\\');
    bare_name || socket.starts_with(r"\\.\pipe\")
}

pub fn default_profile() -> Profile {
    Profile {
        id: DEFAULT_PROFILE_ID.to_string(),
        name: "Default".to_string(),
        deck_name: DEFAULT_DECK_NAME.to_string(),
        model_name: DEFAULT_MODEL_NAME.to_string(),
        field_mapping: default_field_mapping(),
        r#match: ProfileMatch {
            source_id: None,
            app_name: None,
            window_title: None,
        },
    }
}

impl Config {
    /// Loads and validates the config at `path`. A missing file yields the defaults.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Config::default());
            }
            Err(source) => return Err(io_error(path, source)),
        };

        let mut config: Config =
            serde_json::from_str(&contents).map_err(|source| ConfigError::Parse {
                path: path.display().to_string(),
                source,
            })?;
        config.add_new_fields_to_default_note_type();
        config.validate()?;
        Ok(config)
    }

    /// A config saved by an older version has no mapping for the fields added since; for the
    /// note type the app makes itself they get the default names. The word audio is heard from the
    /// sentence audio's field: an earlier version mapped it to a `WordAudio` field of its own, but
    /// adding a field to a note type makes Anki ask for a full sync with AnkiWeb, so that mapping
    /// is replaced. A note type of the user's own is left as it is.
    fn add_new_fields_to_default_note_type(&mut self) {
        for profile in &mut self.profiles {
            if profile.model_name != DEFAULT_MODEL_NAME {
                continue;
            }
            for field in MiningField::ALL {
                profile
                    .field_mapping
                    .entry(field)
                    .or_insert_with(|| field.default_anki_name().to_string());
            }
            if profile
                .field_mapping
                .get(&MiningField::WordAudio)
                .map(String::as_str)
                == Some("WordAudio")
            {
                profile.field_mapping.insert(
                    MiningField::WordAudio,
                    MiningField::WordAudio.default_anki_name().to_string(),
                );
            }
        }
    }

    /// Validates and atomically writes the config to `path`, creating parent folders.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        self.validate()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
        }

        let serialized =
            serde_json::to_string_pretty(self).map_err(|source| ConfigError::Parse {
                path: path.display().to_string(),
                source,
            })?;
        let temporary_path = path.with_extension("json.tmp");
        fs::write(&temporary_path, serialized)
            .map_err(|source| io_error(&temporary_path, source))?;
        fs::rename(&temporary_path, path).map_err(|source| io_error(path, source))
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn default_profile(&self) -> Option<&Profile> {
        self.profile(&self.default_profile_id)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut problems = Vec::new();

        if !is_http_url(&self.anki_connect_url) {
            problems.push(format!(
                "ankiConnectUrl must start with http:// or https:// (got `{}`)",
                self.anki_connect_url
            ));
        }
        if self.websocket_port == 0 {
            problems.push("websocketPort must be between 1 and 65535".to_string());
        }
        if !self.mpv_socket.is_empty() && !is_valid_mpv_socket(&self.mpv_socket, cfg!(windows)) {
            problems.push(format!(
                "mpvSocket must be {} or empty (got `{}`)",
                if cfg!(windows) {
                    "the name of a pipe (such as mpvsocket) or a pipe path (\\\\.\\pipe\\name)"
                } else {
                    "an absolute path"
                },
                self.mpv_socket
            ));
        }
        if self.mining_hotkey.trim().is_empty() {
            problems.push("miningHotkey must not be empty".to_string());
        }
        if !(MIN_DWELL_MS..=MAX_DWELL_MS).contains(&self.hover_dwell_ms) {
            problems.push(format!(
                "hoverDwellMs must be between {MIN_DWELL_MS} and {MAX_DWELL_MS} (got {})",
                self.hover_dwell_ms
            ));
        }
        if self.profiles.is_empty() {
            problems.push("at least one profile is required".to_string());
        }
        problems.extend(self.profile_problems());
        if !self
            .profiles
            .iter()
            .any(|profile| profile.id == self.default_profile_id)
        {
            problems.push(format!(
                "defaultProfileId `{}` does not match any profile",
                self.default_profile_id
            ));
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Invalid(problems))
        }
    }

    fn profile_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut seen_ids = HashSet::new();

        for profile in &self.profiles {
            if profile.id.trim().is_empty() {
                problems.push("profile id must not be empty".to_string());
            } else if !seen_ids.insert(profile.id.as_str()) {
                problems.push(format!("duplicate profile id `{}`", profile.id));
            }
            if profile.deck_name.trim().is_empty() {
                problems.push(format!("profile `{}` has an empty deckName", profile.id));
            }
            if profile.model_name.trim().is_empty() {
                problems.push(format!("profile `{}` has an empty modelName", profile.id));
            }
        }

        problems
    }
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn io_error(path: &Path, source: io::Error) -> ConfigError {
    ConfigError::Io {
        path: path.display().to_string(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    /// A valid mpv socket other than the default, on this system.
    #[cfg(unix)]
    const OTHER_SOCKET: &str = "/run/user/1000/mpv.sock";
    /// A valid mpv socket other than the default, on this system.
    #[cfg(windows)]
    const OTHER_SOCKET: &str = r"\\.\pipe\other";

    fn invalid_messages(config: &Config) -> Vec<String> {
        match config.validate() {
            Err(ConfigError::Invalid(messages)) => messages,
            other => panic!("expected an Invalid error, got {other:?}"),
        }
    }

    #[test]
    fn default_config_uses_the_documented_values_and_is_valid() {
        let config = Config::default();

        assert_eq!(config.anki_connect_url, "http://127.0.0.1:8765");
        assert_eq!(config.websocket_port, 6677);
        assert_eq!(config.mpv_socket, crate::sources::mpv::DEFAULT_SOCKET);
        assert_eq!(config.mining_hotkey, "Alt+M");
        assert_eq!(config.hover_mode, TriggerMode::Hold);
        assert_eq!(config.hover_key, TriggerKey::Shift);
        assert_eq!(config.hover_dwell_ms, 300);
        assert!(config.hover_enabled);
        assert!(config.screenshot_on_cards);
        assert!(config.audio_on_cards);
        assert_eq!(config.audio_seconds, 8);
        assert_eq!(config.duplicate_policy, DuplicatePolicy::Skip);
        assert!(!config.close_to_tray);
        assert!(config.tts_on_cards);
        assert!(config.auto_update);
        assert!(config.always_on_top);
        assert_eq!(config.tts_style_id, 8);
        assert_eq!(config.default_profile_id, "default");
        assert_eq!(config.profiles, vec![default_profile()]);
        assert_eq!(config.profiles[0].deck_name, "Mining");
        assert_eq!(config.profiles[0].model_name, "AnkiMining");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn default_config_serializes_with_camel_case_keys() {
        let value = serde_json::to_value(Config::default()).expect("config must serialize");

        assert_eq!(value["ankiConnectUrl"], json!("http://127.0.0.1:8765"));
        assert_eq!(value["websocketPort"], json!(6677));
        assert_eq!(
            value["mpvSocket"],
            json!(crate::sources::mpv::DEFAULT_SOCKET)
        );
        assert_eq!(value["miningHotkey"], json!("Alt+M"));
        assert_eq!(value["hoverMode"], json!("hold"));
        assert_eq!(value["hoverKey"], json!("shift"));
        assert_eq!(value["hoverDwellMs"], json!(300));
        assert_eq!(value["hoverEnabled"], json!(true));
        assert_eq!(value["screenshotOnCards"], json!(true));
        assert_eq!(value["audioOnCards"], json!(true));
        assert_eq!(value["audioSeconds"], json!(8));
        assert_eq!(value["duplicatePolicy"], json!("skip"));
        assert_eq!(value["closeToTray"], json!(false));
        assert_eq!(value["ttsOnCards"], json!(true));
        assert_eq!(value["autoUpdate"], json!(true));
        assert_eq!(value["alwaysOnTop"], json!(true));
        assert_eq!(value["ttsStyleId"], json!(8));
        assert_eq!(value["defaultProfileId"], json!("default"));
        assert_eq!(value["profiles"][0]["deckName"], json!("Mining"));
    }

    #[test]
    fn load_returns_defaults_when_the_file_does_not_exist() {
        let directory = tempdir().expect("temp dir");

        let config = Config::load(&directory.path().join("missing.json")).expect("load");

        assert_eq!(config, Config::default());
    }

    #[test]
    fn load_fills_missing_fields_with_defaults() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        fs::write(&path, r#"{ "websocketPort": 7000 }"#).expect("write");

        let config = Config::load(&path).expect("load");

        assert_eq!(
            config,
            Config {
                websocket_port: 7000,
                ..Config::default()
            }
        );
    }

    #[test]
    fn load_fails_with_a_parse_error_on_malformed_json() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        fs::write(&path, "{ not json").expect("write");

        let error = Config::load(&path).expect_err("must fail");

        assert!(matches!(error, ConfigError::Parse { .. }));
        assert!(error.to_string().contains("is not valid JSON"));
    }

    #[test]
    fn load_fails_with_an_io_error_when_the_path_is_a_directory() {
        let directory = tempdir().expect("temp dir");

        let error = Config::load(directory.path()).expect_err("must fail");

        assert!(matches!(error, ConfigError::Io { .. }));
    }

    #[test]
    fn load_rejects_a_file_that_parses_but_is_invalid() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        fs::write(&path, r#"{ "websocketPort": 0 }"#).expect("write");

        let error = Config::load(&path).expect_err("must fail");

        assert!(matches!(error, ConfigError::Invalid(_)));
    }

    #[test]
    fn save_creates_parent_folders_and_round_trips() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("nested").join("config.json");
        let config = Config {
            websocket_port: 7001,
            ..Config::default()
        };

        config.save(&path).expect("save");
        let loaded = Config::load(&path).expect("load");

        assert_eq!(loaded, config);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn save_overwrites_an_existing_file() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        Config::default().save(&path).expect("first save");
        let updated = Config {
            websocket_port: 7002,
            ..Config::default()
        };

        updated.save(&path).expect("second save");

        assert_eq!(Config::load(&path).expect("load"), updated);
    }

    #[test]
    fn save_refuses_an_invalid_config_and_leaves_no_file() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        let config = Config {
            websocket_port: 0,
            ..Config::default()
        };

        let error = config.save(&path).expect_err("must fail");

        assert!(matches!(error, ConfigError::Invalid(_)));
        assert!(!path.exists());
    }

    #[test]
    fn save_fails_with_an_io_error_when_the_parent_is_a_file() {
        let directory = tempdir().expect("temp dir");
        let blocker = directory.path().join("blocker");
        fs::write(&blocker, "x").expect("write");

        let error = Config::default()
            .save(&blocker.join("config.json"))
            .expect_err("must fail");

        assert!(matches!(error, ConfigError::Io { .. }));
    }

    #[test]
    fn validate_rejects_a_non_http_anki_connect_url() {
        let config = Config {
            anki_connect_url: "127.0.0.1:8765".to_string(),
            ..Config::default()
        };

        assert_eq!(
            invalid_messages(&config),
            vec!["ankiConnectUrl must start with http:// or https:// (got `127.0.0.1:8765`)"]
        );
    }

    #[test]
    fn validate_accepts_https_urls() {
        let config = Config {
            anki_connect_url: "https://example.test:8765".to_string(),
            ..Config::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_rejects_port_zero() {
        let config = Config {
            websocket_port: 0,
            ..Config::default()
        };

        assert_eq!(
            invalid_messages(&config),
            vec!["websocketPort must be between 1 and 65535"]
        );
    }

    #[test]
    fn validate_rejects_a_blank_mining_hotkey() {
        let config = Config {
            mining_hotkey: "  ".to_string(),
            ..Config::default()
        };

        assert_eq!(
            invalid_messages(&config),
            vec!["miningHotkey must not be empty"]
        );
    }

    #[test]
    fn validate_rejects_an_empty_profile_list() {
        let config = Config {
            profiles: vec![],
            ..Config::default()
        };

        assert_eq!(
            invalid_messages(&config),
            vec![
                "at least one profile is required",
                "defaultProfileId `default` does not match any profile"
            ]
        );
    }

    #[test]
    fn validate_rejects_duplicate_and_empty_profile_ids() {
        let duplicate = default_profile();
        let blank = Profile {
            id: "  ".to_string(),
            ..default_profile()
        };
        let config = Config {
            profiles: vec![default_profile(), duplicate, blank],
            ..Config::default()
        };

        assert_eq!(
            invalid_messages(&config),
            vec![
                "duplicate profile id `default`",
                "profile id must not be empty"
            ]
        );
    }

    #[test]
    fn validate_rejects_empty_deck_and_model_names() {
        let profile = Profile {
            deck_name: " ".to_string(),
            model_name: String::new(),
            ..default_profile()
        };
        let config = Config {
            profiles: vec![profile],
            ..Config::default()
        };

        assert_eq!(
            invalid_messages(&config),
            vec![
                "profile `default` has an empty deckName",
                "profile `default` has an empty modelName"
            ]
        );
    }

    #[test]
    fn validate_rejects_an_unknown_default_profile_id() {
        let config = Config {
            default_profile_id: "ghost".to_string(),
            ..Config::default()
        };

        assert_eq!(
            invalid_messages(&config),
            vec!["defaultProfileId `ghost` does not match any profile"]
        );
    }

    #[test]
    fn validate_reports_every_problem_at_once() {
        let config = Config {
            anki_connect_url: "ftp://x".to_string(),
            websocket_port: 0,
            default_profile_id: "ghost".to_string(),
            ..Config::default()
        };

        assert_eq!(invalid_messages(&config).len(), 3);
    }

    #[test]
    fn invalid_error_joins_the_messages() {
        let error = ConfigError::Invalid(vec!["a".to_string(), "b".to_string()]);

        assert_eq!(error.to_string(), "invalid config: a; b");
    }

    #[test]
    fn profile_finds_a_profile_by_id() {
        let games = Profile {
            id: "games".to_string(),
            name: "Games".to_string(),
            deck_name: "Games".to_string(),
            ..default_profile()
        };
        let config = Config {
            profiles: vec![default_profile(), games.clone()],
            ..Config::default()
        };

        assert_eq!(config.profile("games"), Some(&games));
        assert_eq!(config.profile("default"), Some(&default_profile()));
        assert_eq!(config.profile("ghost"), None);
    }

    #[test]
    fn default_profile_follows_default_profile_id() {
        let games = Profile {
            id: "games".to_string(),
            ..default_profile()
        };
        let config = Config {
            default_profile_id: "games".to_string(),
            profiles: vec![default_profile(), games.clone()],
            ..Config::default()
        };

        assert_eq!(config.default_profile(), Some(&games));
        assert_eq!(
            Config::default().default_profile(),
            Some(&default_profile())
        );
    }

    #[test]
    fn hover_settings_can_be_loaded_from_a_file() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        fs::write(
            &path,
            r#"{ "hoverMode": "auto", "hoverKey": "alt", "hoverDwellMs": 450, "hoverEnabled": false }"#,
        )
        .expect("write");

        let config = Config::load(&path).expect("load");

        assert_eq!(config.hover_mode, TriggerMode::Auto);
        assert_eq!(config.hover_key, TriggerKey::Alt);
        assert_eq!(config.hover_dwell_ms, 450);
        assert!(!config.hover_enabled);
    }

    #[test]
    fn an_unknown_hover_mode_is_a_parse_error() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        fs::write(&path, r#"{ "hoverMode": "telepathy" }"#).expect("write");

        assert!(matches!(
            Config::load(&path),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn validate_limits_the_dwell_time() {
        for dwell in [MIN_DWELL_MS - 1, MAX_DWELL_MS + 1, 0] {
            let config = Config {
                hover_dwell_ms: dwell,
                ..Config::default()
            };

            assert_eq!(
                invalid_messages(&config),
                vec![format!(
                    "hoverDwellMs must be between 100 and 3000 (got {dwell})"
                )]
            );
        }
        for dwell in [MIN_DWELL_MS, MAX_DWELL_MS] {
            let config = Config {
                hover_dwell_ms: dwell,
                ..Config::default()
            };
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn the_mpv_socket_can_be_changed_or_turned_off() {
        let custom = Config {
            mpv_socket: OTHER_SOCKET.to_string(),
            ..Config::default()
        };
        let off = Config {
            mpv_socket: String::new(),
            ..Config::default()
        };

        assert!(custom.validate().is_ok());
        assert!(off.validate().is_ok());
    }

    #[test]
    fn validate_rejects_a_mpv_socket_that_is_not_valid_on_this_system() {
        let wrong = if cfg!(windows) {
            r"some\folder"
        } else {
            "mpvsocket"
        };
        let config = Config {
            mpv_socket: wrong.to_string(),
            ..Config::default()
        };

        let messages = invalid_messages(&config);

        assert_eq!(messages.len(), 1);
        assert!(
            messages[0].starts_with("mpvSocket must be "),
            "{messages:?}"
        );
        assert!(
            messages[0].ends_with(&format!("or empty (got `{wrong}`)")),
            "{messages:?}"
        );
    }

    #[test]
    fn a_mpv_socket_is_an_absolute_path_on_linux_and_a_pipe_on_windows() {
        assert!(is_valid_mpv_socket("/tmp/mpvsocket", false));
        assert!(!is_valid_mpv_socket("mpvsocket", false));
        assert!(!is_valid_mpv_socket("relative/path", false));

        assert!(is_valid_mpv_socket("mpvsocket", true));
        assert!(is_valid_mpv_socket(r"\\.\pipe\mpvsocket", true));
        assert!(!is_valid_mpv_socket(r"some\folder", true));
        assert!(!is_valid_mpv_socket("C:/x/sock", true));
    }

    #[test]
    fn the_mpv_socket_is_read_from_the_file() {
        let directory = tempdir().expect("temp dir");
        let path = directory.path().join("config.json");
        fs::write(&path, json!({ "mpvSocket": OTHER_SOCKET }).to_string()).expect("write");

        assert_eq!(Config::load(&path).expect("load").mpv_socket, OTHER_SOCKET);
    }

    #[test]
    fn mining_settings_round_trip_through_the_config_with_camel_case_keys() {
        let mut config = Config::default();
        let mut settings = MiningSettings::from_config(&config);
        settings.audio_seconds = 3;
        settings.duplicate_policy = DuplicatePolicy::Allow;
        settings.screenshot_on_cards = false;
        settings.close_to_tray = true;
        settings.tts_on_cards = false;
        settings.auto_update = false;
        settings.always_on_top = false;

        settings.clone().apply_to(&mut config);

        assert_eq!(MiningSettings::from_config(&config), settings);
        assert_eq!(config.audio_seconds, 3);
        let value = serde_json::to_value(&settings).expect("json");
        assert_eq!(value["audioSeconds"], json!(3));
        assert_eq!(value["duplicatePolicy"], json!("allow"));
        assert_eq!(value["screenshotOnCards"], json!(false));
        assert_eq!(value["closeToTray"], json!(true));
        assert_eq!(value["ttsOnCards"], json!(false));
        assert_eq!(value["autoUpdate"], json!(false));
        assert_eq!(value["alwaysOnTop"], json!(false));
        assert!(!config.auto_update);
        assert!(!config.always_on_top);
        assert!(!config.tts_on_cards);
        assert!(config.close_to_tray);
        assert_eq!(value["defaultProfileId"], json!("default"));
        assert_eq!(value["profiles"][0]["match"]["windowTitle"], json!(null));
    }

    #[test]
    fn an_old_config_gets_the_new_fields_only_for_the_default_note_type() {
        let directory = tempfile::tempdir().expect("dir");
        let path = directory.path().join("config.json");
        let mut old_mapping = default_field_mapping();
        old_mapping.remove(&MiningField::WordAudio);
        let own = Profile {
            id: "own".to_string(),
            model_name: "Basic".to_string(),
            field_mapping: old_mapping.clone(),
            ..default_profile()
        };
        let default = Profile {
            field_mapping: old_mapping.clone(),
            ..default_profile()
        };
        let config = Config {
            profiles: vec![default, own],
            ..Config::default()
        };
        std::fs::write(&path, serde_json::to_string(&config).expect("json")).expect("write");

        let loaded = Config::load(&path).expect("load");

        assert_eq!(
            loaded.profiles[0]
                .field_mapping
                .get(&MiningField::WordAudio),
            Some(&"SentenceAudio".to_string())
        );
        assert_eq!(loaded.profiles[1].field_mapping, old_mapping);
    }

    #[test]
    fn a_field_the_user_switched_off_stays_off() {
        let directory = tempfile::tempdir().expect("dir");
        let path = directory.path().join("config.json");
        let mut mapping = default_field_mapping();
        mapping.insert(MiningField::WordAudio, String::new());
        let config = Config {
            profiles: vec![Profile {
                field_mapping: mapping,
                ..default_profile()
            }],
            ..Config::default()
        };
        std::fs::write(&path, serde_json::to_string(&config).expect("json")).expect("write");

        let loaded = Config::load(&path).expect("load");

        assert_eq!(
            loaded.profiles[0]
                .field_mapping
                .get(&MiningField::WordAudio),
            Some(&String::new())
        );
    }

    #[test]
    fn a_word_audio_field_of_its_own_from_an_earlier_version_is_mapped_to_the_sentence_audio_field()
    {
        let directory = tempfile::tempdir().expect("dir");
        let path = directory.path().join("config.json");
        let mut mapping = default_field_mapping();
        mapping.insert(MiningField::WordAudio, "WordAudio".to_string());
        let own = Profile {
            id: "own".to_string(),
            model_name: "Basic".to_string(),
            field_mapping: mapping.clone(),
            ..default_profile()
        };
        let default = Profile {
            field_mapping: mapping.clone(),
            ..default_profile()
        };
        let config = Config {
            profiles: vec![default, own],
            ..Config::default()
        };
        std::fs::write(&path, serde_json::to_string(&config).expect("json")).expect("write");

        let loaded = Config::load(&path).expect("load");

        assert_eq!(
            loaded.profiles[0]
                .field_mapping
                .get(&MiningField::WordAudio),
            Some(&"SentenceAudio".to_string())
        );
        // A note type of the user's own keeps what the user mapped.
        assert_eq!(
            loaded.profiles[1]
                .field_mapping
                .get(&MiningField::WordAudio),
            Some(&"WordAudio".to_string())
        );
    }

    #[test]
    fn the_word_audio_shares_the_field_of_the_sentence_audio_by_default() {
        let mapping = default_field_mapping();

        assert_eq!(
            mapping.get(&MiningField::WordAudio),
            mapping.get(&MiningField::SentenceAudio)
        );
    }
}
