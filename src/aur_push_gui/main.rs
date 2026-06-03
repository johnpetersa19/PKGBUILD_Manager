/* pkgbuild-manager-aur-push — Unified Push GUI
 *
 * Copyright 2026 johnpetersa19
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * A standalone gtk4-rs + libadwaita window launched by the Nautilus scripts
 * "06_Push AUR", "17_Push AUR Tag", "Push Git" and "Push Git Tag".
 *
 * The window auto-detects the repository type from the selected folder:
 *   - AUR repo  (PKGBUILD + remote = aur.archlinux.org) → AUR push mode
 *   - Git repo  (.git present, not AUR)                 → Git push mode
 *   - Other                                              → informational page
 *
 * Usage:
 *   pkgbuild-manager-aur-push <path>          # auto-detect, push only
 *   pkgbuild-manager-aur-push <path> --tag    # auto-detect, push + annotated tag
 *   pkgbuild-manager-aur-push <path> --mode=aur   # force AUR mode
 *   pkgbuild-manager-aur-push <path> --mode=git   # force Git mode
 */

mod aur_dialog;

use aur_dialog::{RepoMode, UnifiedPushWindow};
use adw::prelude::*;
use adw::Application;
use adw::gio::ApplicationFlags;
use gtk::glib;
use std::env;

const APP_ID: &str = "io.github.john.PkgbuildManager.AurPush";

fn main() -> glib::ExitCode {
    let args: Vec<String> = env::args().collect();

    // Path to project directory (first non-flag arg after argv[0])
    let target = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| ".".to_string());

    let with_tag = args.iter().any(|a| a == "--tag");

    // Optional forced mode via --mode=aur or --mode=git
    let forced_mode: Option<RepoMode> = args.iter().find_map(|a| {
        if a == "--mode=aur" { Some(RepoMode::Aur) }
        else if a == "--mode=git" { Some(RepoMode::Git) }
        else { None }
    });

    // Auto-detect unless caller forced a mode
    let mode = forced_mode.unwrap_or_else(|| RepoMode::detect(&target));

    // NON_UNIQUE: every invocation from the Nautilus script must open its
    // own independent window.
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        let win = UnifiedPushWindow::new(app, mode, target.clone(), with_tag);
        win.present();
    });

    app.run_with_args::<String>(&[])
}
