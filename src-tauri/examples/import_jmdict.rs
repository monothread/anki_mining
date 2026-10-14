//! Builds the dictionary database from a `jmdict-simplified` JSON file and scans sample texts.
//!
//! Usage: cargo run --release --example import_jmdict -- <jmdict.json> <output.db> [text...]
//! Each text is scanned from its start, like hovering the beginning of a sentence.

use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use anki_mining_lib::dictionary::Dictionary;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [input, output, texts @ ..] = arguments.as_slice() else {
        eprintln!("usage: import_jmdict <jmdict.json> <output.db> [text...]");
        return ExitCode::from(2);
    };

    match run(Path::new(input), Path::new(output), texts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &Path, output: &Path, texts: &[String]) -> Result<(), String> {
    let file =
        File::open(input).map_err(|error| format!("cannot open {}: {error}", input.display()))?;
    let mut dictionary = Dictionary::open(output).map_err(|error| error.to_string())?;

    let started = Instant::now();
    let stats = dictionary
        .import_jmdict(BufReader::new(file))
        .map_err(|error| error.to_string())?;
    println!("imported {stats:?} in {:?}", started.elapsed());

    for text in texts {
        let started = Instant::now();
        let results = dictionary.scan(text).map_err(|error| error.to_string())?;
        println!(
            "-- {text}: {} matches in {:?}",
            results.len(),
            started.elapsed()
        );
        for result in results.iter().take(4) {
            let found = &result.entries[0];
            println!(
                "   {} -> {} [{}] {:?} | {}",
                result.source,
                found.entry.expression,
                found.entry.reading,
                found.reasons,
                found.entry.meanings.first().map_or("", String::as_str)
            );
        }
    }
    Ok(())
}
