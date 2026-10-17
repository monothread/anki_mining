//! Builds the dictionary that is bundled with the app: JMdict (English) plus the Portuguese
//! meanings from the Portuguese Wiktionary.
//!
//! Usage: cargo run --release --example build_dictionary -- <jmdict.json> <ptwiktionary.jsonl> <output.db>

use std::env;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::process::ExitCode;

use anki_mining_lib::dictionary::{Dictionary, BUNDLE_VERSION, SOURCE_KEY};

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [jmdict, portuguese, output] = arguments.as_slice() else {
        eprintln!("usage: build_dictionary <jmdict.json> <ptwiktionary.jsonl> <output.db>");
        return ExitCode::from(2);
    };
    match run(Path::new(jmdict), Path::new(portuguese), Path::new(output)) {
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

fn run(jmdict: &Path, portuguese: &Path, output: &Path) -> Result<(), String> {
    if output.exists() {
        fs::remove_file(output).map_err(|error| error.to_string())?;
    }
    let mut dictionary = Dictionary::open(output).map_err(|error| error.to_string())?;
    let english = dictionary
        .import_jmdict(open(jmdict)?)
        .map_err(|error| error.to_string())?;
    println!("JMdict: {english:?}");
    let portuguese = dictionary
        .import_portuguese(open(portuguese)?)
        .map_err(|error| error.to_string())?;
    println!("Portuguese: {portuguese:?}");
    dictionary
        .set_meta(SOURCE_KEY, &format!("bundled:{BUNDLE_VERSION}"))
        .map_err(|error| error.to_string())
}
