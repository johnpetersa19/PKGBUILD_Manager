use std::path::Path;
use std::process::Command;
use super::get_target_dir;

pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    
    // We will run namcap on PKGBUILD
    let mut args = vec!["PKGBUILD".to_string()];
    
    // Check for any compiled packages (*.pkg.tar.zst) to inspect them as well
    if let Ok(entries) = std::fs::read_dir(&target_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "zst" {
                        if let Some(filename) = path.file_name() {
                            let filename_str = filename.to_string_lossy().into_owned();
                            if filename_str.contains(".pkg.tar.") {
                                args.push(filename_str);
                            }
                        }
                    }
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
