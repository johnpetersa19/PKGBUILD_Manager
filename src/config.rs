#![allow(dead_code)]

// These constants are injected by meson at build time via environment variables:
//   LOCALEDIR   → set by arch-meson to /usr/share/locale
//   PKGDATADIR  → set by arch-meson to /usr/share/pkgbuild_manager
//
// When building directly with `cargo build` (development), meson is not
// involved, so the env vars are absent. We fall back to /usr/share/locale
// (where the .mo files live after `makepkg -si`) so that a locally-compiled
// binary can still find translations at runtime via PKGBUILD_MANAGER_LOCALEDIR.
//
// Priority at runtime (see main.rs):
//   1. PKGBUILD_MANAGER_LOCALEDIR env var  (always wins — useful for dev/testing)
//   2. LOCALEDIR compiled in here

pub static VERSION:         &str = env!("CARGO_PKG_VERSION");
pub static GETTEXT_PACKAGE: &str = "pkgbuild_manager";

/// Directory where .mo files are installed.
/// Meson injects LOCALEDIR; falls back to /usr/share/locale for plain cargo builds.
pub static LOCALEDIR:  &str = match option_env!("LOCALEDIR") {
    Some(v) => v,
    None    => "/usr/share/locale",
};

/// Directory where package data files are installed.
/// Meson injects PKGDATADIR; falls back to /usr/share/pkgbuild_manager.
pub static PKGDATADIR: &str = match option_env!("PKGDATADIR") {
    Some(v) => v,
    None    => "/usr/share/pkgbuild_manager",
};
