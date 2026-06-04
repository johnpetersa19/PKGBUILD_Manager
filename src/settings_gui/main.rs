mod app;
mod config;
mod win_state;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};

// Domain name must match the .mo filename: pkgbuild_manager.mo (underscore, not hyphen)
const GETTEXT_PACKAGE: &str = "pkgbuild_manager";

// Compile-time LOCALEDIR injected by Meson (via PKGBUILD_MANAGER_LOCALEDIR_BUILD env var
// set in build.rs). Falls back to the standard system path if not set.
const LOCALEDIR: &str = match option_env!("PKGBUILD_MANAGER_LOCALEDIR_BUILD") {
    Some(v) => v,
    None    => "/usr/share/locale",
};

fn main() -> gtk::glib::ExitCode {
    setlocale(LocaleCategory::LcAll, "");

    // Runtime override takes priority (useful for `meson devenv` or manual testing
    // without running `meson install`).
    let locale_dir = std::env::var("PKGBUILD_MANAGER_LOCALEDIR")
        .unwrap_or_else(|_| LOCALEDIR.to_string());

    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);

    let application = app::SettingsApp::new();
    application.run()
}
