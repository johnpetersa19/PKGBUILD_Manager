use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use super::{get_target_dir, collect_pkg_files};

pub fn run(path: &Path) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;

    let mut args = vec!["PKGBUILD".to_string()];
    // Reuse shared helper — avoids duplicating read_dir logic from clean.rs
    args.extend(collect_pkg_files(&target_dir));

    let args_slices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    println!(">>> namcap {} (in {:?})", args_slices.join(" "), target_dir);

    // FIX: namcap always exits with code 0, even when it finds errors.
    // We need to capture output to scan for E: lines, but we also want the
    // user to see output live. Strategy: inherit stdout/stderr for live display
    // AND collect via piped output in a second pass. Since namcap is fast,
    // we run it once with output capture and print it ourselves immediately.
    let output = Command::new("namcap")
        .args(&args_slices)
        .current_dir(&target_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Stream output to terminal so user sees it live
    if !stdout.trim().is_empty() {
        print!("{}", stdout);
    }
    if !stderr.trim().is_empty() {
        eprint!("{}", stderr);
    }

    // Collect lines that start with "E:" — these are namcap errors
    let error_lines: Vec<&str> = combined
        .lines()
        .filter(|l| {
            let t = l.trim();
            // namcap output format: "PKGBUILD (pkgname) E: some message"
            // or simply: "E: some message"
            t.contains(" E: ") || t.starts_with("E: ")
        })
        .collect();

    // Also propagate a non-zero exit if namcap ever starts using it
    let exit_failed = !output.status.success();

    if !error_lines.is_empty() || exit_failed {
        // Write error log
        let log_path = write_error_log("namcap", &target_dir, &combined);
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
    }

    if !error_lines.is_empty() {
        return Err(anyhow::anyhow!(
            "{} ({} {}):",
            gettextrs::gettext("namcap reported errors"),
            error_lines.len(),
            if error_lines.len() == 1 {
                gettextrs::gettext("error")
            } else {
                gettextrs::gettext("errors")
            },
        ));
    }

    if exit_failed {
        return Err(anyhow::anyhow!(
            "{} {}",
            gettextrs::gettext("namcap failed with status"),
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
        .map_err(|_| anyhow::anyhow!("{}", gettextrs::gettext("HOME env var not set")))?;

    let log_dir = std::path::PathBuf::from(home)
        .join(".local/share/pkgbuild_manager/logs");
    std::fs::create_dir_all(&log_dir)?;

    // Timestamp: YYYYMMDD-HHMMSS
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple deterministic timestamp from unix epoch (no chrono dependency)
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
    // Days since epoch
    let days = secs / 86400;
    let rem  = secs % 86400;
    let hh   = rem / 3600;
    let mm   = (rem % 3600) / 60;
    let ss   = rem % 60;

    // Gregorian calendar calculation (valid for 1970-2100)
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
