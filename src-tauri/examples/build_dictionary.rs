//! Builds the dictionary that is bundled with the app: JMdict (English meanings), the pitch
//! accents (Kanjium `accents.txt`) and the word frequencies (`build_frequency` output).
//!
//! Usage: cargo run --release --example build_dictionary -- <jmdict.json> <accents.txt> <frequency.tsv> <output.db>

use std::env;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::process::ExitCode;

use anki_mining_lib::dictionary::{Dictionary, BUNDLE_VERSION, SOURCE_KEY};

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [jmdict, accents, frequency, output] = arguments.as_slice() else {
        eprintln!(
            "usage: build_dictionary <jmdict.json> <accents.txt> <frequency.tsv> <output.db>"
        );
        return ExitCode::from(2);
    };
    match run(
        Path::new(jmdict),
        Path::new(accents),
        Path::new(frequency),
        Path::new(output),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn open(path: &Path) -> Result<BufReader<File>, String> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))
}

fn run(jmdict: &Path, accents: &Path, frequency: &Path, output: &Path) -> Result<(), String> {
    if output.exists() {
        fs::remove_file(output).map_err(|error| error.to_string())?;
    }
    let mut dictionary = Dictionary::open(output).map_err(|error| error.to_string())?;
    let entries = dictionary
        .import_jmdict(open(jmdict)?)
        .map_err(|error| error.to_string())?;
    println!("JMdict: {entries:?}");
    let pitch = dictionary
        .import_pitch_accents(open(accents)?)
        .map_err(|error| error.to_string())?;
    println!("Pitch accents: {pitch:?}");
    let ranks = dictionary
        .import_frequencies(open(frequency)?)
        .map_err(|error| error.to_string())?;
    println!("Frequencies: {ranks:?}");
    dictionary
        .set_meta(SOURCE_KEY, &format!("bundled:{BUNDLE_VERSION}"))
        .map_err(|error| error.to_string())
}
