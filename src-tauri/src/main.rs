#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = anki_mining_lib::run() {
        eprintln!("failed to run the application: {error}");
        std::process::exit(1);
    }
}
