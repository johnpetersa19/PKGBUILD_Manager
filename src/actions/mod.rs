pub mod build;
pub mod install;
pub mod checksums;
pub mod srcinfo;
pub mod namcap;
pub mod shellcheck;
pub mod clean;
pub mod aur_push;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use gettextrs::gettext;

/// Resolve a path to the directory containing PKGBUILD.
/// Accepts either a directory or a PKGBUILD file path directly.
pub fn get_target_dir(path: &Path) -> Result<PathBuf> {
    let resolved = path
        .canonicalize()
        .with_context(|| {
            format!(
                "PKGBUILD Manager: {} {:?}",
                gettext("failed to canonicalize path"),
                path
            )
        })?;

    let mut target = resolved.clone();
    if resolved.is_file() {
        target = resolved
            .parent()
            .ok_or_else(|| anyhow!(gettext("Failed to resolve parent directory")))?
            .to_path_buf();
    }

    if !target.exists() {
        return Err(anyhow!(
            "{}: {:?}",
            gettext("Directory does not exist"),
            target
        ));
    }
    let pkgbuild_path = target.join("PKGBUILD");
    if !pkgbuild_path.exists() {
        return Err(anyhow!(
            "{}: {:?}",
            gettext("No PKGBUILD found in directory"),
            target
        ));
    }

    Ok(target)
}

/// Run a command in a directory, herdando o TTY do processo pai.
///
/// Usa Stdio::inherit() em stdin/stdout/stderr para que comandos
/// interativos (como `makepkg -si` que chama `pacman`) possam exibir
/// prompts e receber respostas do usuário normalmente.
pub fn run_command(cmd_name: &str, args: &[&str], dir: &Path) -> Result<()> {
    println!(">>> {} {} (in {:?})", cmd_name, args.join(" "), dir);

    let status = Command::new(cmd_name)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| {
            format!(
                "PKGBUILD Manager: {} '{}'",
                gettext("failed to spawn command"),
                cmd_name
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "PKGBUILD Manager: {} '{}' {} {}",
            gettext("command failed"),
            cmd_name,
            gettext("with status"),
            status
        ))
    }
}

/// Helper to run makepkg with a base set of arguments plus extra flags.
pub fn run_makepkg(path: &Path, base_args: &[&str], extra_flags: &[&str]) -> Result<()> {
    let target_dir = get_target_dir(path)?;
    let mut args: Vec<&str> = base_args.to_vec();
    args.extend_from_slice(extra_flags);
    run_command("makepkg", &args, &target_dir)
}

/// Collect all *.pkg.tar.* file names in `dir`.
/// Shared between namcap and clean to avoid duplicating directory traversal logic.
pub fn collect_pkg_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
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

/// Regenerate .SRCINFO using `makepkg --printsrcinfo`, write it to disk,
/// and return the generated content as String.
pub fn regenerate_srcinfo(dir: &Path) -> Result<String> {
    // FIX: verify PKGBUILD exists before calling makepkg to produce a clear error
    if !dir.join("PKGBUILD").exists() {
        return Err(anyhow!(
            "{}: {:?}",
            gettext("No PKGBUILD found in directory"),
            dir
        ));
    }

    println!("{} {:?}", gettextrs::gettext(">>> Regenerating .SRCINFO in"), dir);

    let output = Command::new("makepkg")
        .arg("--printsrcinfo")
        .current_dir(dir)
        .output()
        .with_context(|| "PKGBUILD Manager: failed to run makepkg --printsrcinfo")?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "{}: {}",
            gettextrs::gettext("makepkg --printsrcinfo failed"),
            err_msg.trim()
        ));
    }

    fs::write(dir.join(".SRCINFO"), &output.stdout)
        .with_context(|| "PKGBUILD Manager: failed to write .SRCINFO")?;

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
