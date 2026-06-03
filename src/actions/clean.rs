use std::path::Path;
use super::{get_target_dir, run_command, collect_pkg_files};

/// Clean the srcdir using `makepkg -c` (soft clean, preserves pkg/).
/// Use `full = true` for a complete wipe: removes src/, pkg/, built packages,
/// bare-repo cache dirs and any _build* directories (cmake/meson/autotools out-of-tree builds).
pub fn run(path: &Path, full: bool) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;

    if full {
        println!(
            "{} {:?}",
            gettextrs::gettext("Removing src/ pkg/ and built packages in"),
            target_dir
        );

        // Remove src/ and pkg/ directories
        for dir in &["src", "pkg"] {
            let to_remove = target_dir.join(dir);
            if to_remove.exists() {
                std::fs::remove_dir_all(&to_remove)?;
                println!("  {} {:?}", gettextrs::gettext("Removed"), to_remove);
            }
        }

        // Remove built package files using shared helper
        for pkg in collect_pkg_files(&target_dir) {
            let p = target_dir.join(&pkg);
            std::fs::remove_file(&p)?;
            println!("  {} {:?}", gettextrs::gettext("Removed"), p);
        }

        // Single directory traversal: removes bare git repo cache dirs and _build* dirs.
        // FIX: o .git/ de um clone normal também possui HEAD + objects/, então
        // precisamos excluí-lo explicitamente para não destruir o repositório do pacote.
        if let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let Ok(ft) = entry.file_type() else { continue };
                let p    = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();

                if ft.is_dir() {
                    // FIX: nunca remover o .git/ do próprio projeto
                    if name == ".git" {
                        continue;
                    }

                    if p.join("HEAD").exists() && p.join("objects").exists() {
                        std::fs::remove_dir_all(&p)?;
                        println!(
                            "  {} {:?}",
                            gettextrs::gettext("Removed bare repo cache"),
                            p
                        );
                    } else if name.starts_with("_build") {
                        std::fs::remove_dir_all(&p)?;
                        println!(
                            "  {} {:?}",
                            gettextrs::gettext("Removed build dir"),
                            p
                        );
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
