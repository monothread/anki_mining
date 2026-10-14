use std::collections::HashMap;
use std::fmt;
use std::io::Read;

use rusqlite::{params, Connection, Statement, Transaction};
use serde::de::{
    DeserializeSeed, Deserializer, Error as DeError, IgnoredAny, MapAccess, SeqAccess, Visitor,
};
use serde::Deserialize;

use super::{DictionaryError, Reading};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ImportStats {
    pub entries: usize,
    pub terms: usize,
    /// Entries ignored because they have no kana reading.
    pub skipped: usize,
}

#[derive(Deserialize)]
struct Word {
    id: String,
    #[serde(default)]
    kanji: Vec<Form>,
    #[serde(default)]
    kana: Vec<KanaForm>,
    #[serde(default)]
    sense: Vec<Sense>,
}

#[derive(Deserialize)]
struct Form {
    common: bool,
    text: String,
}

#[derive(Deserialize)]
struct KanaForm {
    common: bool,
    text: String,
    #[serde(default, rename = "appliesToKanji")]
    applies_to_kanji: Vec<String>,
}

#[derive(Deserialize)]
struct Sense {
    #[serde(default, rename = "partOfSpeech")]
    part_of_speech: Vec<String>,
    #[serde(default)]
    gloss: Vec<Gloss>,
}

#[derive(Deserialize)]
struct Gloss {
    text: String,
}

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS entry (
        id INTEGER PRIMARY KEY,
        common INTEGER NOT NULL,
        readings TEXT NOT NULL,
        meanings TEXT NOT NULL,
        pos TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS term (
        text TEXT NOT NULL,
        entry_id INTEGER NOT NULL,
        is_kanji INTEGER NOT NULL,
        PRIMARY KEY (text, entry_id)
    ) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS tag (
        code TEXT PRIMARY KEY,
        description TEXT NOT NULL
    );
";

pub(super) fn create_schema(connection: &Connection) -> Result<(), DictionaryError> {
    connection.execute_batch(SCHEMA)?;
    Ok(())
}

pub(super) fn import_jmdict(
    connection: &mut Connection,
    reader: impl Read,
) -> Result<ImportStats, DictionaryError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch("DELETE FROM term; DELETE FROM entry; DELETE FROM tag;")?;

    let stats = {
        let mut inserter = Inserter::new(&transaction)?;
        let tags = {
            let mut deserializer = serde_json::Deserializer::from_reader(reader);
            let outcome = deserializer.deserialize_map(FileVisitor {
                on_word: |word| inserter.insert(word),
            });
            match (outcome, inserter.failure.take()) {
                (_, Some(database_error)) => return Err(database_error),
                (Ok(tags), None) => tags,
                (Err(json_error), None) => return Err(json_error.into()),
            }
        };
        for (code, description) in tags {
            inserter.insert_tag(&code, &description)?;
        }
        inserter.stats
    };

    transaction.commit()?;
    Ok(stats)
}

struct Inserter<'a> {
    entry: Statement<'a>,
    term: Statement<'a>,
    tag: Statement<'a>,
    stats: ImportStats,
    failure: Option<DictionaryError>,
}

impl<'a> Inserter<'a> {
    fn new(transaction: &'a Transaction<'_>) -> Result<Inserter<'a>, rusqlite::Error> {
        Ok(Inserter {
            entry: transaction.prepare(
                "INSERT INTO entry (id, common, readings, meanings, pos) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?,
            term: transaction.prepare(
                "INSERT OR IGNORE INTO term (text, entry_id, is_kanji) VALUES (?1, ?2, ?3)",
            )?,
            tag: transaction.prepare("INSERT INTO tag (code, description) VALUES (?1, ?2)")?,
            stats: ImportStats {
                entries: 0,
                terms: 0,
                skipped: 0,
            },
            failure: None,
        })
    }

    /// Stores a word. A failure is remembered in `failure` and reported as `Err(())` so the
    /// streaming parser stops.
    fn insert(&mut self, word: Word) -> Result<(), ()> {
        match self.try_insert(word) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.failure = Some(error);
                Err(())
            }
        }
    }

    fn try_insert(&mut self, word: Word) -> Result<(), DictionaryError> {
        if word.kana.is_empty() {
            self.stats.skipped += 1;
            return Ok(());
        }
        let id: i64 = word
            .id
            .parse()
            .map_err(|_| DictionaryError::InvalidEntryId(word.id.clone()))?;

        let common =
            word.kanji.iter().any(|form| form.common) || word.kana.iter().any(|form| form.common);
        self.entry.execute(params![
            id,
            common,
            serde_json::to_string(&readings_of(&word))?,
            serde_json::to_string(&meanings_of(&word))?,
            serde_json::to_string(&parts_of_speech_of(&word))?,
        ])?;
        self.stats.entries += 1;

        for form in &word.kanji {
            self.insert_term(&form.text, id, true)?;
        }
        for form in &word.kana {
            self.insert_term(&form.text, id, false)?;
        }
        Ok(())
    }

    fn insert_term(
        &mut self,
        text: &str,
        entry_id: i64,
        is_kanji: bool,
    ) -> Result<(), rusqlite::Error> {
        self.stats.terms += self.term.execute(params![text, entry_id, is_kanji])?;
        Ok(())
    }

    fn insert_tag(&mut self, code: &str, description: &str) -> Result<(), rusqlite::Error> {
        self.tag.execute(params![code, description])?;
        Ok(())
    }
}

fn readings_of(word: &Word) -> Vec<Reading> {
    word.kana
        .iter()
        .map(|form| Reading {
            text: form.text.clone(),
            applies_to_kanji: form.applies_to_kanji.clone(),
        })
        .collect()
}

/// One meaning per sense, with the glosses of the sense joined by `; `.
fn meanings_of(word: &Word) -> Vec<String> {
    word.sense
        .iter()
        .map(|sense| {
            sense
                .gloss
                .iter()
                .map(|gloss| gloss.text.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        })
        .collect()
}

/// Distinct part-of-speech tags across all senses, in order of first appearance.
fn parts_of_speech_of(word: &Word) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in word
        .sense
        .iter()
        .flat_map(|sense| sense.part_of_speech.iter())
    {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    tags
}

/// Streams the top-level object: collects `tags` and hands each `words` element to `on_word`.
struct FileVisitor<F> {
    on_word: F,
}

impl<'de, F: FnMut(Word) -> Result<(), ()>> Visitor<'de> for FileVisitor<F> {
    type Value = HashMap<String, String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a jmdict-simplified JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut tags = HashMap::new();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "tags" => tags = map.next_value()?,
                "words" => map.next_value_seed(WordsSeed {
                    on_word: &mut self.on_word,
                })?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(tags)
    }
}

struct WordsSeed<'a, F> {
    on_word: &'a mut F,
}

impl<'de, F: FnMut(Word) -> Result<(), ()>> DeserializeSeed<'de> for WordsSeed<'_, F> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, F: FnMut(Word) -> Result<(), ()>> Visitor<'de> for WordsSeed<'_, F> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of JMdict words")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(word) = seq.next_element::<Word>()? {
            (self.on_word)(word)
                .map_err(|()| A::Error::custom("import aborted by a database error"))?;
        }
        Ok(())
    }
}
