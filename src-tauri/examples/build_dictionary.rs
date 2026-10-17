//! Builds the dictionary that is bundled with the app from a `jmdict-simplified` JSON file.
//!
//! Usage: cargo run --release --example build_dictionary -- <jmdict.json> <output.db>

use std::env;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::process::ExitCode;

use anki_mining_lib::dictionary::{Dictionary, BUNDLE_VERSION, SOURCE_KEY};

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [jmdict, output] = arguments.as_slice() else {
        eprintln!("usage: build_dictionary <jmdict.json> <output.db>");
        return ExitCode::from(2);
    };
    match run(Path::new(jmdict), Path::new(output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(jmdict: &Path, output: &Path) -> Result<(), String> {
    let file = File::open(jmdict)
        .map(BufReader::new)
        .map_err(|error| format!("cannot open {}: {error}", jmdict.display()))?;
    if output.exists() {
        fs::remove_file(output).map_err(|error| error.to_string())?;
    }
    let mut dictionary = Dictionary::open(output).map_err(|error| error.to_string())?;
    let stats = dictionary
        .import_jmdict(file)
        .map_err(|error| error.to_string())?;
    println!("JMdict: {stats:?}");
    dictionary
        .set_meta(SOURCE_KEY, &format!("bundled:{BUNDLE_VERSION}"))
        .map_err(|error| error.to_string())
}
