#![allow(dead_code)]
// This file is the fallback used when building directly with `cargo build`
// (without meson). When built via meson (makepkg -si), this file is
// overwritten by configure_file() from config.rs.in with the correct
// install-time paths injected by the build system.
//
// At runtime, PKGBUILD_MANAGER_LOCALEDIR env var always overrides LOCALEDIR
// (see main.rs), which is useful for development/testing.
pub static VERSION: &str = "2.0.0";
pub static GETTEXT_PACKAGE: &str = "pkgbuild_manager";
pub static LOCALEDIR: &str = "/usr/share/locale";
pub static PKGDATADIR: &str = "/usr/share/pkgbuild_manager";
