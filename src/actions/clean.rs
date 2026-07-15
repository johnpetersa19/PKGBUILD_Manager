use std::path::Path;
use std::process::Stdio;
use super::{get_target_dir, run_command, collect_pkg_files};

/// Remove a directory tree, handling read-only files/dirs inside it.
///
/// `std::fs::remove_dir_all` fails with EACCES on Linux when the tree
/// contains files or directories with restrictive permissions (e.g. git
/// object files at chmod 444, or src/ subdirs at 555).  This helper
/// retries after running `chmod -R u+rwX` so the caller gets the same
/// behaviour as `rm -rf`.
fn remove_dir_force(path: &Path) -> anyhow::Result<()> {
    // Fast path — most trees are writable, avoid spawning chmod.
    if std::fs::remove_dir_all(path).is_ok() {
        return Ok(());
    }
    // Slow path — unlock permissions then retry.
    let _ = crate::host::command("chmod")
        .args(["-R", "u+rwX"])
        .arg(path)
        .status();
    std::fs::remove_dir_all(path).map_err(|e| {
        anyhow::anyhow!(
            "remove_dir_force: could not remove {:?}: {}",
            path, e
        )
    })
}

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

        // Remove src/ and pkg/ — use remove_dir_force so git object files
        // with chmod 444/555 (common in AUR source repos) never block the wipe.
        for dir in &["src", "pkg"] {
            let to_remove = target_dir.join(dir);
            if to_remove.exists() {
                remove_dir_force(&to_remove)?;
                println!("  {} {:?}", gettextrs::gettext("Removed"), to_remove);
            }
        }

        // Remove built package files — only those matching the current PKGBUILD pkgname.
        // FIX: filter by pkgname so we never delete packages built from other PKGBUILDs
        //      that happen to live in the same directory (e.g. workspace setups).
        let pkgname_filter = read_pkgname(&target_dir);
        for pkg in collect_pkg_files(&target_dir) {
            let matches = match pkgname_filter.as_deref() {
                Some(n) => pkg.starts_with(n),
                None => {
                    // Could not determine pkgname (bash expansion that makepkg also
                    // failed to resolve, or PKGBUILD not readable). Fall back to
                    // removing all .pkg.tar.* files and warn the user explicitly.
                    eprintln!(
                        "  {} {}",
                        gettextrs::gettext(
                            "Warning: could not determine pkgname — removing all .pkg.tar.* files:"
                        ),
                        pkg
                    );
                    true
                }
            };
            if matches {
                let p = target_dir.join(&pkg);
                std::fs::remove_file(&p)?;
                println!("  {} {:?}", gettextrs::gettext("Removed"), p);
            }
        }

        // Single directory traversal: removes bare git repo cache dirs and _build* dirs.
        // GUARD: skip .git/ explicitly — a normal git clone has HEAD + objects/ inside
        // .git/ and would be misidentified as a bare repo without this check.
        if let Ok(entries) = std::fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let Ok(ft) = entry.file_type() else { continue };
                let p    = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();

                if ft.is_dir() {
                    // Never remove the project's own .git/ directory.
                    if name == ".git" {
                        continue;
                    }

                    // Robust bare-repo detection: requires all 4 canonical markers.
                    if is_bare_git_repo(&p) {
                        remove_dir_force(&p)?;
                        println!(
                            "  {} {:?}",
                            gettextrs::gettext("Removed bare-repo cache"),
                            p
                        );
                    } else if name.starts_with("_build") {
                        remove_dir_force(&p)?;
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

/// Read `pkgname` from the PKGBUILD in `dir`.
///
/// # Strategy
///
/// 1. **Static scan** — fast path for simple, literal values:
///    Reads `pkgname=` from the PKGBUILD text and strips quotes.
///    Handles scalar (`pkgname=foo`) and array (`pkgname=(foo bar)`) forms.
///
/// 2. **makepkg fallback** — for bash expansions:
///    If the static value contains `$`, `` ` ``, or `{` it is an unexpanded
///    bash expression (e.g. `pkgname="${_pkgname}-git"`).  In that case the
///    literal string would never match real package filenames, so we run
///    `makepkg --printsrcinfo` (offline, fast) and extract `pkgname =` from
///    its structured output, which is always fully expanded.
///
/// 3. **None** is returned only when both methods fail.  The caller then
///    falls back to removing all `.pkg.tar.*` files with a visible warning.
fn read_pkgname(dir: &std::path::Path) -> Option<String> {
    // ── Step 1: static scan ──────────────────────────────────────────────────
    if let Some(name) = static_read_pkgname(dir) {
        // If the name looks like a literal (no bash special chars), use it.
        if !looks_like_bash_expansion(&name) {
            return Some(name);
        }
        // Otherwise fall through to the makepkg expansion step.
    }

    // ── Step 2: makepkg --printsrcinfo fallback ────────────────────────────
    // Run makepkg --printsrcinfo to get the fully-expanded .SRCINFO output.
    // This is the same invocation used by `validate-syntax` — fast and offline.
    let output = crate::host::command("makepkg")
        .arg("--printsrcinfo")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    // Parse `pkgname = <value>` from the .SRCINFO output.
    // The format is always `key = value` (single space around `=`).
    let srcinfo = String::from_utf8_lossy(&output.stdout);
    for line in srcinfo.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("pkgname = ") {
            let val = val.trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }

    None
}

/// Fast static scan of the PKGBUILD text for a literal `pkgname=` value.
/// Returns None if the file cannot be read or no `pkgname=` line is found.
fn static_read_pkgname(dir: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("PKGBUILD")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("pkgname=") {
            let val = val.trim();

            // Array form: pkgname=('pkg1' 'pkg2')  or  pkgname=(meu-pacote)
            if let Some(inner) = val.strip_prefix('(') {
                let inner = inner.trim_end_matches(')').trim();
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

/// Returns true if `s` contains bash variable/command expansion characters
/// that would prevent a static match against real package filenames.
#[inline]
fn looks_like_bash_expansion(s: &str) -> bool {
    s.contains('$') || s.contains('`') || s.contains('{')
}
