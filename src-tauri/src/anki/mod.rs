pub mod model;
#[cfg(test)]
pub(crate) mod test_support;

use std::thread;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use thiserror::Error;

use crate::types::anki::AnkiNote;

pub use model::{default_mining_model, CreateModel};

const API_VERSION: u8 = 6;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_millis(200);
const DEFAULT_RETRIES: u32 = 2;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AnkiError {
    #[error("cannot reach AnkiConnect at {url}: {reason} (is Anki running with the AnkiConnect add-on?)")]
    Connection { url: String, reason: String },
    #[error("AnkiConnect answered with HTTP status {0}")]
    Http(u16),
    #[error("unexpected AnkiConnect response: {0}")]
    InvalidResponse(String),
    #[error("the note is a duplicate")]
    Duplicate,
    #[error("AnkiConnect error: {0}")]
    Api(String),
}

#[derive(Deserialize)]
struct Envelope {
    result: Value,
    error: Option<String>,
}

/// Typed client for the AnkiConnect HTTP API (version 6).
pub struct AnkiClient {
    url: String,
    agent: ureq::Agent,
    retries: u32,
}

impl AnkiClient {
    pub fn new(url: &str) -> AnkiClient {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build();
        AnkiClient {
            url: url.to_string(),
            agent: ureq::Agent::new_with_config(config),
            retries: DEFAULT_RETRIES,
        }
    }

    /// How many times a request is repeated when Anki cannot be reached.
    pub fn with_retries(mut self, retries: u32) -> AnkiClient {
        self.retries = retries;
        self
    }

    pub fn version(&self) -> Result<u32, AnkiError> {
        self.call("version", Value::Null)
    }

    pub fn deck_names(&self) -> Result<Vec<String>, AnkiError> {
        self.call("deckNames", Value::Null)
    }

    pub fn model_names(&self) -> Result<Vec<String>, AnkiError> {
        self.call("modelNames", Value::Null)
    }

    pub fn create_model(&self, model: &CreateModel) -> Result<(), AnkiError> {
        let _: Value = self.call("createModel", to_value(model)?)?;
        Ok(())
    }

    /// Creates the note type if no model with that name exists. Returns true when it was created.
    pub fn ensure_model(&self, model: &CreateModel) -> Result<bool, AnkiError> {
        if self.model_names()?.contains(&model.model_name) {
            return Ok(false);
        }
        self.create_model(model)?;
        Ok(true)
    }

    /// For each search query, whether any note matches it (one request for all of them).
    pub fn notes_exist(&self, queries: &[String]) -> Result<Vec<bool>, AnkiError> {
        if queries.is_empty() {
            return Ok(Vec::new());
        }
        let actions: Vec<Value> = queries
            .iter()
            .map(|query| json!({ "action": "findNotes", "params": { "query": query } }))
            .collect();
        let answers: Vec<Value> = self.call("multi", json!({ "actions": actions }))?;
        if answers.len() != queries.len() {
            return Err(AnkiError::InvalidResponse(format!(
                "multi: {} answers for {} queries",
                answers.len(),
                queries.len()
            )));
        }
        Ok(answers.iter().map(found_any).collect())
    }

    /// Stores base64 `data` in Anki's media folder and returns the name it was stored under.
    pub fn store_media_file(&self, filename: &str, data: &str) -> Result<String, AnkiError> {
        self.call(
            "storeMediaFile",
            json!({ "filename": filename, "data": data }),
        )
    }

    /// For each note, whether it could be added (false for duplicates and invalid notes).
    pub fn can_add_notes(&self, notes: &[AnkiNote]) -> Result<Vec<bool>, AnkiError> {
        let notes = notes
            .iter()
            .map(note_params)
            .collect::<Result<Vec<_>, _>>()?;
        self.call("canAddNotes", json!({ "notes": notes }))
    }

    /// Adds a note and returns its id. Duplicates are rejected with [`AnkiError::Duplicate`].
    pub fn add_note(&self, note: &AnkiNote) -> Result<u64, AnkiError> {
        self.add_note_with(note, false)
    }

    /// Adds a note and returns its id; with `allow_duplicate` a note that already exists in the
    /// deck is added again.
    pub fn add_note_with(&self, note: &AnkiNote, allow_duplicate: bool) -> Result<u64, AnkiError> {
        let mut params = note_params(note)?;
        params["options"] = json!({ "allowDuplicate": allow_duplicate, "duplicateScope": "deck" });

        let id: Option<u64> = self.call("addNote", json!({ "note": params }))?;
        id.ok_or_else(|| AnkiError::InvalidResponse("addNote returned no note id".to_string()))
    }

    fn call<T: DeserializeOwned>(&self, action: &str, params: Value) -> Result<T, AnkiError> {
        let mut request = json!({ "action": action, "version": API_VERSION });
        if !params.is_null() {
            request["params"] = params;
        }

        let response = self.send_with_retries(&request)?;
        if let Some(message) = response.error {
            return Err(classify_api_error(message));
        }
        serde_json::from_value(response.result)
            .map_err(|error| AnkiError::InvalidResponse(format!("{action}: {error}")))
    }

    fn send_with_retries(&self, request: &Value) -> Result<Envelope, AnkiError> {
        let mut attempt = 0;
        loop {
            match self.send(request) {
                Err(AnkiError::Connection { .. }) if attempt < self.retries => {
                    attempt += 1;
                    thread::sleep(RETRY_DELAY);
                }
                outcome => return outcome,
            }
        }
    }

    fn send(&self, request: &Value) -> Result<Envelope, AnkiError> {
        let mut response = self
            .agent
            .post(&self.url)
            .send_json(request)
            .map_err(|error| match error {
                ureq::Error::StatusCode(status) => AnkiError::Http(status),
                other => AnkiError::Connection {
                    url: self.url.clone(),
                    reason: other.to_string(),
                },
            })?;
        response
            .body_mut()
            .read_json::<Envelope>()
            .map_err(|error| AnkiError::InvalidResponse(error.to_string()))
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, AnkiError> {
    serde_json::to_value(value).map_err(|error| AnkiError::InvalidResponse(error.to_string()))
}

/// The note as AnkiConnect expects it; empty media lists are left out.
fn note_params(note: &AnkiNote) -> Result<Value, AnkiError> {
    let mut params = to_value(note)?;
    if let Some(object) = params.as_object_mut() {
        for key in ["audio", "picture"] {
            if object
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                object.remove(key);
            }
        }
    }
    Ok(params)
}

fn classify_api_error(message: String) -> AnkiError {
    if message.to_lowercase().contains("duplicate") {
        AnkiError::Duplicate
    } else {
        AnkiError::Api(message)
    }
}

/// Escapes the characters that have a meaning in an Anki search inside a quoted text.
fn escape_search(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '\\' | '"' | '*' | '_' | ':') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// The Anki search for the notes of `deck` whose `field` is exactly `word`.
pub fn known_word_query(deck: &str, field: &str, word: &str) -> String {
    format!(
        "\"deck:{}\" \"{}:{}\"",
        escape_search(deck),
        escape_search(field),
        escape_search(word)
    )
}

/// Whether one answer of `multi` to `findNotes` lists a note: the answer is the list of ids, or an
/// object `{ result, error }` holding it; an error counts as no note.
fn found_any(answer: &Value) -> bool {
    match answer {
        Value::Array(ids) => !ids.is_empty(),
        Value::Object(map) => {
            map.get("error").is_none_or(Value::is_null)
                && map
                    .get("result")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| !ids.is_empty())
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use std::net::TcpListener;

    use super::test_support::{failure, ok, read_request_body, write_reply, MockAnki, Reply};
    use super::*;
    use crate::types::anki::AnkiMedia;

    fn client(mock: &MockAnki) -> AnkiClient {
        AnkiClient::new(&mock.url).with_retries(0)
    }

    fn sample_note() -> AnkiNote {
        AnkiNote {
            deck_name: "Youtube".to_string(),
            model_name: "AnkiMining".to_string(),
            fields: BTreeMap::from([
                ("Expression".to_string(), "猫".to_string()),
                ("Sentence".to_string(), "<b>猫</b>が好きです".to_string()),
            ]),
            tags: vec!["anki-mining".to_string()],
            audio: vec![],
            picture: vec![],
        }
    }

    fn sample_note_json() -> Value {
        json!({
            "deckName": "Youtube",
            "modelName": "AnkiMining",
            "fields": { "Expression": "猫", "Sentence": "<b>猫</b>が好きです" },
            "tags": ["anki-mining"]
        })
    }

    #[test]
    fn version_sends_the_action_without_params() {
        let mock = MockAnki::start(vec![ok(json!(6))]);

        let version = client(&mock).version().expect("version");

        assert_eq!(version, 6);
        assert_eq!(
            mock.requests(),
            vec![json!({ "action": "version", "version": 6 })]
        );
    }

    #[test]
    fn lists_deck_and_model_names() {
        let mock = MockAnki::start(vec![
            ok(json!(["Default", "Youtube"])),
            ok(json!(["Basic"])),
        ]);
        let anki = client(&mock);

        assert_eq!(
            anki.deck_names().expect("decks"),
            vec!["Default", "Youtube"]
        );
        assert_eq!(anki.model_names().expect("models"), vec!["Basic"]);
        assert_eq!(
            mock.requests(),
            vec![
                json!({ "action": "deckNames", "version": 6 }),
                json!({ "action": "modelNames", "version": 6 }),
            ]
        );
    }

    #[test]
    fn add_note_sends_the_exact_note_and_duplicate_options_and_returns_the_id() {
        let mock = MockAnki::start(vec![ok(json!(1_496_198_395_707_u64))]);

        let id = client(&mock).add_note(&sample_note()).expect("add");

        assert_eq!(id, 1_496_198_395_707);
        assert_eq!(
            mock.requests(),
            vec![json!({
                "action": "addNote",
                "version": 6,
                "params": {
                    "note": {
                        "deckName": "Youtube",
                        "modelName": "AnkiMining",
                        "fields": { "Expression": "猫", "Sentence": "<b>猫</b>が好きです" },
                        "tags": ["anki-mining"],
                        "options": { "allowDuplicate": false, "duplicateScope": "deck" }
                    }
                }
            })]
        );
    }

    #[test]
    fn add_note_includes_audio_and_picture_when_present() {
        let mock = MockAnki::start(vec![ok(json!(7))]);
        let note = AnkiNote {
            audio: vec![AnkiMedia {
                filename: "clip.mp3".to_string(),
                data: "AAEC".to_string(),
                fields: vec!["SentenceAudio".to_string()],
            }],
            picture: vec![AnkiMedia {
                filename: "shot.jpg".to_string(),
                data: "AwQF".to_string(),
                fields: vec!["Picture".to_string()],
            }],
            ..sample_note()
        };

        client(&mock).add_note(&note).expect("add");

        let sent = &mock.requests()[0]["params"]["note"];
        assert_eq!(
            sent["audio"],
            json!([{ "filename": "clip.mp3", "data": "AAEC", "fields": ["SentenceAudio"] }])
        );
        assert_eq!(
            sent["picture"],
            json!([{ "filename": "shot.jpg", "data": "AwQF", "fields": ["Picture"] }])
        );
    }

    #[test]
    fn add_note_can_allow_duplicates() {
        let mock = MockAnki::start(vec![ok(json!(9))]);

        let id = client(&mock)
            .add_note_with(&sample_note(), true)
            .expect("add");

        assert_eq!(id, 9);
        assert_eq!(
            mock.requests()[0]["params"]["note"]["options"],
            json!({ "allowDuplicate": true, "duplicateScope": "deck" })
        );
    }

    #[test]
    fn add_note_reports_duplicates() {
        let mock = MockAnki::start(vec![failure(
            "cannot create note because it is a duplicate",
        )]);

        let error = client(&mock)
            .add_note(&sample_note())
            .expect_err("must fail");

        assert_eq!(error, AnkiError::Duplicate);
    }

    #[test]
    fn add_note_reports_other_api_errors() {
        let mock = MockAnki::start(vec![failure("deck was not found: Youtube")]);

        let error = client(&mock)
            .add_note(&sample_note())
            .expect_err("must fail");

        assert_eq!(
            error,
            AnkiError::Api("deck was not found: Youtube".to_string())
        );
        assert_eq!(
            error.to_string(),
            "AnkiConnect error: deck was not found: Youtube"
        );
    }

    #[test]
    fn add_note_rejects_a_null_id() {
        let mock = MockAnki::start(vec![ok(Value::Null)]);

        let error = client(&mock)
            .add_note(&sample_note())
            .expect_err("must fail");

        assert_eq!(
            error,
            AnkiError::InvalidResponse("addNote returned no note id".to_string())
        );
    }

    #[test]
    fn can_add_notes_sends_every_note_and_returns_the_flags() {
        let mock = MockAnki::start(vec![ok(json!([true, false]))]);
        let other = AnkiNote {
            deck_name: "Games".to_string(),
            ..sample_note()
        };

        let flags = client(&mock)
            .can_add_notes(&[sample_note(), other])
            .expect("can add");

        assert_eq!(flags, vec![true, false]);
        assert_eq!(
            mock.requests(),
            vec![json!({
                "action": "canAddNotes",
                "version": 6,
                "params": {
                    "notes": [
                        sample_note_json(),
                        {
                            "deckName": "Games",
                            "modelName": "AnkiMining",
                            "fields": { "Expression": "猫", "Sentence": "<b>猫</b>が好きです" },
                            "tags": ["anki-mining"]
                        }
                    ]
                }
            })]
        );
    }

    #[test]
    fn store_media_file_sends_filename_and_data_and_returns_the_stored_name() {
        let mock = MockAnki::start(vec![ok(json!("clip_1.mp3"))]);

        let stored = client(&mock)
            .store_media_file("clip.mp3", "AAEC")
            .expect("store");

        assert_eq!(stored, "clip_1.mp3");
        assert_eq!(
            mock.requests(),
            vec![json!({
                "action": "storeMediaFile",
                "version": 6,
                "params": { "filename": "clip.mp3", "data": "AAEC" }
            })]
        );
    }

    #[test]
    fn create_model_sends_the_whole_model() {
        let mock = MockAnki::start(vec![ok(json!({ "id": 1, "name": "AnkiMining" }))]);
        let model = default_mining_model();

        client(&mock).create_model(&model).expect("create");

        assert_eq!(
            mock.requests(),
            vec![json!({
                "action": "createModel",
                "version": 6,
                "params": serde_json::to_value(&model).expect("model json")
            })]
        );
    }

    #[test]
    fn ensure_model_creates_the_model_only_when_it_is_missing() {
        let mock = MockAnki::start(vec![
            ok(json!(["Basic"])),
            ok(json!({})),
            ok(json!(["AnkiMining"])),
        ]);
        let anki = client(&mock);
        let model = default_mining_model();

        assert_eq!(anki.ensure_model(&model), Ok(true));
        assert_eq!(anki.ensure_model(&model), Ok(false));

        let actions: Vec<String> = mock
            .requests()
            .iter()
            .map(|request| request["action"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(actions, vec!["modelNames", "createModel", "modelNames"]);
    }

    #[test]
    fn reports_http_error_statuses() {
        let mock = MockAnki::start(vec![Reply::Status(500)]);

        assert_eq!(client(&mock).version(), Err(AnkiError::Http(500)));
    }

    #[test]
    fn reports_a_response_that_is_not_the_expected_envelope() {
        let mock = MockAnki::start(vec![Reply::Body("not json".to_string())]);

        let error = client(&mock).version().expect_err("must fail");

        assert!(matches!(error, AnkiError::InvalidResponse(_)));
    }

    #[test]
    fn reports_a_result_of_the_wrong_type() {
        let mock = MockAnki::start(vec![ok(json!("six"))]);

        let error = client(&mock).version().expect_err("must fail");

        assert!(
            matches!(error, AnkiError::InvalidResponse(ref message) if message.starts_with("version:"))
        );
    }

    #[test]
    fn reports_connection_failures_with_the_url() {
        let unused = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", unused.local_addr().expect("address"));
        drop(unused);

        let error = AnkiClient::new(&url)
            .with_retries(0)
            .version()
            .expect_err("must fail");

        assert!(
            matches!(error, AnkiError::Connection { url: ref failed_url, .. } if *failed_url == url)
        );
        assert!(error.to_string().contains("is Anki running"));
    }

    #[test]
    fn retries_when_anki_cannot_be_reached_and_then_succeeds() {
        // The mock only starts listening after the first attempt has already failed.
        let reserved = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = reserved.local_addr().expect("address");
        drop(reserved);
        let url = format!("http://{address}");
        let starter = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            let listener = TcpListener::bind(address).expect("rebind");
            let (stream, _) = listener.accept().expect("accept");
            let _ = read_request_body(&stream);
            write_reply(stream, &ok(json!(6)));
        });

        let version = AnkiClient::new(&url).with_retries(5).version();

        starter.join().expect("starter thread");
        assert_eq!(version, Ok(6));
    }

    #[test]
    fn does_not_retry_api_errors() {
        let mock = MockAnki::start(vec![failure("boom"), ok(json!(6))]);

        let result = AnkiClient::new(&mock.url).with_retries(3).version();

        assert_eq!(result, Err(AnkiError::Api("boom".to_string())));
        assert_eq!(mock.requests().len(), 1);
    }

    #[test]
    fn classify_api_error_detects_duplicates_case_insensitively() {
        assert_eq!(
            classify_api_error(
                "Cannot create note because it is a DUPLICATE of another".to_string()
            ),
            AnkiError::Duplicate
        );
        assert_eq!(
            classify_api_error("collection is not available".to_string()),
            AnkiError::Api("collection is not available".to_string())
        );
    }

    #[test]
    fn note_params_drops_empty_media_lists_only() {
        let with_audio = AnkiNote {
            audio: vec![AnkiMedia {
                filename: "a.mp3".to_string(),
                data: "AA".to_string(),
                fields: vec!["SentenceAudio".to_string()],
            }],
            ..sample_note()
        };

        let plain = note_params(&sample_note()).expect("params");
        let audio = note_params(&with_audio).expect("params");

        assert_eq!(plain, sample_note_json());
        assert!(audio.get("audio").is_some());
        assert!(audio.get("picture").is_none());
    }

    #[test]
    fn notes_exist_sends_one_find_per_query_in_a_single_request() {
        let mock = MockAnki::start(vec![ok(
            json!([[11], [], { "result": [5], "error": null }]),
        )]);

        let found = client(&mock)
            .notes_exist(&["a".to_string(), "b".to_string(), "c".to_string()])
            .expect("found");

        assert_eq!(found, vec![true, false, true]);
        assert_eq!(
            mock.requests(),
            vec![json!({
                "action": "multi",
                "version": 6,
                "params": { "actions": [
                    { "action": "findNotes", "params": { "query": "a" } },
                    { "action": "findNotes", "params": { "query": "b" } },
                    { "action": "findNotes", "params": { "query": "c" } }
                ] }
            })]
        );
    }

    #[test]
    fn notes_exist_with_no_queries_does_not_call_anki() {
        let mock = MockAnki::start(vec![]);

        assert_eq!(
            client(&mock).notes_exist(&[]).expect("empty"),
            Vec::<bool>::new()
        );
        assert!(mock.requests().is_empty());
    }

    #[test]
    fn a_failed_query_counts_as_no_note_and_a_wrong_count_is_an_error() {
        let mock = MockAnki::start(vec![
            ok(json!([{ "result": null, "error": "bad query" }, 7, [1]])),
            ok(json!([[1]])),
        ]);
        let queries = ["x".to_string(), "y".to_string(), "z".to_string()];

        assert_eq!(
            client(&mock).notes_exist(&queries).expect("found"),
            vec![false, false, true]
        );
        let error = client(&mock).notes_exist(&queries).expect_err("count");
        assert_eq!(
            error,
            AnkiError::InvalidResponse("multi: 1 answers for 3 queries".to_string())
        );
    }

    #[test]
    fn notes_exist_reports_a_closed_anki_as_a_connection_error() {
        let result = AnkiClient::new("http://127.0.0.1:1")
            .with_retries(0)
            .notes_exist(&["a".to_string()]);

        assert!(matches!(result, Err(AnkiError::Connection { .. })));
    }

    #[test]
    fn the_known_word_query_searches_the_deck_and_the_exact_field() {
        assert_eq!(
            known_word_query("Mining", "Expression", "猫"),
            "\"deck:Mining\" \"Expression:猫\""
        );
    }

    #[test]
    fn characters_with_a_meaning_in_a_search_are_escaped() {
        assert_eq!(
            known_word_query("Anime: \"A_B\"", "My*Field", "a\\b*c_d:e\"f"),
            "\"deck:Anime\\: \\\"A\\_B\\\"\" \"My\\*Field:a\\\\b\\*c\\_d\\:e\\\"f\""
        );
        assert_eq!(escape_search("日本語"), "日本語");
    }
}
