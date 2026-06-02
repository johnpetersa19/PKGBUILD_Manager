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

    let status = Command::new("namcap")
        .args(&args_slices)
        .current_dir(&target_dir)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "{} {}",
            gettextrs::gettext("namcap failed with status"),
            status
        ))
    }
}
