use std::path::Path;
use std::process::Command;
use super::get_target_dir;

pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;

    let mut args = vec!["PKGBUILD".to_string()];

    // Collect all *.pkg.tar.* files (zst, xz, gz, bz2, etc.)
    if let Ok(entries) = std::fs::read_dir(&target_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                if name.contains(".pkg.tar.") {
                    args.push(name.into_owned());
                }
            }
        }
    }

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
