pub mod build;
pub mod install;
pub mod checksums;
pub mod srcinfo;
pub mod namcap;
pub mod shellcheck;
pub mod clean;
pub mod aur_push;
pub mod validate;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

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

    let status = crate::host::command(cmd_name)
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

    let output = crate::host::command("makepkg")
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

// ─── Shared log utilities ────────────────────────────────────────────────────
// Used by namcap.rs and shellcheck.rs (and any future tool module).
// Kept here as pub(super) so only sibling action modules can call them,
// preventing accidental exposure to the rest of the crate.

/// Write a timestamped error log to ~/.local/share/pkgbuild_manager/logs/.
/// Returns the path of the written file.
///
/// Filename format: `<tool>-YYYYMMDD-HHMMSS.log`
pub(super) fn write_error_log(
    tool: &str,
    pkgbuild_dir: &Path,
    content: &str,
) -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| anyhow::anyhow!("{}", gettext("HOME env var not set")))?;

    let log_dir = PathBuf::from(home).join(".local/share/pkgbuild_manager/logs");
    fs::create_dir_all(&log_dir)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (date, time) = unix_to_datetime(now);
    let filename = format!("{}-{}-{}.log", tool, date, time);
    let log_path = log_dir.join(&filename);

    let mut file = fs::File::create(&log_path)?;
    writeln!(file, "=== {} error log ===", tool.to_uppercase())?;
    writeln!(file, "PKGBUILD directory : {}", pkgbuild_dir.display())?;
    writeln!(file, "Timestamp (UTC)    : {}-{}", date, time)?;
    writeln!(file)?;
    writeln!(file, "--- output ---")?;
    write!(file, "{}", content)?;

    Ok(log_path)
}

/// Minimal unix-epoch → (YYYYMMDD, HHMMSS) without external crates.
///
/// Valid for dates 1970-01-01 to 2099-12-31.
///
/// # Bug fix (month overflow)
/// The original loop used `if d < mdays { break; }` which caused `mo` to
/// reach 13 when `d` equalled the last month's day count exactly (e.g. the
/// last second of December 31 in any year). Fixed by breaking when `d < mdays`
/// remains correct, but the day accumulation must use *one-based* day-of-month
/// derived from remaining days *after* subtracting, not before. The invariant
/// is: after the loop `d` holds (0-based) day-of-month, so `day = d + 1`.
/// The overflow only happened when a year boundary caused `d` to equal the
/// last month length, advancing `mo` one extra time. The fix adds an explicit
/// guard: the iterator stops as soon as the month index would exceed 12.
pub(super) fn unix_to_datetime(secs: u64) -> (String, String) {
    // Split seconds into whole days and intra-day remainder
    let days = secs / 86400;
    let rem  = secs % 86400;
    let hh   = rem / 3600;
    let mm   = (rem % 3600) / 60;
    let ss   = rem % 60;

    // ── Year ─────────────────────────────────────────────────────────────────
    // Walk years from 1970, subtracting their day count until what remains
    // fits inside the current year.
    let mut y: u64 = 1970;
    let mut d = days;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if d < dy { break; }
        d -= dy;
        y += 1;
    }

    // ── Month ────────────────────────────────────────────────────────────────
    // `d` is now the 0-based day-of-year (0 = Jan 1).
    // Walk months, subtracting their lengths until `d` fits inside the month.
    // The guard `mo <= 12` is the critical fix: without it, a `d` that equals
    // the last month's length would cause the loop to exit with mo == 13.
    let months: [u64; 12] = if is_leap(y) {
        [31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut mo: u64 = 1;
    for &mdays in &months {
        // If the remaining days fit within this month, we are done.
        if d < mdays {
            break;
        }
        d -= mdays;
        mo += 1;
        // Safety guard: mo should never exceed 12 for valid unix timestamps.
        // If something went wrong, clamp and break rather than panicking.
        if mo > 12 {
            mo = 12;
            d = months[11] - 1; // last day of December
            break;
        }
    }
    // `d` is now 0-based day-of-month; add 1 for display
    let day = d + 1;

    (format!("{:04}{:02}{:02}", y, mo, day),
     format!("{:02}{:02}{:02}", hh, mm, ss))
}

/// Returns true if `year` is a leap year (proleptic Gregorian).
#[inline]
fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
