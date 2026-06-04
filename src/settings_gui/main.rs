mod app;
mod config;
mod win_state;

use gettextrs::{LocaleCategory, setlocale, bindtextdomain, textdomain};

fn main() -> gtk::glib::ExitCode {
    setlocale(LocaleCategory::LcAll, "");
    bindtextdomain("pkgbuild-manager", "/usr/share/locale")
        .expect("Failed to bind text domain");
    textdomain("pkgbuild-manager")
        .expect("Failed to set text domain");

    let application = app::SettingsApp::new();
    application.run()
}
