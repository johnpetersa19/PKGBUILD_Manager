//! Single entry point for every PKGBUILD Manager GTK interface.

#[path = "../aur_push_gui/aur_dialog.rs"]
mod aur_dialog;
#[path = "../settings_gui/config.rs"]
mod config;
#[path = "../clean_gui/dialog.rs"]
mod clean_dialog;
#[path = "../aur_push_gui/release_dialog.rs"]
mod release_dialog;
#[path = "../settings_gui/app.rs"]
mod settings_app;
#[path = "../aur_push_gui/win_state.rs"]
mod win_state;
#[path = "../host.rs"]
mod host;

use adw::gio::ApplicationFlags;
use adw::prelude::*;
use adw::Application;
use aur_dialog::{RepoMode, UnifiedPushWindow};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory};
use release_dialog::ReleaseWindow;

const APP_ID: &str = "io.github.johnpetersa19.PkgbuildManager";
const GETTEXT_PACKAGE: &str = "pkgbuild_manager";
const LOCALEDIR: &str = match option_env!("PKGBUILD_MANAGER_LOCALEDIR_BUILD") {
    Some(value) => value,
    None => "/usr/share/locale",
};

fn main() -> gtk::glib::ExitCode {
    init_i18n();
    if host::is_flatpak() {
        if let Err(error) = install_flatpak_desktop_integration() {
            eprintln!("Could not install Flatpak desktop integration: {error}");
        }
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("settings") => settings_app::SettingsApp::new().run(),
        Some("push") => run_push(&args[1..]),
        Some("release") => run_release(&args[1..]),
        Some("clean") => run_clean(&args[1..]),
        None => settings_app::SettingsApp::new().run(),
        Some("help" | "--help" | "-h") => {
            print_usage();
            gtk::glib::ExitCode::SUCCESS
        }
        Some(command) => {
            eprintln!("Unknown GUI command: {command}");
            print_usage();
            gtk::glib::ExitCode::FAILURE
        }
    }
}

fn install_flatpak_desktop_integration() -> std::io::Result<()> {
    use std::fs;
    use std::path::PathBuf;

    fn copy_tree(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
        if !source.is_dir() { return Ok(()); }
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let destination = target.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_tree(&entry.path(), &destination)?;
            } else {
                fs::copy(entry.path(), destination)?;
            }
        }
        Ok(())
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set"))?;
    let source_scripts = PathBuf::from("/app/share/pkgbuild-manager/scripts");
    let target_scripts = home.join(".local/share/pkgbuild-manager/scripts");
    fs::create_dir_all(&target_scripts)?;
    if source_scripts.is_dir() {
        for entry in fs::read_dir(source_scripts)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let target = target_scripts.join(entry.file_name());
                fs::copy(entry.path(), &target)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
                }
            }
        }
    }

    let source_extension =
        PathBuf::from("/app/share/nautilus-python/extensions/pkgbuild_manager.py");
    let extension_dir = home.join(".local/share/nautilus-python/extensions");
    fs::create_dir_all(&extension_dir)?;
    if source_extension.is_file() {
        fs::copy(source_extension, extension_dir.join("pkgbuild_manager.py"))?;
    }
    copy_tree(
        std::path::Path::new("/app/share/locale"),
        &home.join(".local/share/locale"),
    )?;
    Ok(())
}

fn init_i18n() {
    setlocale(LocaleCategory::LcAll, "");
    let locale_dir = std::env::var("PKGBUILD_MANAGER_LOCALEDIR").unwrap_or_else(|_| {
        if host::is_flatpak() { "/app/share/locale".to_string() } else { LOCALEDIR.to_string() }
    });
    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);
}

fn run_push(args: &[String]) -> gtk::glib::ExitCode {
    let target = target_arg(args);
    let with_tag = args.iter().any(|arg| arg == "--tag");
    let forced_mode = args.iter().find_map(|arg| match arg.as_str() {
        "--mode=aur" => Some(RepoMode::Aur),
        "--mode=git" => Some(RepoMode::Git),
        "--mode=gitlab" => Some(RepoMode::GitLab),
        "--mode=codeberg" => Some(RepoMode::Codeberg),
        "--mode=generic" => Some(RepoMode::Generic),
        _ => None,
    });
    let mode = forced_mode.unwrap_or_else(|| RepoMode::detect(&target));

    let app = new_application("Push");
    app.connect_activate(move |app| {
        let window = UnifiedPushWindow::new(app, mode, target.clone(), with_tag);
        window.present();
    });
    app.run_with_args::<String>(&[])
}

fn run_release(args: &[String]) -> gtk::glib::ExitCode {
    let target = target_arg(args);
    let app = new_application("Release");
    app.connect_activate(move |app| {
        let window = ReleaseWindow::new(app, target.clone());
        window.present();
    });
    app.run_with_args::<String>(&[])
}

fn run_clean(args: &[String]) -> gtk::glib::ExitCode {
    let target = target_arg(args);
    let app = new_application("Clean");
    app.connect_activate(move |app| {
        clean_dialog::present(app, target.clone());
    });
    app.run_with_args::<String>(&[])
}

fn target_arg(args: &[String]) -> String {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| ".".to_string())
}

fn new_application(suffix: &str) -> Application {
    Application::builder()
        .application_id(format!("{APP_ID}.{suffix}"))
        .flags(ApplicationFlags::NON_UNIQUE)
        .build()
}

fn print_usage() {
    println!("Usage: pkgbuild-manager-gui <settings|push|release|clean> [path] [options]");
}
