// pkgbuild-manager-settings — Rust/GTK4/Libadwaita
// Replaces the former Python app.py

mod app;
mod config;
mod win_state;

fn main() -> glib::ExitCode {
    use adw::prelude::*;
    let application = app::SettingsApp::new();
    application.run()
}
