//! Scores the Rust OCR engine on a folder of subtitle images (see `spikes/ocr`).
//!
//! Usage: cargo run --release --example ocr_eval -- <resources/ocr dir> <cases dir>
//! The cases directory holds PNG files and a `cases.json` list of `{ "file", "text" }`.

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use anki_mining_lib::normalizer::clean_text;
use anki_mining_lib::ocr::{lines_to_text, MeikiEngine, OcrEngine};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    file: String,
    text: String,
}

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [models, cases] = arguments.as_slice() else {
        eprintln!("usage: ocr_eval <resources/ocr dir> <cases dir>");
        return ExitCode::from(2);
    };
    match run(Path::new(models), Path::new(cases)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            current.push(
                (previous[j + 1] + 1)
                    .min(current[j] + 1)
                    .min(previous[j] + usize::from(ca != cb)),
            );
        }
        previous = current;
    }
    previous[b.len()]
}

fn run(models: &Path, cases_dir: &Path) -> Result<(), String> {
    let cases: Vec<Case> = serde_json::from_str(
        &fs::read_to_string(cases_dir.join("cases.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut engine = MeikiEngine::from_directory(models).map_err(|error| error.to_string())?;

    let first = image::open(cases_dir.join(&cases[0].file))
        .map_err(|error| error.to_string())?
        .to_rgb8();
    engine
        .recognize(&first)
        .map_err(|error| error.to_string())?;

    let (mut characters, mut errors, mut exact, mut elapsed) = (0, 0, 0, 0.0);
    let mut failures = Vec::new();
    for case in &cases {
        let image = image::open(cases_dir.join(&case.file))
            .map_err(|error| error.to_string())?
            .to_rgb8();
        let started = Instant::now();
        let lines = engine
            .recognize(&image)
            .map_err(|error| error.to_string())?;
        elapsed += started.elapsed().as_secs_f64();

        let found: Vec<char> = clean_text(&lines_to_text(&lines)).chars().collect();
        let expected: Vec<char> = clean_text(&case.text).chars().collect();
        let distance = edit_distance(&found, &expected);
        characters += expected.len();
        errors += distance;
        exact += usize::from(distance == 0);
        if distance > 0 {
            failures.push((
                case.file.clone(),
                found.iter().collect::<String>(),
                case.text.clone(),
            ));
        }
    }

    println!(
        "{} cases: char accuracy {:.2}%, exact {:.1}%, {:.0} ms/image",
        cases.len(),
        100.0 * (1.0 - errors as f64 / characters as f64),
        100.0 * exact as f64 / cases.len() as f64,
        1000.0 * elapsed / cases.len() as f64
    );
    for (file, found, expected) in failures.iter().take(10) {
        println!("  {file}\n    expected: {expected}\n    found:    {found}");
    }
    Ok(())
}
