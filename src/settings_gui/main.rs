mod app;
mod config;
mod win_state;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};
use config::{GETTEXT_PACKAGE, LOCALEDIR};

fn main() -> gtk::glib::ExitCode {
    setlocale(LocaleCategory::LcAll, "");

    // Respects PKGBUILD_MANAGER_LOCALEDIR override for dev/testing without
    // needing `meson install`. Falls back to LOCALEDIR set by Meson at build time.
    let locale_dir = std::env::var("PKGBUILD_MANAGER_LOCALEDIR")
        .unwrap_or_else(|_| LOCALEDIR.to_string());

    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);

    let application = app::SettingsApp::new();
    application.run()
}
