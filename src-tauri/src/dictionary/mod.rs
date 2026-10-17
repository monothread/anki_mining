pub mod deinflect;
mod import;

use std::io::Read;
use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::types::entry::Entry;

pub use import::ImportStats;

/// Version of the dictionary that is bundled with the app; raise it when the bundled data changes.
pub const BUNDLE_VERSION: u32 = 2;
/// The `meta` key that tells where the entries came from (`bundled:<version>` or nothing).
pub const SOURCE_KEY: &str = "source";

/// Copies the bundled dictionary to `target` when there is none there yet, when it is empty, when
/// it predates the bundled one (no marker) or when it is an older bundled version. A dictionary
/// the user imported (`custom`) is left alone. Returns
/// whether it was copied.
pub fn install_bundled(bundled: &Path, target: &Path) -> Result<bool, DictionaryError> {
    if !needs_bundled_copy(target)? {
        return Ok(false);
    }
    let partial = target.with_extension("db.part");
    std::fs::copy(bundled, &partial).map_err(|error| {
        DictionaryError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
    })?;
    std::fs::rename(&partial, target).map_err(|error| {
        DictionaryError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
    })?;
    Ok(true)
}

fn needs_bundled_copy(target: &Path) -> Result<bool, DictionaryError> {
    if !target.exists() {
        return Ok(true);
    }
    let existing = Dictionary::open(target)?;
    if existing.entry_count()? == 0 {
        return Ok(true);
    }
    // A dictionary with no marker predates the bundled one; only an explicit import is custom.
    let Some(source) = existing.meta(SOURCE_KEY)? else {
        return Ok(true);
    };
    let installed = source
        .strip_prefix("bundled:")
        .and_then(|version| version.parse::<u32>().ok());
    Ok(installed.is_some_and(|version| version < BUNDLE_VERSION))
}

/// Longest text (in characters) tried by [`Dictionary::scan`].
pub const MAX_SCAN_CHARS: usize = 20;

/// A dictionary entry reached from an inflected text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeinflectedEntry {
    pub entry: Entry,
    /// The text that was looked up, exactly as written.
    pub source: String,
    /// Inflections undone to reach `entry`, outermost first; empty for an exact match.
    pub reasons: Vec<&'static str>,
}

/// All entries found for one prefix of the scanned text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub source: String,
    /// Length of `source` in UTF-16 code units, for highlighting it in the frontend.
    pub length: usize,
    pub entries: Vec<DeinflectedEntry>,
}

#[derive(Debug, Error)]
pub enum DictionaryError {
    #[error("dictionary database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid JMdict JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("JMdict entry id `{0}` is not a number")]
    InvalidEntryId(String),
    #[error("corrupt dictionary row for entry {id}: {reason}")]
    CorruptRow { id: i64, reason: String },
}

/// A kana reading and the kanji spellings it applies to (`*` means all of them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Reading {
    pub text: String,
    pub applies_to_kanji: Vec<String>,
}

const WILDCARD: &str = "*";

/// JMdict-backed dictionary stored in SQLite.
pub struct Dictionary {
    connection: Connection,
}

impl Dictionary {
    pub fn open(path: &Path) -> Result<Dictionary, DictionaryError> {
        Dictionary::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Dictionary, DictionaryError> {
        Dictionary::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Dictionary, DictionaryError> {
        import::create_schema(&connection)?;
        Ok(Dictionary { connection })
    }

    /// Replaces the dictionary contents with a `jmdict-simplified` JSON file.
    pub fn import_jmdict(&mut self, reader: impl Read) -> Result<ImportStats, DictionaryError> {
        import::import_jmdict(&mut self.connection, reader)
    }

    /// A value kept in the database next to the entries (e.g. where they came from).
    pub fn meta(&self, key: &str) -> Result<Option<String>, DictionaryError> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = statement.query(params![key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), DictionaryError> {
        self.connection.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn entry_count(&self) -> Result<usize, DictionaryError> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM entry", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or_default())
    }

    /// Human-readable description of a JMdict tag such as `v5r` or `n`.
    pub fn tag_description(&self, code: &str) -> Result<Option<String>, DictionaryError> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT description FROM tag WHERE code = ?1")?;
        let mut rows = statement.query(params![code])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Exact lookup of a written form or reading. Common entries come first.
    ///
    /// For a kanji match `expression` is that spelling and `reading` the first kana reading that
    /// applies to it; for a kana match both are the matched kana. `parts_of_speech` holds the raw
    /// JMdict tags (e.g. `v1`, `n`) so deinflection can check the word class.
    pub fn lookup(&self, term: &str) -> Result<Vec<Entry>, DictionaryError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT entry.id, entry.readings, entry.meanings, entry.pos, term.is_kanji
             FROM term JOIN entry ON entry.id = term.entry_id
             WHERE term.text = ?1
             ORDER BY entry.common DESC, entry.id ASC",
        )?;
        let rows = statement.query_map(params![term], |row| {
            Ok(EntryRow {
                id: row.get(0)?,
                readings: row.get(1)?,
                meanings: row.get(2)?,
                pos: row.get(3)?,
                matched_kanji: row.get(4)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?.into_entry(term)?);
        }
        Ok(entries)
    }

    /// Looks up `text` as written and as every deinflected form whose dictionary entry has a
    /// matching part of speech. The exact match comes first, then fewer inflections first.
    pub fn lookup_deinflected(&self, text: &str) -> Result<Vec<DeinflectedEntry>, DictionaryError> {
        let mut results: Vec<DeinflectedEntry> = Vec::new();

        for (index, candidate) in deinflect::deinflect(text).into_iter().enumerate() {
            for entry in self.lookup(&candidate.term)? {
                let is_exact_match = index == 0;
                if !is_exact_match && !matches_category(&entry, candidate.rules) {
                    continue;
                }
                let found = DeinflectedEntry {
                    entry,
                    source: text.to_string(),
                    reasons: candidate.reasons.clone(),
                };
                if !results.iter().any(|existing| existing.entry == found.entry) {
                    results.push(found);
                }
            }
        }
        Ok(results)
    }

    /// Tries every prefix of `text` (at most [`MAX_SCAN_CHARS`] characters), longest first, and
    /// returns the prefixes that have dictionary entries. Used to find compounds and inflected
    /// words starting at a position in a sentence.
    pub fn scan(&self, text: &str) -> Result<Vec<ScanResult>, DictionaryError> {
        let boundaries: Vec<usize> = text
            .char_indices()
            .map(|(index, _)| index)
            .skip(1)
            .chain(std::iter::once(text.len()))
            .take(MAX_SCAN_CHARS)
            .collect();

        let mut results = Vec::new();
        for end in boundaries.into_iter().rev() {
            let source = &text[..end];
            let entries = self.lookup_deinflected(source)?;
            if !entries.is_empty() {
                results.push(ScanResult {
                    source: source.to_string(),
                    length: source.encode_utf16().count(),
                    entries,
                });
            }
        }
        Ok(results)
    }
}

fn matches_category(entry: &Entry, category: u32) -> bool {
    entry
        .parts_of_speech
        .iter()
        .any(|tag| deinflect::category_of_tag(tag) & category != 0)
}

struct EntryRow {
    id: i64,
    readings: String,
    meanings: String,
    pos: String,
    matched_kanji: bool,
}

impl EntryRow {
    fn into_entry(self, term: &str) -> Result<Entry, DictionaryError> {
        let readings: Vec<Reading> = self.parse(&self.readings, "readings")?;
        let reading = if self.matched_kanji {
            reading_for_kanji(&readings, term)
        } else {
            term.to_string()
        };

        Ok(Entry {
            expression: term.to_string(),
            reading,
            meanings: self.parse(&self.meanings, "meanings")?,
            parts_of_speech: self.parse(&self.pos, "pos")?,
            frequency: None,
        })
    }

    /// The English meanings, preceded by the Portuguese ones when that language is chosen.
    fn parse<T: for<'de> Deserialize<'de>>(
        &self,
        json: &str,
        column: &str,
    ) -> Result<T, DictionaryError> {
        serde_json::from_str(json).map_err(|error| DictionaryError::CorruptRow {
            id: self.id,
            reason: format!("column `{column}`: {error}"),
        })
    }
}

fn reading_for_kanji(readings: &[Reading], kanji: &str) -> String {
    let applicable = readings.iter().find(|reading| {
        reading
            .applies_to_kanji
            .iter()
            .any(|applies| applies == WILDCARD || applies == kanji)
    });
    applicable
        .or_else(|| readings.first())
        .map(|reading| reading.text.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn word(
        id: &str,
        kanji: &[(&str, bool)],
        kana: &[(&str, bool, &[&str])],
        senses: &[(&[&str], &[&str])],
    ) -> serde_json::Value {
        json!({
            "id": id,
            "kanji": kanji.iter().map(|(text, common)| json!({ "common": common, "text": text, "tags": [] })).collect::<Vec<_>>(),
            "kana": kana.iter().map(|(text, common, applies)| json!({ "common": common, "text": text, "tags": [], "appliesToKanji": applies })).collect::<Vec<_>>(),
            "sense": senses.iter().map(|(pos, glosses)| json!({
                "partOfSpeech": pos,
                "gloss": glosses.iter().map(|text| json!({ "lang": "eng", "gender": null, "type": null, "text": text })).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        })
    }

    fn fixture() -> String {
        json!({
            "version": "3.6.2",
            "languages": ["eng"],
            "commonOnly": false,
            "dictDate": "2026-10-12",
            "dictRevisions": [],
            "tags": { "n": "noun (common) (futsuumeishi)", "v1": "Ichidan verb", "vt": "transitive verb" },
            "words": [
                word("1467640", &[("猫", true)], &[("ねこ", true, &["*"]), ("ネコ", false, &[])],
                    &[(&["n"], &["cat", "feline"]), (&["n"], &["shamisen"])]),
                word("1358280", &[("食べる", true), ("喰べる", false)], &[("たべる", true, &["*"])],
                    &[(&["v1", "vt"], &["to eat"])]),
                word("1000001", &[("髪", true)], &[("かみ", true, &["*"])], &[(&["n"], &["hair"])]),
                word("1000002", &[("神", false)], &[("かみ", false, &["*"])], &[(&["n"], &["god"])]),
                word("1000003", &[("紙", true)], &[("かみ", true, &["*"])], &[(&["n"], &["paper"])]),
                word("1000004", &[("日", true)], &[("ひ", true, &["日"]), ("にち", false, &[])], &[(&["n"], &["day"])]),
                word("1000005", &[], &[("ヽ", false, &["*"])], &[(&["unc"], &["repetition mark"])]),
                word("1000006", &[("壊", false)], &[], &[(&["n"], &["broken entry"])]),
            ]
        })
        .to_string()
    }

    fn imported() -> Dictionary {
        let mut dictionary = Dictionary::open_in_memory().expect("open");
        dictionary
            .import_jmdict(fixture().as_bytes())
            .expect("import");
        dictionary
    }

    #[test]
    fn import_reports_what_it_stored_and_skipped() {
        let mut dictionary = Dictionary::open_in_memory().expect("open");

        let stats = dictionary
            .import_jmdict(fixture().as_bytes())
            .expect("import");

        assert_eq!(
            stats,
            ImportStats {
                entries: 7,
                terms: 16,
                skipped: 1
            }
        );
        assert_eq!(dictionary.entry_count().expect("count"), 7);
    }

    #[test]
    fn looks_up_a_kanji_form_with_senses_and_part_of_speech() {
        assert_eq!(
            imported().lookup("猫").expect("lookup"),
            vec![Entry {
                expression: "猫".to_string(),
                reading: "ねこ".to_string(),
                meanings: vec!["cat; feline".to_string(), "shamisen".to_string()],
                parts_of_speech: vec!["n".to_string()],
                frequency: None,
            }]
        );
    }

    #[test]
    fn looks_up_a_kana_form_and_keeps_the_matched_kana_as_expression() {
        assert_eq!(
            imported().lookup("ねこ").expect("lookup"),
            vec![Entry {
                expression: "ねこ".to_string(),
                reading: "ねこ".to_string(),
                meanings: vec!["cat; feline".to_string(), "shamisen".to_string()],
                parts_of_speech: vec!["n".to_string()],
                frequency: None,
            }]
        );
    }

    #[test]
    fn looks_up_an_alternative_katakana_reading() {
        let entries = imported().lookup("ネコ").expect("lookup");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].expression, "ネコ");
        assert_eq!(entries[0].reading, "ネコ");
    }

    #[test]
    fn looks_up_an_alternative_kanji_spelling_of_a_verb_with_all_of_its_pos_tags() {
        assert_eq!(
            imported().lookup("喰べる").expect("lookup"),
            vec![Entry {
                expression: "喰べる".to_string(),
                reading: "たべる".to_string(),
                meanings: vec!["to eat".to_string()],
                parts_of_speech: vec!["v1".to_string(), "vt".to_string()],
                frequency: None,
            }]
        );
    }

    #[test]
    fn orders_common_entries_first_then_by_id() {
        let expressions: Vec<(String, String)> = imported()
            .lookup("かみ")
            .expect("lookup")
            .into_iter()
            .map(|entry| (entry.meanings[0].clone(), entry.reading))
            .collect();

        assert_eq!(
            expressions,
            vec![
                ("hair".to_string(), "かみ".to_string()),
                ("paper".to_string(), "かみ".to_string()),
                ("god".to_string(), "かみ".to_string()),
            ]
        );
    }

    #[test]
    fn picks_the_reading_that_applies_to_the_matched_kanji() {
        let entries = imported().lookup("日").expect("lookup");

        assert_eq!(entries[0].reading, "ひ");
    }

    #[test]
    fn kana_only_entries_are_found_by_their_reading() {
        let entries = imported().lookup("ヽ").expect("lookup");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].meanings, vec!["repetition mark".to_string()]);
        assert_eq!(entries[0].parts_of_speech, vec!["unc".to_string()]);
    }

    #[test]
    fn returns_nothing_for_unknown_or_empty_terms() {
        let dictionary = imported();

        assert_eq!(dictionary.lookup("存在しない").expect("lookup"), vec![]);
        assert_eq!(dictionary.lookup("").expect("lookup"), vec![]);
    }

    #[test]
    fn skipped_entries_without_kana_are_not_searchable() {
        assert_eq!(imported().lookup("壊").expect("lookup"), vec![]);
    }

    #[test]
    fn exposes_tag_descriptions() {
        let dictionary = imported();

        assert_eq!(
            dictionary.tag_description("v1").expect("tag"),
            Some("Ichidan verb".to_string())
        );
        assert_eq!(dictionary.tag_description("zzz").expect("tag"), None);
    }

    #[test]
    fn importing_again_replaces_the_previous_contents() {
        let mut dictionary = imported();
        let replacement = json!({
            "tags": {},
            "words": [word("1", &[("犬", true)], &[("いぬ", true, &["*"])], &[(&["n"], &["dog"])])]
        })
        .to_string();

        let stats = dictionary
            .import_jmdict(replacement.as_bytes())
            .expect("import");

        assert_eq!(
            stats,
            ImportStats {
                entries: 1,
                terms: 2,
                skipped: 0
            }
        );
        assert_eq!(dictionary.lookup("猫").expect("lookup"), vec![]);
        assert_eq!(dictionary.lookup("犬").expect("lookup").len(), 1);
        assert_eq!(dictionary.tag_description("v1").expect("tag"), None);
    }

    #[test]
    fn invalid_json_fails_and_keeps_the_previous_contents() {
        let mut dictionary = imported();

        let error = dictionary
            .import_jmdict("{ \"words\": [ { not json".as_bytes())
            .expect_err("must fail");

        assert!(matches!(error, DictionaryError::InvalidJson(_)));
        assert_eq!(dictionary.entry_count().expect("count"), 7);
    }

    #[test]
    fn a_word_with_a_non_numeric_id_fails_the_import() {
        let mut dictionary = Dictionary::open_in_memory().expect("open");
        let payload = json!({
            "words": [word("abc", &[], &[("あ", true, &["*"])], &[(&["n"], &["x"])])]
        })
        .to_string();

        let error = dictionary
            .import_jmdict(payload.as_bytes())
            .expect_err("must fail");

        assert!(matches!(error, DictionaryError::InvalidEntryId(ref id) if id == "abc"));
        assert_eq!(error.to_string(), "JMdict entry id `abc` is not a number");
        assert_eq!(dictionary.entry_count().expect("count"), 0);
    }

    #[test]
    fn persists_to_disk_and_can_be_reopened() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("jmdict.db");
        {
            let mut dictionary = Dictionary::open(&path).expect("open");
            dictionary
                .import_jmdict(fixture().as_bytes())
                .expect("import");
        }

        let reopened = Dictionary::open(&path).expect("reopen");

        assert_eq!(reopened.entry_count().expect("count"), 7);
        assert_eq!(reopened.lookup("猫").expect("lookup").len(), 1);
    }

    #[test]
    fn open_fails_when_the_path_is_not_a_database_location() {
        let directory = tempfile::tempdir().expect("temp dir");

        let result = Dictionary::open(&directory.path().join("missing").join("jmdict.db"));

        assert!(matches!(result, Err(DictionaryError::Database(_))));
    }

    #[test]
    fn a_corrupt_row_is_reported_with_its_entry_id() {
        let dictionary = imported();
        dictionary
            .connection
            .execute(
                "UPDATE entry SET meanings = 'not json' WHERE id = 1467640",
                [],
            )
            .expect("update");

        let error = dictionary.lookup("猫").expect_err("must fail");

        assert!(matches!(
            error,
            DictionaryError::CorruptRow { id: 1467640, .. }
        ));
        assert!(error
            .to_string()
            .starts_with("corrupt dictionary row for entry 1467640"));
    }

    #[test]
    fn reading_for_kanji_falls_back_to_the_first_reading() {
        let readings = vec![Reading {
            text: "ひ".to_string(),
            applies_to_kanji: vec!["火".to_string()],
        }];

        assert_eq!(reading_for_kanji(&readings, "日"), "ひ");
        assert_eq!(reading_for_kanji(&[], "日"), "");
    }

    fn inflection_fixture() -> Dictionary {
        let long_kana = "あ".repeat(MAX_SCAN_CHARS);
        let too_long_kana = "い".repeat(MAX_SCAN_CHARS + 1);
        let payload = json!({
            "tags": {},
            "words": [
                word("1", &[("食べる", true)], &[("たべる", true, &["*"])], &[(&["v1", "vt"], &["to eat"])]),
                word("2", &[("買う", true)], &[("かう", true, &["*"])], &[(&["v5u", "vt"], &["to buy"])]),
                word("3", &[("買い", false)], &[("かい", false, &["*"])], &[(&["n"], &["purchase"])]),
                word("4", &[("行く", true)], &[("いく", true, &["*"])], &[(&["v5k-s", "vi"], &["to go"])]),
                word("5", &[], &[("する", true, &["*"])], &[(&["vs-i", "vt"], &["to do"])]),
                word("6", &[("来る", true)], &[("くる", true, &["*"])], &[(&["vk", "vi"], &["to come"])]),
                word("7", &[("高い", true)], &[("たかい", true, &["*"])], &[(&["adj-i"], &["high"])]),
                word("8", &[("勉強", true)], &[("べんきょう", true, &["*"])], &[(&["n", "vs"], &["study"])]),
                word("9", &[("日本語", true)], &[("にほんご", true, &["*"])], &[(&["n"], &["Japanese language"])]),
                word("10", &[("日本", true)], &[("にほん", true, &["*"])], &[(&["n"], &["Japan"])]),
                word("11", &[("走る", false)], &[("はしる", false, &["*"])], &[(&["n"], &["fake noun"])]),
                word("12", &[("歩く", false)], &[("あるく", false, &["*"])], &[(&["v1", "v5k"], &["to walk"])]),
                word("13", &[], &[(long_kana.as_str(), false, &["*"])], &[(&["n"], &["twenty"])]),
                word("14", &[], &[(too_long_kana.as_str(), false, &["*"])], &[(&["n"], &["twenty-one"])]),
            ]
        })
        .to_string();
        let mut dictionary = Dictionary::open_in_memory().expect("open");
        dictionary
            .import_jmdict(payload.as_bytes())
            .expect("import");
        dictionary
    }

    type ExpressionAndReasons = (String, Vec<&'static str>);

    fn sources_and_reasons(results: &[DeinflectedEntry]) -> Vec<ExpressionAndReasons> {
        results
            .iter()
            .map(|result| (result.entry.expression.clone(), result.reasons.clone()))
            .collect()
    }

    #[test]
    fn an_exact_match_has_no_reasons_and_keeps_the_source() {
        let results = inflection_fixture()
            .lookup_deinflected("食べる")
            .expect("lookup");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "食べる");
        assert_eq!(results[0].reasons, Vec::<&str>::new());
        assert_eq!(results[0].entry.expression, "食べる");
        assert_eq!(results[0].entry.reading, "たべる");
        assert_eq!(results[0].entry.meanings, vec!["to eat".to_string()]);
    }

    #[test]
    fn finds_the_dictionary_form_of_a_deeply_inflected_verb() {
        let results = inflection_fixture()
            .lookup_deinflected("食べさせられなかった")
            .expect("lookup");

        assert_eq!(
            sources_and_reasons(&results),
            vec![(
                "食べる".to_string(),
                vec!["past", "negative", "potential or passive", "causative"]
            )]
        );
        assert_eq!(results[0].source, "食べさせられなかった");
    }

    #[test]
    fn finds_godan_irregular_suru_kuru_and_adjective_forms() {
        let dictionary = inflection_fixture();
        let cases = [
            ("買いました", "買う", vec!["past", "polite"]),
            ("行った", "行く", vec!["past"]),
            ("しなかった", "する", vec!["past", "negative"]),
            ("来ません", "来る", vec!["negative", "polite"]),
            ("高くなかった", "高い", vec!["past", "negative"]),
        ];

        for (text, expression, reasons) in cases {
            let results = dictionary.lookup_deinflected(text).expect("lookup");
            assert!(
                sources_and_reasons(&results).contains(&(expression.to_string(), reasons.clone())),
                "{text} should reach {expression} with {reasons:?}, got {results:?}"
            );
        }
    }

    #[test]
    fn lists_the_exact_match_before_deinflected_matches() {
        let results = inflection_fixture()
            .lookup_deinflected("買い")
            .expect("lookup");

        assert_eq!(
            sources_and_reasons(&results),
            vec![("買い".to_string(), vec![]), ("買う".to_string(), vec![]),]
        );
        assert_eq!(results[0].entry.parts_of_speech, vec!["n".to_string()]);
        assert_eq!(
            results[1].entry.parts_of_speech,
            vec!["v5u".to_string(), "vt".to_string()]
        );
    }

    #[test]
    fn rejects_candidates_whose_part_of_speech_does_not_match() {
        // 走る is only a noun in the fixture, so 走った must not resolve to it.
        let results = inflection_fixture()
            .lookup_deinflected("走った")
            .expect("lookup");

        assert_eq!(results, vec![]);
    }

    #[test]
    fn rejects_an_adjective_ending_on_a_verb_entry() {
        let results = inflection_fixture()
            .lookup_deinflected("食べくない")
            .expect("lookup");

        assert_eq!(results, vec![]);
    }

    #[test]
    fn reports_an_entry_reached_through_two_rules_only_once() {
        let results = inflection_fixture()
            .lookup_deinflected("歩ける")
            .expect("lookup");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.expression, "歩く");
        assert_eq!(results[0].reasons, vec!["potential"]);
    }

    #[test]
    fn scan_returns_prefixes_longest_first() {
        let results = inflection_fixture()
            .scan("日本語を勉強しています")
            .expect("scan");

        assert_eq!(
            results
                .iter()
                .map(|result| result.source.as_str())
                .collect::<Vec<_>>(),
            vec!["日本語", "日本"]
        );
        assert_eq!(
            results[0].entries[0].entry.meanings,
            vec!["Japanese language".to_string()]
        );
        assert_eq!(
            results[1].entries[0].entry.meanings,
            vec!["Japan".to_string()]
        );
    }

    #[test]
    fn scan_finds_an_inflected_word_and_every_shorter_valid_form_of_it() {
        let results = inflection_fixture()
            .scan("食べさせられなかったので")
            .expect("scan");

        let summary: Vec<(String, Vec<ExpressionAndReasons>)> = results
            .iter()
            .map(|result| (result.source.clone(), sources_and_reasons(&result.entries)))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "食べさせられなかった".to_string(),
                    vec![(
                        "食べる".to_string(),
                        vec!["past", "negative", "potential or passive", "causative"]
                    )]
                ),
                (
                    "食べさせられ".to_string(),
                    vec![(
                        "食べる".to_string(),
                        vec!["potential or passive", "causative"]
                    )]
                ),
                (
                    "食べさせ".to_string(),
                    vec![("食べる".to_string(), vec!["causative"])]
                ),
                ("食べ".to_string(), vec![("食べる".to_string(), vec![])]),
            ]
        );
    }

    #[test]
    fn scan_of_a_noun_followed_by_suru_conjugation_finds_the_noun() {
        let results = inflection_fixture().scan("勉強して").expect("scan");

        assert_eq!(
            results
                .iter()
                .map(|result| result.source.as_str())
                .collect::<Vec<_>>(),
            vec!["勉強"]
        );
    }

    #[test]
    fn scan_returns_nothing_for_empty_or_unknown_text() {
        let dictionary = inflection_fixture();

        assert_eq!(dictionary.scan("").expect("scan"), vec![]);
        assert_eq!(dictionary.scan("zzz").expect("scan"), vec![]);
    }

    #[test]
    fn scan_considers_at_most_the_maximum_number_of_characters() {
        let dictionary = inflection_fixture();

        let twenty = dictionary.scan(&"あ".repeat(MAX_SCAN_CHARS)).expect("scan");
        let twenty_one = dictionary
            .scan(&"い".repeat(MAX_SCAN_CHARS + 1))
            .expect("scan");

        assert_eq!(twenty.len(), 1);
        assert_eq!(twenty[0].source.chars().count(), MAX_SCAN_CHARS);
        assert_eq!(twenty_one, vec![]);
    }

    #[test]
    fn scan_only_looks_at_the_start_of_the_text() {
        let results = inflection_fixture().scan("を日本").expect("scan");

        assert_eq!(results, vec![]);
    }

    #[test]
    fn matches_category_requires_an_inflecting_part_of_speech() {
        let verb = Entry {
            expression: "食べる".to_string(),
            reading: "たべる".to_string(),
            meanings: vec![],
            parts_of_speech: vec!["vt".to_string(), "v1".to_string()],
            frequency: None,
        };
        let noun = Entry {
            parts_of_speech: vec!["n".to_string()],
            ..verb.clone()
        };

        assert!(matches_category(&verb, deinflect::V1));
        assert!(!matches_category(&verb, deinflect::V5));
        assert!(!matches_category(&noun, deinflect::ANY));
    }

    #[test]
    fn scan_results_report_their_length_in_utf16_units_and_serialize_for_the_frontend() {
        let results = inflection_fixture().scan("日本語を").expect("scan");

        assert_eq!(results[0].source, "日本語");
        assert_eq!(results[0].length, 3);
        let json = serde_json::to_value(&results[0]).expect("serialize");
        assert_eq!(json["source"], json!("日本語"));
        assert_eq!(json["length"], json!(3));
        assert_eq!(json["entries"][0]["source"], json!("日本語"));
        assert_eq!(json["entries"][0]["reasons"], json!([]));
        assert_eq!(json["entries"][0]["entry"]["expression"], json!("日本語"));
        assert_eq!(json["entries"][0]["entry"]["partsOfSpeech"], json!(["n"]));
    }

    #[test]
    fn meta_values_are_stored_replaced_and_marked_custom_by_an_import() {
        let mut dictionary = imported();
        assert_eq!(
            dictionary.meta("source").expect("meta"),
            Some("custom".to_string())
        );

        dictionary.set_meta("source", "bundled:1").expect("set");
        dictionary.set_meta("source", "bundled:2").expect("replace");
        assert_eq!(
            dictionary.meta("source").expect("meta"),
            Some("bundled:2".to_string())
        );

        dictionary
            .import_jmdict(fixture().as_bytes())
            .expect("import");
        assert_eq!(
            dictionary.meta("source").expect("meta"),
            Some("custom".to_string())
        );
    }

    fn dictionary_file(
        directory: &tempfile::TempDir,
        name: &str,
        source: Option<&str>,
        fill: bool,
    ) -> std::path::PathBuf {
        let path = directory.path().join(name);
        let mut dictionary = Dictionary::open(&path).expect("open");
        if fill {
            dictionary
                .import_jmdict(fixture().as_bytes())
                .expect("import");
        }
        match source {
            None => {
                dictionary
                    .connection
                    .execute("DELETE FROM meta WHERE key = 'source'", [])
                    .expect("clear");
            }
            Some(source) => {
                dictionary.set_meta(SOURCE_KEY, source).expect("meta");
            }
        }
        path
    }

    #[test]
    fn the_bundled_dictionary_is_installed_when_there_is_none() {
        let directory = tempfile::tempdir().expect("dir");
        let bundled = dictionary_file(&directory, "bundled.db", Some("bundled:1"), true);
        let target = directory.path().join("jmdict.db");

        assert!(install_bundled(&bundled, &target).expect("install"));

        assert_eq!(
            Dictionary::open(&target)
                .expect("open")
                .entry_count()
                .expect("count"),
            7
        );
        assert!(!directory.path().join("jmdict.db.part").exists());
    }

    #[test]
    fn an_empty_dictionary_is_replaced_by_the_bundled_one() {
        let directory = tempfile::tempdir().expect("dir");
        let bundled = dictionary_file(&directory, "bundled.db", Some("bundled:1"), true);
        let target = dictionary_file(&directory, "jmdict.db", None, false);

        assert!(install_bundled(&bundled, &target).expect("install"));

        assert_eq!(
            Dictionary::open(&target)
                .expect("open")
                .entry_count()
                .expect("count"),
            7
        );
    }

    #[test]
    fn an_older_bundled_version_is_replaced_but_the_current_one_is_kept() {
        let directory = tempfile::tempdir().expect("dir");
        let bundled = dictionary_file(&directory, "bundled.db", Some("bundled:1"), true);
        let older = dictionary_file(&directory, "older.db", Some("bundled:0"), true);
        let current = dictionary_file(
            &directory,
            "current.db",
            Some(&format!("bundled:{BUNDLE_VERSION}")),
            true,
        );

        assert!(install_bundled(&bundled, &older).expect("older"));
        assert!(!install_bundled(&bundled, &current).expect("current"));
    }

    #[test]
    fn a_dictionary_the_user_imported_is_never_replaced() {
        let directory = tempfile::tempdir().expect("dir");
        let bundled = dictionary_file(&directory, "bundled.db", Some("bundled:1"), true);
        let custom = dictionary_file(&directory, "custom.db", Some("custom"), true);

        assert!(!install_bundled(&bundled, &custom).expect("custom"));
        assert_eq!(
            Dictionary::open(&custom)
                .expect("open")
                .meta(SOURCE_KEY)
                .expect("meta"),
            Some("custom".to_string())
        );
    }

    #[test]
    fn a_dictionary_without_a_marker_is_replaced() {
        let directory = tempfile::tempdir().expect("dir");
        let bundled = dictionary_file(&directory, "bundled.db", Some("bundled:1"), true);
        let legacy = dictionary_file(&directory, "legacy.db", None, true);

        assert!(install_bundled(&bundled, &legacy).expect("legacy"));
        assert_eq!(
            Dictionary::open(&legacy)
                .expect("open")
                .meta(SOURCE_KEY)
                .expect("meta"),
            Some("bundled:1".to_string())
        );
    }

    #[test]
    fn a_missing_bundled_file_is_an_error_and_leaves_no_partial_file() {
        let directory = tempfile::tempdir().expect("dir");
        let target = directory.path().join("jmdict.db");

        let result = install_bundled(&directory.path().join("missing.db"), &target);

        assert!(result.is_err());
        assert!(!target.exists());
        assert!(!directory.path().join("jmdict.db.part").exists());
    }
}
