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
use std::hash::{Hash, Hasher};
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

    // makepkg normally creates $srcdir as <PKGBUILD directory>/src. That is
    // destructive for source repositories (including this one) which already
    // track a real src/ directory: `makepkg -c` or `makepkg -C` can remove the
    // application's source code. Keep all makepkg work trees in a per-project
    // cache directory instead.
    let build_dir = makepkg_build_dir(&target_dir)?;
    println!(
        ">>> makepkg {} (in {:?}, BUILDDIR={:?})",
        args.join(" "),
        target_dir,
        build_dir
    );

    let status = crate::host::command("makepkg")
        .args(&args)
        .env("BUILDDIR", &build_dir)
        .current_dir(&target_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| {
            format!(
                "PKGBUILD Manager: {} 'makepkg'",
                gettext("failed to spawn command")
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "PKGBUILD Manager: {} 'makepkg' {} {}",
            gettext("command failed"),
            gettext("with status"),
            status
        ))
    }
}

fn makepkg_build_dir(target_dir: &Path) -> Result<PathBuf> {
    let cache_root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or_else(|| anyhow!(gettext("HOME environment variable is not set")))?;

    let build_dir = makepkg_build_dir_path(&cache_root, target_dir);
    fs::create_dir_all(&build_dir).with_context(|| {
        format!(
            "PKGBUILD Manager: {}: {}",
            gettext("failed to create makepkg build directory"),
            build_dir.display()
        )
    })?;
    Ok(build_dir)
}

fn makepkg_build_dir_path(cache_root: &Path, target_dir: &Path) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target_dir.hash(&mut hasher);
    let project = target_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("package")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    cache_root.join("pkgbuild-manager/makepkg").join(format!(
        "{}-{:016x}",
        project,
        hasher.finish()
    ))
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
        .with_context(|| {
            format!(
                "PKGBUILD Manager: {}",
                gettext("failed to run makepkg --printsrcinfo")
            )
        })?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "{}: {}",
            gettextrs::gettext("makepkg --printsrcinfo failed"),
            err_msg.trim()
        ));
    }

    fs::write(dir.join(".SRCINFO"), &output.stdout).with_context(|| {
        format!(
            "PKGBUILD Manager: {}",
            gettext("failed to write .SRCINFO")
        )
    })?;

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
    writeln!(
        file,
        "=== {}: {} ===",
        gettext("error log"),
        tool.to_uppercase()
    )?;
    writeln!(
        file,
        "{}: {}",
        gettext("PKGBUILD directory"),
        pkgbuild_dir.display()
    )?;
    writeln!(file, "{}: {}-{}", gettext("Timestamp (UTC)"), date, time)?;
    writeln!(file)?;
    writeln!(file, "--- {} ---", gettext("output"))?;
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

#[cfg(test)]
mod tests {
    use super::makepkg_build_dir_path;
    use std::path::Path;

    #[test]
    fn makepkg_work_tree_is_outside_the_source_repository() {
        let repository = Path::new("/home/user/project");
        let build_dir = makepkg_build_dir_path(Path::new("/home/user/.cache"), repository);

        assert!(build_dir.starts_with("/home/user/.cache/pkgbuild-manager/makepkg"));
        assert!(!build_dir.starts_with(repository));
    }

    #[test]
    fn makepkg_work_trees_are_distinct_per_repository() {
        let cache = Path::new("/home/user/.cache");
        let first = makepkg_build_dir_path(cache, Path::new("/work/first/package"));
        let second = makepkg_build_dir_path(cache, Path::new("/work/second/package"));

        assert_ne!(first, second);
    }
}
