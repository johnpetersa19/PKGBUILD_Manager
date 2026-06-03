use std::path::Path;
use std::process::Command;
use super::{get_target_dir, run_command, regenerate_srcinfo};

/// Stage PKGBUILD + .SRCINFO, commit with conventional AUR message, and push to origin.
///
/// If `message` is None, the commit message is auto-generated from the PKGBUILD:
///   "upgpkg: pkgname ver-rel"
pub fn run(path: &Path, message: Option<&str>) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;
    run_with_dir(&target_dir, message)
}

/// Same as `run` but also creates an annotated version tag and pushes it.
///
/// `tag`: e.g. "1.2.3-1"
pub fn run_with_tag(path: &Path, tag: &str) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;

    // FIX: validate tag format — must be non-empty and contain no whitespace
    let tag = tag.trim();
    if tag.is_empty() {
        return Err(anyhow::anyhow!(
            "{}",
            gettextrs::gettext("Tag cannot be empty")
        ));
    }
    if tag.contains(char::is_whitespace) {
        return Err(anyhow::anyhow!(
            "{}: {:?}",
            gettextrs::gettext("Tag must not contain whitespace"),
            tag
        ));
    }

    run_with_dir(&target_dir, None)?;

    // Create annotated tag
    println!(">>> git tag -a {:?} -m {:?}", tag, tag);
    run_command("git", &["tag", "-a", tag, "-m", tag], &target_dir)?;

    // Push tag
    println!(">>> git push --tags");
    run_command("git", &["push", "--tags"], &target_dir)?;

    Ok(())
}

// Internal: perform the full stage -> commit -> push flow given an already-resolved dir.
fn run_with_dir(target_dir: &Path, message: Option<&str>) -> anyhow::Result<()> {
    // Regenerate .SRCINFO and parse package info from the same output in one pass.
    let srcinfo_content = regenerate_srcinfo(target_dir)?;

    // Stage
    run_command("git", &["add", "PKGBUILD", ".SRCINFO"], target_dir)?;

    // Build commit message
    let auto_msg;
    let commit_msg: &str = if let Some(m) = message {
        m
    } else {
        let (name, ver, rel) = parse_pkgbuild_info(&srcinfo_content);
        auto_msg = format!("upgpkg: {} {}-{}", name, ver, rel);
        &auto_msg
    };

    println!(">>> git commit -m {:?}", commit_msg);
    let output = Command::new("git")
        .args(["commit", "-m", commit_msg])
        .current_dir(target_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // git exits 1 with "nothing to commit" -- that is not a real error,
        // so we detect it explicitly and continue. Any other failure is propagated.
        let combined = format!("{}{}", stdout, stderr);
        if combined.contains("nothing to commit") || combined.contains("nothing added to commit") {
            println!("{}", gettextrs::gettext("Note: nothing to commit — continuing with push."));
        } else {
            return Err(anyhow::anyhow!(
                "{}: {}",
                gettextrs::gettext("git commit failed"),
                stderr.trim()
            ));
        }
    }

    push_to_aur(target_dir)
}

/// Parse pkgname/pkgver/pkgrel from already-fetched .SRCINFO text.
/// Stops iterating as soon as all three fields are found (early-exit).
fn parse_pkgbuild_info(content: &str) -> (String, String, String) {
    let mut pkgname = String::from("unknown");
    let mut pkgver  = String::from("0");
    let mut pkgrel  = String::from("1");
    let mut found_name = false;
    let mut found_ver  = false;
    let mut found_rel  = false;

    for line in content.lines() {
        if found_name && found_ver && found_rel {
            break;
        }
        let line = line.trim();
        if !found_name {
            if let Some(val) = line.strip_prefix("pkgname = ") {
                pkgname = val.to_string();
                found_name = true;
                continue;
            }
        }
        if !found_ver {
            if let Some(val) = line.strip_prefix("pkgver = ") {
                pkgver = val.to_string();
                found_ver = true;
                continue;
            }
        }
        if !found_rel {
            if let Some(val) = line.strip_prefix("pkgrel = ") {
                pkgrel = val.to_string();
                found_rel = true;
                continue;
            }
        }
    }

    (pkgname, pkgver, pkgrel)
}

/// Pushes to AUR, detecting the default remote branch.
fn push_to_aur(dir: &Path) -> anyhow::Result<()> {
    let default_branch = detect_remote_default_branch(dir);
    let branch = default_branch.as_deref().unwrap_or("master");

    println!(">>> git push origin {}", branch);
    if let Err(e) = run_command("git", &["push", "origin", branch], dir) {
        println!(
            "{} ({}) {}",
            gettextrs::gettext("Note: push to origin failed"),
            e,
            gettextrs::gettext("Trying plain 'git push'...")
        );
        run_command("git", &["push"], dir)?;
    }
    Ok(())
}

/// Detects the default branch name on origin (main, master, or custom).
/// Returns None if detection fails — caller should fall back to "master".
///
/// OPT: First checks the local symbolic ref refs/remotes/origin/HEAD (no network).
/// Only falls back to `git remote show origin` (network) if the local ref is absent.
fn detect_remote_default_branch(dir: &Path) -> Option<String> {
    // Fast path: read local tracking ref — no network required
    let local = Command::new("git")
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;

    if local.status.success() {
        let s = String::from_utf8_lossy(&local.stdout);
        // Output is "origin/<branch>" — strip the "origin/" prefix
        if let Some(branch) = s.trim().strip_prefix("origin/") {
            let b = branch.trim().to_string();
            if !b.is_empty() {
                return Some(b);
            }
        }
    }

    // Slow path: network call via `git remote show origin`
    let output = Command::new("git")
        .args(["remote", "show", "origin"])
        .current_dir(dir)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(branch) = line.trim().strip_prefix("HEAD branch: ") {
            let b = branch.trim().to_string();
            if !b.is_empty() && b != "(unknown)" {
                return Some(b);
            }
        }
    }

    None
}
