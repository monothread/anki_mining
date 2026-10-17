//! Pitch accents and word frequencies, kept next to the JMdict entries and found by written form
//! and reading (they do not depend on JMdict entry ids, so replacing JMdict keeps them).

use std::io::{BufRead, Read, Seek};

use rusqlite::{params, Connection};
use serde_json::Value;

use super::DictionaryError;

/// What an import of pitch accents or frequencies stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct MetadataStats {
    pub pitch: usize,
    pub frequency: usize,
}

pub(super) fn create_schema(connection: &Connection) -> Result<(), DictionaryError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS pitch (
            term TEXT NOT NULL,
            reading TEXT NOT NULL,
            positions TEXT NOT NULL,
            PRIMARY KEY (term, reading)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS frequency (
            term TEXT NOT NULL,
            reading TEXT NOT NULL,
            rank INTEGER NOT NULL,
            PRIMARY KEY (term, reading)
        ) WITHOUT ROWID;",
    )?;
    Ok(())
}

/// Katakana written as hiragana, so `ネコ` and `ねこ` are the same reading.
pub(super) fn to_hiragana(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\u{30a1}'..='\u{30f6}' => {
                char::from_u32(u32::from(character) - 0x60).unwrap_or(character)
            }
            other => other,
        })
        .collect()
}

/// Replaces the pitch accents with the lines of a Kanjium `accents.txt`:
/// `word<TAB>reading<TAB>position[,position...]`.
pub(super) fn import_pitch_accents(
    connection: &mut Connection,
    reader: impl BufRead,
) -> Result<MetadataStats, DictionaryError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch("DELETE FROM pitch;")?;
    let mut stored = 0;
    {
        let mut statement = transaction.prepare(
            "INSERT OR REPLACE INTO pitch (term, reading, positions) VALUES (?1, ?2, ?3)",
        )?;
        for line in reader.lines() {
            let line = line.map_err(io_error)?;
            let mut columns = line.split('\t');
            let (Some(term), Some(reading), Some(accents)) =
                (columns.next(), columns.next(), columns.next())
            else {
                continue;
            };
            let positions = parse_positions(accents);
            if term.is_empty() || positions.is_empty() {
                continue;
            }
            let reading = if reading.is_empty() { term } else { reading };
            statement.execute(params![
                term,
                to_hiragana(reading),
                serde_json::to_string(&positions)?
            ])?;
            stored += 1;
        }
    }
    transaction.commit()?;
    Ok(MetadataStats {
        pitch: stored,
        frequency: 0,
    })
}

/// The downstep positions in `0,2`-style text (anything that is not a number is ignored).
fn parse_positions(text: &str) -> Vec<u8> {
    let mut positions: Vec<u8> = text
        .split(',')
        .filter_map(|position| position.trim().parse().ok())
        .collect();
    positions.dedup();
    positions
}

/// Replaces the frequencies with lines of `word<TAB>reading<TAB>rank` (the reading may be empty
/// for any reading). The lowest rank of a word and reading is kept.
pub(super) fn import_frequencies(
    connection: &mut Connection,
    reader: impl BufRead,
) -> Result<MetadataStats, DictionaryError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch("DELETE FROM frequency;")?;
    let mut stored = 0;
    {
        for line in reader.lines() {
            let line = line.map_err(io_error)?;
            let mut columns = line.split('\t');
            let (Some(term), Some(reading), Some(rank)) =
                (columns.next(), columns.next(), columns.next())
            else {
                continue;
            };
            let Ok(rank) = rank.trim().parse::<u32>() else {
                continue;
            };
            if term.is_empty() {
                continue;
            }
            store_frequency(&transaction, term, reading, rank)?;
            stored += 1;
        }
    }
    transaction.commit()?;
    Ok(MetadataStats {
        pitch: 0,
        frequency: stored,
    })
}

fn store_frequency(
    connection: &Connection,
    term: &str,
    reading: &str,
    rank: u32,
) -> Result<(), rusqlite::Error> {
    connection.execute(
        "INSERT INTO frequency (term, reading, rank) VALUES (?1, ?2, ?3)
         ON CONFLICT(term, reading) DO UPDATE SET rank = MIN(rank, excluded.rank)",
        params![term, to_hiragana(reading), rank],
    )?;
    Ok(())
}

/// Adds what a Yomitan dictionary (a zip with `term_meta_bank_N.json` files) holds about pitch
/// accents (`pitch`) and frequencies (`freq`). Other files in the zip are ignored.
pub(super) fn import_yomitan(
    connection: &mut Connection,
    reader: impl Read + Seek,
) -> Result<MetadataStats, DictionaryError> {
    let mut archive = zip::ZipArchive::new(reader).map_err(zip_error)?;
    let names: Vec<String> = archive
        .file_names()
        .filter(|name| name.rsplit('/').next().is_some_and(is_meta_bank))
        .map(str::to_string)
        .collect();

    let transaction = connection.transaction()?;
    let mut stats = MetadataStats::default();
    for name in names {
        let mut text = String::new();
        archive
            .by_name(&name)
            .map_err(zip_error)?
            .read_to_string(&mut text)
            .map_err(io_error)?;
        let rows: Vec<Value> = serde_json::from_str(&text)?;
        for row in rows {
            add_meta_row(&transaction, &row, &mut stats)?;
        }
    }
    transaction.commit()?;
    Ok(stats)
}

fn is_meta_bank(file_name: &str) -> bool {
    file_name.starts_with("term_meta_bank_") && file_name.ends_with(".json")
}

fn add_meta_row(
    connection: &Connection,
    row: &Value,
    stats: &mut MetadataStats,
) -> Result<(), DictionaryError> {
    let (Some(term), Some(kind), Some(data)) = (
        row.get(0).and_then(Value::as_str),
        row.get(1).and_then(Value::as_str),
        row.get(2),
    ) else {
        return Ok(());
    };
    match kind {
        "freq" => {
            if let Some((reading, rank)) = frequency_of(data) {
                store_frequency(connection, term, &reading, rank)?;
                stats.frequency += 1;
            }
        }
        "pitch" => {
            let reading = data.get("reading").and_then(Value::as_str).unwrap_or(term);
            let positions: Vec<u8> = data
                .get("pitches")
                .and_then(Value::as_array)
                .map(|pitches| {
                    pitches
                        .iter()
                        .filter_map(|pitch| pitch.get("position").and_then(Value::as_u64))
                        .filter_map(|position| u8::try_from(position).ok())
                        .collect()
                })
                .unwrap_or_default();
            if !positions.is_empty() {
                connection.execute(
                    "INSERT OR REPLACE INTO pitch (term, reading, positions) VALUES (?1, ?2, ?3)",
                    params![
                        term,
                        to_hiragana(reading),
                        serde_json::to_string(&positions)?
                    ],
                )?;
                stats.pitch += 1;
            }
        }
        _ => {}
    }
    Ok(())
}

/// The reading (empty for any) and rank of a Yomitan `freq` value, which comes in several shapes:
/// a number, `{value, displayValue}` or `{reading, frequency: number | {value, displayValue}}`.
fn frequency_of(data: &Value) -> Option<(String, u32)> {
    if let Some(rank) = rank_of(data) {
        return Some((String::new(), rank));
    }
    let reading = data
        .get("reading")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let rank = data
        .get("frequency")
        .and_then(rank_of)
        .or_else(|| data.get("value").and_then(rank_of))?;
    Some((reading, rank))
}

fn rank_of(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|rank| u32::try_from(rank).ok()),
        Value::Object(map) => map.get("value").and_then(rank_of),
        _ => None,
    }
}

/// The downstep positions of a word with this reading.
pub(super) fn pitch_for(
    connection: &Connection,
    term: &str,
    reading: &str,
) -> Result<Vec<u8>, DictionaryError> {
    let mut statement =
        connection.prepare_cached("SELECT reading, positions FROM pitch WHERE term = ?1")?;
    let wanted = to_hiragana(reading);
    let rows = statement.query_map(params![term], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (stored, positions) = row?;
        if stored == wanted {
            return Ok(serde_json::from_str(&positions)?);
        }
    }
    Ok(Vec::new())
}

/// The frequency rank of a word with this reading (a rank for any reading counts too).
pub(super) fn frequency_for(
    connection: &Connection,
    term: &str,
    reading: &str,
) -> Result<Option<u32>, DictionaryError> {
    let mut statement = connection.prepare_cached(
        "SELECT MIN(rank) FROM frequency WHERE term = ?1 AND (reading = ?2 OR reading = '')",
    )?;
    let rank: Option<i64> =
        statement.query_row(params![term, to_hiragana(reading)], |row| row.get(0))?;
    Ok(rank.and_then(|rank| u32::try_from(rank).ok()))
}

fn io_error(error: std::io::Error) -> DictionaryError {
    DictionaryError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn zip_error(error: zip::result::ZipError) -> DictionaryError {
    DictionaryError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}
