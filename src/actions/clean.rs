use std::path::Path;
use super::{get_target_dir, run_command};

/// Clean the srcdir using `makepkg -c` (soft clean, preserves pkg/).
/// Use `full = true` for a complete wipe: removes src/, pkg/, built packages,
/// bare-repo cache dirs and any _build* directories (cmake/meson/autotools out-of-tree builds).
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

        // Single directory traversal: removes *.pkg.tar.* files, bare git repo
        // cache dirs (dirs containing both HEAD and objects/) and _build* dirs
        // (out-of-tree build directories created by cmake/meson/autotools) in one pass.
        if let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let name = p.file_name().unwrap_or_default().to_string_lossy();

                if p.is_file() && name.contains(".pkg.tar.") {
                    std::fs::remove_file(&p)?;
                    println!("  {} {:?}", gettextrs::gettext("Removed"), p);
                } else if p.is_dir() {
                    // Bare git repo cache (makepkg git sources)
                    if p.join("HEAD").exists() && p.join("objects").exists() {
                        std::fs::remove_dir_all(&p)?;
                        println!("  {} {:?}", gettextrs::gettext("Removed bare repo cache"), p);
                    // _build* directories (cmake / meson / autotools out-of-tree builds)
                    } else if name.starts_with("_build") {
                        std::fs::remove_dir_all(&p)?;
                        println!("  {} {:?}", gettextrs::gettext("Removed build dir"), p);
                    }
                }
            }
        }

        Ok(())
    } else {
        // Soft clean via makepkg -c (removes srcdir only)
        run_command("makepkg", &["-c"], &target_dir)
    }
}
