pub mod anki;
pub mod app;
pub mod audio;
pub mod capture;
pub mod cards;
pub mod config;
pub mod dictionary;
pub mod hover;
pub mod media;
pub mod normalizer;
pub mod ocr;
pub mod pointer;
pub mod profiles;
pub mod sources;
pub mod text_offsets;
pub mod tokenizer;
pub mod tts;
pub mod types;

/// The X11 tests share one display (and one of them changes `$DISPLAY`), so they take turns.
#[cfg(test)]
pub(crate) fn x11_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn run() -> tauri::Result<()> {
    app::run()
}
