//! Prints what the OCR sees in an image: each line and the position of every character.
//!
//! Usage: cargo run --release --example ocr_boxes -- <resources/ocr dir> <image.png>

use std::env;
use std::path::Path;
use std::process::ExitCode;

use anki_mining_lib::ocr::{MeikiEngine, OcrEngine};

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let [models, image_path] = arguments.as_slice() else {
        eprintln!("usage: ocr_boxes <resources/ocr dir> <image.png>");
        return ExitCode::from(2);
    };
    match run(Path::new(models), Path::new(image_path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(models: &Path, image_path: &Path) -> Result<(), String> {
    let image = image::open(image_path)
        .map_err(|error| error.to_string())?
        .to_rgb8();
    let mut engine = MeikiEngine::from_directory(models).map_err(|error| error.to_string())?;
    for line in engine
        .recognize(&image)
        .map_err(|error| error.to_string())?
    {
        println!("line {:?}: {}", line.bbox, line.text);
        for (index, ocr_char) in line.chars.iter().enumerate() {
            let [x1, y1, x2, y2] = ocr_char.bbox;
            println!(
                "  {index:>2} {} center=({}, {}) conf={:.2}",
                ocr_char.character,
                (x1 + x2) / 2,
                (y1 + y2) / 2,
                ocr_char.confidence
            );
        }
    }
    Ok(())
}
