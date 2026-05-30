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
pub fn run_command(cmd_name: &str, args: &[&str], dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!(">>> {} {} (in {:?})", cmd_name, args.join(" "), dir);
    let status = Command::new(cmd_name)
        .args(args)
        .current_dir(dir)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{} {} {}", cmd_name, args.join(" "), status).into())
    }
}

/// Read pkgname and pkgver from the PKGBUILD in the target directory.
/// Uses `makepkg --printsrcinfo` for reliable parsing.
pub fn read_pkgbuild_info(dir: &Path) -> Result<(String, String, String), Box<dyn std::error::Error>> {
    let output = Command::new("makepkg")
        .arg("--printsrcinfo")
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(gettext("Failed to read PKGBUILD info with makepkg --printsrcinfo").into());
    }

    let content = String::from_utf8_lossy(&output.stdout).to_string();
    let mut pkgname = String::from("unknown");
    let mut pkgver  = String::from("0");
    let mut pkgrel  = String::from("1");

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("pkgname = ") {
            pkgname = val.to_string();
        } else if let Some(val) = line.strip_prefix("pkgver = ") {
            pkgver = val.to_string();
        } else if let Some(val) = line.strip_prefix("pkgrel = ") {
            pkgrel = val.to_string();
        }
    }

    Ok((pkgname, pkgver, pkgrel))
}
