use std::path::Path;
use std::process::Command;
use super::{get_target_dir, collect_pkg_files};

pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;

    let mut args = vec!["PKGBUILD".to_string()];
    // Reuse shared helper — avoids duplicating read_dir logic from clean.rs
    args.extend(collect_pkg_files(&target_dir));

    let args_slices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    println!(">>> namcap {} (in {:?})", args_slices.join(" "), target_dir);

    let status = Command::new("namcap")
        .args(&args_slices)
        .current_dir(&target_dir)
        .status()
        .map_err(|e| -> Box<dyn std::error::Error> {
            if e.kind() == std::io::ErrorKind::NotFound {
                gettextrs::gettext("namcap not found. Install it with: sudo pacman -S namcap").into()
            } else {
                e.into()
            }
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{} {}", gettextrs::gettext("namcap failed with status"), status).into())
    }
}
