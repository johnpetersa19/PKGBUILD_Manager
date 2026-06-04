use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::get_target_dir;

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
            anyhow::anyhow!("failed to run shellcheck: {}", e)
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
        // Write error log
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

/// Write a timestamped error log to ~/.local/share/pkgbuild_manager/logs/.
/// Returns the path of the written file.
fn write_error_log(
    tool: &str,
    pkgbuild_dir: &Path,
    content: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| anyhow::anyhow!("HOME env var not set"))?;

    let log_dir = std::path::PathBuf::from(home)
        .join(".local/share/pkgbuild_manager/logs");
    std::fs::create_dir_all(&log_dir)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (date, time) = unix_to_datetime(now);
    let filename = format!("{}-{}-{}.log", tool, date, time);
    let log_path = log_dir.join(&filename);

    let mut file = std::fs::File::create(&log_path)?;
    writeln!(file, "=== {} error log ===", tool.to_uppercase())?;
    writeln!(file, "PKGBUILD directory : {}", pkgbuild_dir.display())?;
    writeln!(file, "Timestamp (UTC)    : {}-{}", date, time)?;
    writeln!(file, "")?;
    writeln!(file, "--- output ---")?;
    write!(file, "{}", content)?;

    Ok(log_path)
}

/// Minimal unix-epoch → (YYYYMMDD, HHMMSS) without external crates.
fn unix_to_datetime(secs: u64) -> (String, String) {
    let days = secs / 86400;
    let rem  = secs % 86400;
    let hh   = rem / 3600;
    let mm   = (rem % 3600) / 60;
    let ss   = rem % 60;

    let mut y: u64 = 1970;
    let mut d = days;
    loop {
        let dy = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
        if d < dy { break; }
        d -= dy;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let months = if leap {
        [31u64,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31u64,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut mo: u64 = 1;
    for &mdays in &months {
        if d < mdays { break; }
        d -= mdays;
        mo += 1;
    }
    let day = d + 1;

    (format!("{:04}{:02}{:02}", y, mo, day),
     format!("{:02}{:02}{:02}", hh, mm, ss))
}
