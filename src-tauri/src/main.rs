#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = celestite_app::run() {
        eprintln!("Celestite could not start: {error}");
        std::process::exit(1);
    }
}
