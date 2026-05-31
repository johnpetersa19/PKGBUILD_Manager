pub mod build;
pub mod install;
pub mod checksums;
pub mod srcinfo;
pub mod namcap;
pub mod shellcheck;
pub mod clean;
pub mod aur_push;

use std::path::{Path, PathBuf};
use std::process::Command;
use gettextrs::gettext;

/// Resolve a path to the directory containing PKGBUILD.
/// Accepts either a directory or a PKGBUILD file path directly.
pub fn get_target_dir(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let resolved = path.canonicalize()?;
    let mut target = resolved.clone();
    if resolved.is_file() {
        target = resolved.parent()
            .ok_or_else(|| gettext("Failed to resolve parent directory"))?
            .to_path_buf();
    }

    if !target.exists() {
        return Err(format!("{}: {:?}", gettext("Directory does not exist"), target).into());
    }
    let pkgbuild_path = target.join("PKGBUILD");
    if !pkgbuild_path.exists() {
        return Err(format!("{}: {:?}", gettext("No PKGBUILD found in directory"), target).into());
    }

    Ok(target)
}

/// Run a command in a directory and stream its output to stdout/stderr.
/// On failure, the error message includes the captured stderr for better diagnostics.
pub fn run_command(cmd_name: &str, args: &[&str], dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!(">>> {} {} (in {:?})", cmd_name, args.join(" "), dir);
    let output = Command::new(cmd_name)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| -> Box<dyn std::error::Error> {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("{} '{}'", gettext("Command not found"), cmd_name).into()
            } else {
                e.into()
            }
        })?;

    // Stream stdout/stderr to the terminal as the original .status() did
    use std::io::Write;
    let _ = std::io::stdout().write_all(&output.stdout);
    let _ = std::io::stderr().write_all(&output.stderr);

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            format!("{} {}", cmd_name, output.status)
        } else {
            format!("{} {}: {}", cmd_name, output.status, stderr.trim())
        };
        Err(detail.into())
    }
}

/// Collect all *.pkg.tar.* file names in `dir`.
/// Shared between namcap and clean to avoid duplicating directory traversal logic.
pub fn collect_pkg_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    (e.path().is_file() && name.contains(".pkg.tar.")).then_some(name)
                })
                .collect()
        })
        .unwrap_or_default()
}
