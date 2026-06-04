use std::path::Path;
use std::process::{Command, Stdio};

use super::{get_target_dir, write_error_log};

/// Run `shellcheck --shell=bash --exclude=SC2034,SC2154,SC2164 PKGBUILD`.
///
/// Excluded rules fire on valid PKGBUILD patterns that makepkg handles itself:
///   SC2034 – variables "unused" but consumed by makepkg's own scope
///   SC2154 – variables referenced before assignment (normal in PKGBUILD)
///   SC2164 – `cd` without error check (makepkg wraps everything safely)
///
/// Output is streamed live to the terminal. On failure a timestamped log is
/// written to ~/.local/share/pkgbuild_manager/logs/shellcheck-YYYYMMDD-HHMMSS.log
pub fn run(path: &Path) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;

    println!(
        ">>> shellcheck --shell=bash --exclude=SC2034,SC2154,SC2164 PKGBUILD (in {:?})",
        target_dir
    );

    let output = Command::new("shellcheck")
        .args(["--shell=bash", "--exclude=SC2034,SC2154,SC2164", "PKGBUILD"])
        .current_dir(&target_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                return anyhow::anyhow!(
                    "{}",
                    gettextrs::gettext(
                        "shellcheck not found. Install it with: sudo pacman -S shellcheck"
                    )
                );
            }
            anyhow::anyhow!(
                "{}: {}",
                gettextrs::gettext("Failed to run shellcheck"),
                e
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Stream output live to the terminal
    if !stdout.trim().is_empty() {
        print!("{}", stdout);
    }
    if !stderr.trim().is_empty() {
        eprint!("{}", stderr);
    }

    if !output.status.success() {
        // write_error_log is defined in mod.rs (shared with namcap)
        let log_path = write_error_log("shellcheck", &target_dir, &combined);
        match log_path {
            Ok(p) => eprintln!(
                "\n{}: {}",
                gettextrs::gettext("Error log written to"),
                p.display()
            ),
            Err(e) => eprintln!(
                "\n{}: {}",
                gettextrs::gettext("Warning: could not write error log"),
                e
            ),
        }

        return Err(anyhow::anyhow!(
            "{} {}",
            gettextrs::gettext("shellcheck found issues (see log above)"),
            output.status
        ));
    }

    Ok(())
}
