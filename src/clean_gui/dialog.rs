//! Protected preview dialog for the destructive "Clean Everything" action.

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{glib, Button, Label, ProgressBar, Switch};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;

pub fn present(app: &adw::Application, target: String) {
    let target = resolve_target(&target);
    let candidates = find_candidates(&target);
    let builder = crate::gui_blueprint::builder(include_str!(concat!(env!("OUT_DIR"), "/clean.ui")));
    let window: adw::ApplicationWindow = crate::gui_blueprint::object(&builder, "clean_window");
    let title: Label = crate::gui_blueprint::object(&builder, "clean_title");
    let path_label: Label = crate::gui_blueprint::object(&builder, "clean_path");
    let list: gtk::ListBox = crate::gui_blueprint::object(&builder, "clean_list");
    let unlock_row: adw::ActionRow = crate::gui_blueprint::object(&builder, "unlock_row");
    let unlock: Switch = crate::gui_blueprint::object(&builder, "unlock_switch");
    let status: Label = crate::gui_blueprint::object(&builder, "clean_status");
    let progress: ProgressBar = crate::gui_blueprint::object(&builder, "clean_progress");
    let cancel: Button = crate::gui_blueprint::object(&builder, "cancel_button");
    let clean: Button = crate::gui_blueprint::object(&builder, "clean_button");

    window.set_application(Some(app));
    window.set_title(Some(&gettext("Clean Everything")));
    title.set_label(&gettext("Files found for cleanup"));
    path_label.set_label(&target.to_string_lossy());
    if candidates.is_empty() {
        let row = adw::ActionRow::builder()
            .title(&gettext("Nothing to clean"))
            .subtitle(&gettext("No build artifacts were found."))
            .build();
        list.append(&row);
    } else {
        for candidate in &candidates {
            let relative = candidate.strip_prefix(&target).unwrap_or(candidate);
            let locked = contains_locked_entry(candidate);
            let subtitle = if locked {
                gettext("Contains protected files — permissions will be unlocked")
            } else if candidate.is_dir() {
                gettext("Directory and all its contents")
            } else {
                gettext("File")
            };
            let row = adw::ActionRow::builder()
                .title(relative.to_string_lossy())
                .subtitle(&subtitle)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name(if locked {
                "changes-prevent-symbolic"
            } else {
                "edit-delete-symbolic"
            }));
            list.append(&row);
        }
    }
    unlock_row.set_title(&gettext("Unlock cleanup"));
    unlock_row.set_subtitle(&gettext("I reviewed the list and authorize permanent removal"));
    unlock_row.set_activatable_widget(Some(&unlock));
    cancel.set_label(&gettext("Cancel"));
    clean.set_label(&gettext("Clean permanently"));

    unlock.connect_state_set(glib::clone!(
        #[strong] clean,
        move |_, state| {
            clean.set_sensitive(state && !candidates.is_empty());
            glib::Propagation::Proceed
        }
    ));
    cancel.connect_clicked(glib::clone!(#[weak] window, move |_| window.close()));

    clean.connect_clicked(glib::clone!(
        #[weak] window,
        #[weak] clean,
        #[weak] cancel,
        #[weak] unlock,
        #[weak] status,
        #[weak] progress,
        #[strong] target,
        move |_| {
            clean.set_sensitive(false);
            cancel.set_sensitive(false);
            unlock.set_sensitive(false);
            progress.set_visible(true);
            progress.pulse();
            status.set_label(&gettext("Unlocking protected files and cleaning…"));

            let (tx, rx) = async_channel::bounded::<Result<(), String>>(1);
            let path = target.clone();
            thread::spawn(move || {
                let result = crate::host::command("pkgbuild_manager")
                    .arg("clean-all")
                    .arg(path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|error| error.to_string())
                    .and_then(|output| {
                        if output.status.success() {
                            Ok(())
                        } else {
                            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
                        }
                    });
                let _ = tx.send_blocking(result);
            });

            glib::spawn_future_local(async move {
                match rx.recv().await {
                    Ok(Ok(())) => {
                        progress.set_fraction(1.0);
                        status.set_label(&gettext("Everything was cleaned successfully."));
                        cancel.set_label(&gettext("Close"));
                    }
                    Ok(Err(error)) => {
                        progress.set_visible(false);
                        status.set_label(&format!("{}\n{}", gettext("Cleanup failed."), error));
                        cancel.set_label(&gettext("Close"));
                    }
                    Err(error) => status.set_label(&error.to_string()),
                }
                cancel.set_sensitive(true);
                window.present();
            });
        }
    ));

    window.present();
}

fn resolve_target(value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_file() {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        path
    }
}

fn find_candidates(target: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for name in [".makepkg.lock", "src", "pkg"] {
        let path = target.join(name);
        if path.exists() {
            result.push(path);
        }
    }
    if let Ok(entries) = std::fs::read_dir(target) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if result.contains(&path) || name == ".git" {
                continue;
            }
            if (path.is_file() && name.contains(".pkg.tar."))
                || (path.is_dir() && (name.starts_with("_build") || is_bare_git_repo(&path)))
            {
                result.push(path);
            }
        }
    }
    result.sort();
    result
}

fn contains_locked_entry(path: &Path) -> bool {
    fn writable(path: &Path) -> bool {
        std::fs::metadata(path)
            .map(|metadata| !metadata.permissions().readonly())
            .unwrap_or(true)
    }
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !writable(path) {
        return true;
    }
    // Do not follow symlinks while scanning: a build directory can contain a
    // link back to an ancestor or to data outside the cleanup target.
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        if let Ok(entries) = std::fs::read_dir(path) {
            return entries.flatten().any(|entry| contains_locked_entry(&entry.path()));
        }
    }
    false
}

fn is_bare_git_repo(path: &Path) -> bool {
    path.join("HEAD").is_file()
        && path.join("config").is_file()
        && path.join("objects").is_dir()
        && path.join("refs").is_dir()
}
