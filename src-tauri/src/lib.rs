pub mod anki;
pub mod app;
pub mod capture;
pub mod cards;
pub mod config;
pub mod dictionary;
pub mod health;
pub mod hover;
pub mod normalizer;
pub mod ocr;
pub mod pointer;
pub mod sources;
pub mod text_offsets;
pub mod tokenizer;
pub mod types;

pub fn run() -> tauri::Result<()> {
    app::run()
}
