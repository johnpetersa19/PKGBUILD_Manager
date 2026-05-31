pub mod build;
pub mod install;
pub mod checksums;
pub mod srcinfo;
pub mod namcap;
pub mod shellcheck;
pub mod clean;
pub mod aur_push;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

/// Run a command in a directory, herdando o TTY do processo pai.
///
/// Usa Stdio::inherit() em stdin/stdout/stderr para que comandos
/// interativos (como `makepkg -si` que chama `pacman`) possam exibir
/// prompts e receber respostas do usuário normalmente.
pub fn run_command(cmd_name: &str, args: &[&str], dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!(">>> {} {} (in {:?})", cmd_name, args.join(" "), dir);

    let status = Command::new(cmd_name)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| -> Box<dyn std::error::Error> {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("{} '{}'", gettext("Command not found"), cmd_name).into()
            } else {
                e.into()
            }
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} {} {}",
            gettext("Command failed:"),
            cmd_name,
            status
        ).into())
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
