use std::path::Path;
use super::{get_target_dir, run_command, collect_pkg_files};

/// Clean the srcdir using `makepkg -c` (soft clean, preserves pkg/).
/// Use `full = true` for a complete wipe: removes .makepkg.lock, src/, pkg/, built packages,
/// bare-repo cache dirs and any _build* directories (cmake/meson/autotools out-of-tree builds).
pub fn run(path: &Path, full: bool) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;

    if full {
        // Remove .makepkg.lock first so a failed/interrupted build never blocks clean-all.
        let lock = target_dir.join(".makepkg.lock");
        if lock.exists() {
            std::fs::remove_file(&lock)?;
        }

        println!(
            "{} {:?}",
            gettextrs::gettext("Removing src/ pkg/ and compiled packages in"),
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

                    // FIX: use a robust helper instead of the fragile HEAD+objects heuristic
                    if is_bare_git_repo(&p) {
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

/// Returns true only when `dir` looks like a genuine git bare repository.
///
/// A bare repo created by git always contains **all four** of:
///   - `HEAD`   — file whose content starts with "ref: refs/" or is a 40-char hex SHA
///   - `objects/` — directory
///   - `refs/`    — directory
///   - `config`   — file
///
/// Requiring all four makes false positives from build-cache directories
/// (which might incidentally have a `HEAD` file or an `objects/` folder)
/// practically impossible.
fn is_bare_git_repo(dir: &std::path::Path) -> bool {
    // Structural presence check
    if !dir.join("objects").is_dir()
        || !dir.join("refs").is_dir()
        || !dir.join("config").is_file()
    {
        return false;
    }

    // Content check on HEAD: must look like a git HEAD
    let head_path = dir.join("HEAD");
    match std::fs::read_to_string(&head_path) {
        Ok(content) => {
            let content = content.trim();
            // Symbolic ref: "ref: refs/heads/main"
            if content.starts_with("ref: refs/") {
                return true;
            }
            // Detached HEAD: exactly 40 lowercase hex chars
            if content.len() == 40 && content.chars().all(|c| c.is_ascii_hexdigit()) {
                return true;
            }
            false
        }
        Err(_) => false,
    }
}

/// Read `pkgname` from the PKGBUILD in `dir` using a simple line scan.
/// Supports both the scalar form (`pkgname=value`) and the array form
/// (`pkgname=('pkg1' 'pkg2')` or `pkgname=(meu-pacote)`).
/// Returns the first package name found, or None if the file cannot be
/// read or no `pkgname=` line is present.
fn read_pkgname(dir: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("PKGBUILD")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("pkgname=") {
            let val = val.trim();

            // Array form: pkgname=('pkg1' 'pkg2')  or  pkgname=(meu-pacote)
            if let Some(inner) = val.strip_prefix('(') {
                let inner = inner.trim_end_matches(')').trim();
                // Split on whitespace and take the first non-empty token
                if let Some(first) = inner.split_whitespace().next() {
                    let name = first.trim_matches(|c| c == '\'' || c == '"');
                    if !name.is_empty() {
                        return Some(name.to_string());
                    }
                }
            } else {
                // Scalar form: pkgname=value  or  pkgname="value"
                let name = val.trim_matches(|c| c == '\'' || c == '"');
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}
