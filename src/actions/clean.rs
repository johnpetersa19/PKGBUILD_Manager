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

        // Remove built package files — only those matching the current PKGBUILD pkgname.
        // FIX: filter by pkgname so we never delete packages built from other PKGBUILDs
        //      that happen to live in the same directory (e.g. workspace setups).
        let pkgname_filter = read_pkgname(&target_dir);
        for pkg in collect_pkg_files(&target_dir) {
            // If we couldn't read pkgname, fall back to removing all pkg.tar.* files
            // (same behaviour as before the fix, safe for single-PKGBUILD directories).
            let matches = pkgname_filter
                .as_deref()
                .map_or(true, |n| pkg.starts_with(n));
            if matches {
                let p = target_dir.join(&pkg);
                std::fs::remove_file(&p)?;
                println!("  {} {:?}", gettextrs::gettext("Removed"), p);
            }
        }

        // Single directory traversal: removes bare git repo cache dirs and _build* dirs.
        // FIX: a normal git clone also has HEAD + objects/ inside .git/, so we must
        // explicitly skip .git/ to avoid destroying the package repository itself.
        if let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let Ok(ft) = entry.file_type() else { continue };
                let p    = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();

                if ft.is_dir() {
                    // FIX: never remove the project's own .git/ directory
                    if name == ".git" {
                        continue;
                    }

                    if p.join("HEAD").exists() && p.join("objects").exists() {
                        std::fs::remove_dir_all(&p)?;
                        println!(
                            "  {} {:?}",
                            gettextrs::gettext("Removed bare-repo cache"),
                            p
                        );
                    } else if name.starts_with("_build") {
                        std::fs::remove_dir_all(&p)?;
                        println!(
                            "  {} {:?}",
                            gettextrs::gettext("Removed build directory"),
                            p
                        );
                    }
                }
            }
        }
    } else {
        run_command("makepkg", &["-c"], &target_dir)?;
    }

    Ok(())
}

/// Read `pkgname` from the PKGBUILD in `dir` using a simple line scan.
/// Returns None if the file cannot be read or no `pkgname=` line is found.
fn read_pkgname(dir: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("PKGBUILD")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("pkgname=") {
            // Strip surrounding quotes if present
            let val = val.trim_matches(|c| c == '\'' || c == '"');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}
