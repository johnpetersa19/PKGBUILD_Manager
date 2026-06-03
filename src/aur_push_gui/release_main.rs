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
use std::env;

const APP_ID: &str = "io.github.john.PkgbuildManager.ReleaseGui";

fn main() -> glib::ExitCode {
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
