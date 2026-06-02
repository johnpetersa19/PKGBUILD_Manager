/* pkgbuild-manager-aur-push — AUR Push GUI
 *
 * Copyright 2026 johnpetersa19
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * A standalone gtk4-rs + libadwaita window launched by the Nautilus script
 * "06_Push AUR" and "17_Push AUR Tag".
 *
 * Usage:
 *   pkgbuild-manager-aur-push <path>          # push only
 *   pkgbuild-manager-aur-push <path> --tag    # push + annotated tag
 */

mod window;

use adw::prelude::*;
use adw::Application;
use adw::gio::ApplicationFlags;
use gtk::glib;
use std::env;

const APP_ID: &str = "io.github.john.PkgbuildManager.AurPush";

fn main() -> glib::ExitCode {
    let args: Vec<String> = env::args().collect();

    // Path to PKGBUILD dir (first non-flag arg after argv[0])
    let target = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| ".".to_string());

    let with_tag = args.iter().any(|a| a == "--tag");

    // NON_UNIQUE: every invocation from the Nautilus script must open its
    // own independent window. Without this flag GLib detects an existing
    // D-Bus registration for APP_ID and silently re-activates the old
    // instance instead of showing a new window.
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        let win = window::AurPushWindow::new(app, target.clone(), with_tag);
        win.present();
    });

    app.run_with_args::<String>(&[])
}
