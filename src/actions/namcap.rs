use std::path::Path;
use std::process::{Command, Stdio};
use super::{get_target_dir, collect_pkg_files, write_error_log};

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
        // write_error_log is defined in mod.rs (shared with shellcheck)
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
