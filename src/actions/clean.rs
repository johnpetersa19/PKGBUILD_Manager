use std::path::Path;
use super::{get_target_dir, run_command, collect_pkg_files};

/// Clean the srcdir using `makepkg -c` (soft clean, preserves pkg/).
/// Use `full = true` for a complete wipe: removes src/, pkg/, built packages
/// and the bare-repo cache directory created by makepkg for git sources.
pub fn run(path: &Path, full: bool) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;

    if full {
        println!("{} {:?}", gettextrs::gettext("Removing src/ pkg/ and built packages in"), target_dir);

        // Remove src/ and pkg/ directories
        for dir in &["src", "pkg"] {
            let to_remove = target_dir.join(dir);
            if to_remove.exists() {
                std::fs::remove_dir_all(&to_remove)?;
                println!("  {} {:?}", gettextrs::gettext("Removed"), to_remove);
            }
        }

        // Single directory traversal: removes *.pkg.tar.* files and bare git repo
        // cache dirs (dirs containing both HEAD and objects/) in one pass.
        if let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                if p.is_file() && name.contains(".pkg.tar.") {
                    std::fs::remove_file(&p)?;
                    println!("  {} {:?}", gettextrs::gettext("Removed"), p);
                } else if p.is_dir() && p.join("HEAD").exists() && p.join("objects").exists() {
                    std::fs::remove_dir_all(&p)?;
                    println!("  {} {:?}", gettextrs::gettext("Removed bare repo cache"), p);
                }
            }
        }

        Ok(())
    } else {
        // Soft clean via makepkg -c (removes srcdir only)
        run_command("makepkg", &["-c"], &target_dir)
    }
}
