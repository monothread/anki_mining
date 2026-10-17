//! Portuguese meanings from the Portuguese Wiktionary (CC BY-SA, through kaikki.org), stored
//! next to the JMdict entries they belong to.

use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;

use rusqlite::{params, Connection};
use serde::Deserialize;

use super::DictionaryError;

/// What an import of Portuguese meanings found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct PortugueseStats {
    /// Dictionary entries that got Portuguese meanings.
    pub entries: usize,
    /// Wiktionary words with usable meanings.
    pub words: usize,
    /// Of those, the words that match no dictionary entry.
    pub unmatched: usize,
}

#[derive(Deserialize)]
struct Row {
    word: String,
    #[serde(default)]
    lang_code: String,
    #[serde(default)]
    pos: String,
    #[serde(default)]
    senses: Vec<Sense>,
}

#[derive(Deserialize)]
struct Sense {
    #[serde(default)]
    glosses: Vec<String>,
}

/// Meanings that only point to another word or describe how a word is written say nothing about
/// what it means, so the English meaning is better.
const REFERENCE_PREFIXES: [&str; 11] = [
    "leitura em",
    "o mesmo que",
    "transliteração",
    "variante",
    "forma ",
    "grafia",
    "ver ",
    "ortografia",
    "escrita ",
    "abreviação de",
    "sinônimo de",
];

pub(super) fn import(
    connection: &mut Connection,
    reader: impl BufRead,
) -> Result<PortugueseStats, DictionaryError> {
    let words = read_words(reader)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch("DELETE FROM entry_pt;")?;

    let mut by_entry: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut unmatched = 0;
    {
        let mut find = transaction.prepare(
            "SELECT entry.id, entry.pos FROM term JOIN entry ON entry.id = term.entry_id
             WHERE term.text = ?1",
        )?;
        for ((word, pos), meanings) in &words {
            let rows: Vec<(i64, String)> = find
                .query_map(params![word], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            let mut matched = false;
            for (id, tags) in rows {
                let tags: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
                if pos_fits(pos, &tags) {
                    by_entry
                        .entry(id)
                        .or_default()
                        .extend(meanings.iter().cloned());
                    matched = true;
                }
            }
            if !matched {
                unmatched += 1;
            }
        }
        let mut store =
            transaction.prepare("INSERT INTO entry_pt (id, meanings) VALUES (?1, ?2)")?;
        for (id, meanings) in &by_entry {
            let mut seen = BTreeSet::new();
            let unique: Vec<&String> = meanings
                .iter()
                .filter(|meaning| seen.insert(meaning.as_str()))
                .collect();
            store.execute(params![id, serde_json::to_string(&unique)?])?;
        }
    }
    transaction.commit()?;
    Ok(PortugueseStats {
        entries: by_entry.len(),
        words: words.len(),
        unmatched,
    })
}

/// The usable meanings of each (word, part of speech) of the Japanese entries.
fn read_words(
    reader: impl BufRead,
) -> Result<BTreeMap<(String, String), Vec<String>>, DictionaryError> {
    let mut words: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for line in reader.lines() {
        let line = line.map_err(|error| {
            DictionaryError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let row: Row = serde_json::from_str(&line)?;
        if row.lang_code != "ja" || row.pos == "romanization" {
            continue;
        }
        let meanings: Vec<String> = row
            .senses
            .iter()
            .filter_map(|sense| clean_meaning(&sense.glosses))
            .collect();
        if !meanings.is_empty() {
            words
                .entry((row.word, row.pos))
                .or_default()
                .extend(meanings);
        }
    }
    Ok(words)
}

/// One meaning from the glosses of a sense; `None` when it is only a reference.
fn clean_meaning(glosses: &[String]) -> Option<String> {
    let joined = glosses
        .iter()
        .map(|gloss| gloss.trim().trim_end_matches('.').trim())
        .filter(|gloss| !gloss.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let lower = joined.to_lowercase();
    if joined.is_empty()
        || REFERENCE_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        || names_another_word(&joined)
    {
        return None;
    }
    Some(joined)
}

/// Whether the meaning starts with a Japanese word and a colon (`記者: repórter`): Wiktionary lists
/// homophones and spellings that way, and those are meanings of other words.
fn names_another_word(meaning: &str) -> bool {
    let Some((head, _)) = meaning.split_once(':') else {
        return false;
    };
    let head = head.trim();
    !head.is_empty() && head.chars().all(is_japanese)
}

fn is_japanese(character: char) -> bool {
    matches!(
        character,
        '\u{3040}'..='\u{30ff}'
            | '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9fff}'
            | '\u{ff66}'..='\u{ff9f}'
            | '々'
    )
}

/// Whether a Wiktionary part of speech can describe an entry with these JMdict tags.
fn pos_fits(wiktionary_pos: &str, tags: &[String]) -> bool {
    let starts = |prefix: &str| tags.iter().any(|tag| tag.starts_with(prefix));
    match wiktionary_pos {
        "noun" | "name" => starts("n") || starts("pn"),
        "verb" => starts("v"),
        "adj" => starts("adj"),
        "adv" => starts("adv"),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn meanings_that_only_refer_to_another_word_are_dropped() {
        assert_eq!(
            clean_meaning(&["leitura em hiragana de 奴隷制".to_string()]),
            None
        );
        assert_eq!(clean_meaning(&["o mesmo que 肝をつぶす".to_string()]), None);
        assert_eq!(clean_meaning(&["Transliteração de を".to_string()]), None);
        assert_eq!(clean_meaning(&[]), None);
        assert_eq!(clean_meaning(&["  .  ".to_string()]), None);
    }

    #[test]
    fn meanings_that_start_with_another_japanese_word_are_dropped() {
        assert_eq!(
            clean_meaning(&["記者: repórter; jornalista".to_string()]),
            None
        );
        assert_eq!(clean_meaning(&["猫: gato".to_string()]), None);
        assert_eq!(clean_meaning(&["ネコ・ねこ: gato".to_string()]), None);
        assert_eq!(
            clean_meaning(&["Nota: não é um verbo".to_string()]),
            Some("Nota: não é um verbo".to_string())
        );
        assert_eq!(
            clean_meaning(&["gato 猫: animal".to_string()]),
            Some("gato 猫: animal".to_string())
        );
    }

    #[test]
    fn glosses_of_a_sense_are_joined_and_lose_the_final_period() {
        assert_eq!(
            clean_meaning(&["ideograma chinês".to_string(), " kanji. ".to_string()]),
            Some("ideograma chinês; kanji".to_string())
        );
        assert_eq!(
            clean_meaning(&["Suíça.".to_string()]),
            Some("Suíça".to_string())
        );
    }

    #[test]
    fn the_part_of_speech_must_fit_the_entry() {
        assert!(pos_fits("noun", &tags(&["n", "vs"])));
        assert!(pos_fits("noun", &tags(&["pn"])));
        assert!(!pos_fits("noun", &tags(&["v1"])));
        assert!(pos_fits("verb", &tags(&["v5r", "vt"])));
        assert!(!pos_fits("verb", &tags(&["n"])));
        assert!(pos_fits("adj", &tags(&["adj-i"])));
        assert!(!pos_fits("adj", &tags(&["adv"])));
        assert!(pos_fits("adv", &tags(&["adv", "n"])));
        assert!(pos_fits("phrase", &tags(&["exp"])));
        assert!(pos_fits("", &tags(&[])));
    }
}
