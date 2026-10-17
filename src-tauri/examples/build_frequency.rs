//! Counts how often each word is used in a file of Japanese sentences and writes a frequency
//! list: `word<TAB><TAB>rank` per line, the most used word first (rank 1).
//!
//! Usage: cargo run --release --example build_frequency -- <sentences.tsv> <output.tsv>
//! The input has one sentence per line; when a line has tabs the last column is the sentence
//! (Tatoeba exports are `id<TAB>language<TAB>sentence`).

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;

use anki_mining_lib::tokenizer::Tokenizer;

/// Words used fewer times than this are left out: their order would be noise.
const MIN_COUNT: usize = 3;
/// Parts of speech that are not words someone looks up.
const IGNORED_POS: [&str; 3] = ["補助記号", "空白", "未知語"];

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [input, output] = arguments.as_slice() else {
        eprintln!("usage: build_frequency <sentences.tsv> <output.tsv>");
        return ExitCode::from(2);
    };
    match run(Path::new(input), Path::new(output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &Path, output: &Path) -> Result<(), String> {
    let tokenizer = Tokenizer::new().map_err(|error| error.to_string())?;
    let reader = BufReader::new(
        File::open(input).map_err(|error| format!("cannot open {}: {error}", input.display()))?,
    );

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut sentences = 0;
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        let sentence = line.rsplit('\t').next().unwrap_or_default().trim();
        if sentence.is_empty() {
            continue;
        }
        sentences += 1;
        for token in tokenizer
            .tokenize(sentence)
            .map_err(|error| error.to_string())?
        {
            if IGNORED_POS.contains(&token.pos.as_str()) || token.lemma.trim().is_empty() {
                continue;
            }
            *counts.entry(token.lemma).or_default() += 1;
        }
    }

    let mut ranked: Vec<(String, usize)> = counts
        .into_iter()
        .filter(|(_, count)| *count >= MIN_COUNT)
        .collect();
    // The most used first; ties in text order so the output does not change between runs.
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let mut writer = BufWriter::new(File::create(output).map_err(|error| error.to_string())?);
    for (index, (word, _)) in ranked.iter().enumerate() {
        writeln!(writer, "{word}\t\t{}", index + 1).map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())?;
    println!(
        "{sentences} sentences, {} words with at least {MIN_COUNT} uses",
        ranked.len()
    );
    Ok(())
}
