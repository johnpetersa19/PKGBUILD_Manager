/* pkgbuild-manager-aur-push — Unified Push GUI
 *
 * Copyright 2026 johnpetersa19
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * A standalone gtk4-rs + libadwaita window launched by the Nautilus scripts.
 *
 * The window auto-detects the repository type from the selected folder:
 *   - AUR repo      (PKGBUILD + remote = aur.archlinux.org)  → AUR push mode
 *   - GitLab repo   (remote contains gitlab.com)             → GitLab push mode
 *   - Codeberg repo (remote contains codeberg.org)           → Codeberg push mode
 *   - Other Git     (.git present, not matched above)        → Generic Git mode
 *   - Other                                                  → informational page
 *
 * Usage:
 *   pkgbuild-manager-aur-push <path>               # auto-detect
 *   pkgbuild-manager-aur-push <path> --tag          # auto-detect + annotated tag
 *   pkgbuild-manager-aur-push <path> --mode=aur
 *   pkgbuild-manager-aur-push <path> --mode=git
 *   pkgbuild-manager-aur-push <path> --mode=gitlab
 *   pkgbuild-manager-aur-push <path> --mode=codeberg
 *   pkgbuild-manager-aur-push <path> --mode=generic
 */

mod win_state;
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

    let target = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| ".".to_string());

    let with_tag = args.iter().any(|a| a == "--tag");

    // Optional forced mode
    let forced_mode: Option<RepoMode> = args.iter().find_map(|a| match a.as_str() {
        "--mode=aur"      => Some(RepoMode::Aur),
        "--mode=git"      => Some(RepoMode::Git),
        "--mode=gitlab"   => Some(RepoMode::GitLab),
        "--mode=codeberg" => Some(RepoMode::Codeberg),
        "--mode=generic"  => Some(RepoMode::Generic),
        _                 => None,
    });

    let mode = forced_mode.unwrap_or_else(|| RepoMode::detect(&target));

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
