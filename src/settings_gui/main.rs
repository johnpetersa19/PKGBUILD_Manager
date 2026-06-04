mod app;
mod config;
mod win_state;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};

// Domain name must match the .mo filename: pkgbuild_manager.mo (underscore, not hyphen)
const GETTEXT_PACKAGE: &str = "pkgbuild_manager";

// LOCALEDIR is injected at compile-time by Meson via build.rs / config.rs.
// We read it the same way the CLI binary does: prefer the env-var override
// (useful for `meson devenv` or manual testing), fall back to the compiled-in path.
const LOCALEDIR: &str = env!("PKGBUILD_MANAGER_LOCALEDIR_BUILD", "/usr/share/locale");

fn main() -> gtk::glib::ExitCode {
    setlocale(LocaleCategory::LcAll, "");

    let locale_dir = std::env::var("PKGBUILD_MANAGER_LOCALEDIR")
        .unwrap_or_else(|_| LOCALEDIR.to_string());

    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);

    let application = app::SettingsApp::new();
    application.run()
}
