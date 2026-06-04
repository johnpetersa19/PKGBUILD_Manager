/* pkgbuild-manager-release-gui — Push · Tags · Releases GUI
 *
 * Copyright 2026 johnpetersa19
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Standalone GTK4 + libadwaita window for commit/push, tag management
 * and release publishing (GitHub, GitLab, Codeberg, Generic Git).
 *
 * Usage:
 *   pkgbuild-manager-release-gui <path>
 */

mod win_state;
mod release_dialog;

use release_dialog::ReleaseWindow;
use adw::prelude::*;
use adw::Application;
use adw::gio::ApplicationFlags;
use gtk::glib;
use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};
use std::env;

const APP_ID: &str = "io.github.john.PkgbuildManager.ReleaseGui";

const GETTEXT_PACKAGE: &str = "pkgbuild_manager";
const LOCALEDIR: &str = match option_env!("PKGBUILD_MANAGER_LOCALEDIR_BUILD") {
    Some(v) => v,
    None    => "/usr/share/locale",
};

fn main() -> glib::ExitCode {
    setlocale(LocaleCategory::LcAll, "");

    let locale_dir = env::var("PKGBUILD_MANAGER_LOCALEDIR")
        .unwrap_or_else(|_| LOCALEDIR.to_string());

    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);

    let args: Vec<String> = env::args().collect();

    let target = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| ".".to_string());

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        let win = ReleaseWindow::new(app, target.clone());
        win.present();
    });

    app.run_with_args::<String>(&[])
}
