#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
#[cfg(target_os = "macos")]
mod macos;
fn main() {
    #[cfg(target_os = "macos")]
    let result = macos::run();
    #[cfg(not(target_os = "macos"))]
    let result: Result<(), ()> = Err(());
    if result.is_err() {
        // Never include a URL, event, JS error, device label or bridge payload.
        std::process::exit(2);
    }
}
