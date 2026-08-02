use gettextrs::{bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory};

/// Initializes gettext using the desktop language rather than an accidentally
/// inherited `LC_ALL=C`/`LC_MESSAGES=C`.  Gettext already falls back to the
/// English msgid when no catalog (or no individual translation) exists.
pub fn init(package: &str, locale_dir: &str) {
    setlocale(LocaleCategory::LcAll, "");

    if let Some(language) = desktop_language() {
        // LANGUAGE may contain a priority list; setlocale needs one locale.
        let locale = language.split(':').next().unwrap_or(&language);
        let _ = setlocale(LocaleCategory::LcMessages, locale);
    }

    let _ = bindtextdomain(package, locale_dir);
    let _ = bind_textdomain_codeset(package, "UTF-8");
    let _ = textdomain(package);
}

fn desktop_language() -> Option<String> {
    // LANG is a complete locale name suitable for setlocale (for example
    // pt_BR.UTF-8). LANGUAGE is only a fallback because it commonly contains
    // gettext-only values such as `pt_BR` or a colon-separated preference list.
    ["LANG", "LANGUAGE"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.trim().is_empty())
}
