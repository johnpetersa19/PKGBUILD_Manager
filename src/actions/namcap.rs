use std::path::Path;
use std::process::Command;
use super::{get_target_dir, collect_pkg_files};

pub fn run(path: &Path) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;

    let mut args = vec!["PKGBUILD".to_string()];
    // Reuse shared helper — avoids duplicating read_dir logic from clean.rs
    args.extend(collect_pkg_files(&target_dir));

    let args_slices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    println!(">>> namcap {} (in {:?})", args_slices.join(" "), target_dir);

    // FIX: namcap always exits with code 0, even when it finds errors.
    // Capture output and scan for lines prefixed with "E:" (namcap error marker).
    // "W:" lines are warnings — printed but do not cause failure.
    let output = Command::new("namcap")
        .args(&args_slices)
        .current_dir(&target_dir)
        .output()?;

    // Print all output so the user sees every warning and error, just like before
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    print!("{}", combined);

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

    // Also propagate a non-zero exit if namcap ever starts using it
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "{} {}",
            gettextrs::gettext("namcap failed with status"),
            output.status
        ));
    }

    Ok(())
}
