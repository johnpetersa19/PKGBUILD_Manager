use std::path::Path;
use std::process::Command;
use super::{get_target_dir, run_command};

/// Stage PKGBUILD + .SRCINFO, commit with conventional AUR message, and push.
///
/// If `message` is None, the commit message is auto-generated from the PKGBUILD:
///   "upgpkg: pkgname ver-rel"
pub fn run(path: &Path, message: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    run_with_dir(&target_dir, message)
}

/// Same as `run` but also creates an annotated version tag and pushes it.
///
/// `tag`: e.g. "1.2.3-1"
pub fn run_with_tag(path: &Path, tag: &str) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;

    run_with_dir(&target_dir, None)?;

    // Create annotated tag
    println!("[STEP] git-tag start");
    println!(">>> git tag -a {:?} -m {:?}", tag, tag);
    if let Err(e) = run_command("git", &["tag", "-a", tag, "-m", tag], &target_dir) {
        println!("[STEP] git-tag error: {}", e);
        return Err(e);
    }
    println!("[STEP] git-tag ok");

    // Push tags
    println!("[STEP] git-push-tags start");
    println!(">>> git push --tags");
    if let Err(e) = run_command("git", &["push", "--tags"], &target_dir) {
        println!("[STEP] git-push-tags error: {}", e);
        return Err(e);
    }
    println!("[STEP] git-push-tags ok");

    Ok(())
}

// Internal: perform the full stage -> commit -> push flow given an already-resolved dir.
fn run_with_dir(target_dir: &Path, message: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    // ── Step 1: Regenerate .SRCINFO ──────────────────────────────────────────
    println!("[STEP] regen-srcinfo start");
    let srcinfo_content = match regenerate_srcinfo(target_dir) {
        Ok(c) => {
            println!("[STEP] regen-srcinfo ok");
            c
        }
        Err(e) => {
            println!("[STEP] regen-srcinfo error: {}", e);
            return Err(e);
        }
    };

    // ── Step 2: git status ───────────────────────────────────────────────────
    println!("[STEP] git-status start");
    let status_out = Command::new("git")
        .args(["status", "--short"])
        .current_dir(target_dir)
        .output();
    match status_out {
        Ok(o) => {
            let txt = String::from_utf8_lossy(&o.stdout);
            for line in txt.lines() {
                println!("  {}", line);
            }
            println!("[STEP] git-status ok");
        }
        Err(e) => {
            println!("[STEP] git-status error: {}", e);
            // Non-fatal — continue anyway
        }
    }

    // ── Step 3: git add ──────────────────────────────────────────────────────
    println!("[STEP] git-add start");
    if let Err(e) = run_command("git", &["add", "PKGBUILD", ".SRCINFO"], target_dir) {
        println!("[STEP] git-add error: {}", e);
        return Err(e);
    }
    println!("[STEP] git-add ok");

    // ── Step 4: git commit ───────────────────────────────────────────────────
    println!("[STEP] git-commit start");
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
        let combined = format!("{}{}", stdout, stderr);
        if combined.contains("nothing to commit") || combined.contains("nothing added to commit") {
            println!("{}", gettextrs::gettext("Note: nothing to commit — continuing with push."));
            println!("[STEP] git-commit ok");
        } else {
            println!("[STEP] git-commit error: {}", stderr.trim());
            return Err(format!(
                "{}: {}",
                gettextrs::gettext("git commit failed"),
                stderr.trim()
            ).into());
        }
    } else {
        println!("[STEP] git-commit ok");
    }

    // ── Step 5: git push ─────────────────────────────────────────────────────
    println!("[STEP] git-push start");
    if let Err(e) = push_to_aur(target_dir) {
        println!("[STEP] git-push error: {}", e);
        return Err(e);
    }
    println!("[STEP] git-push ok");

    Ok(())
}

/// Regenerate .SRCINFO and return its content (avoids a second makepkg call).
fn regenerate_srcinfo(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    println!("{} {:?}", gettextrs::gettext(">>> Regenerating .SRCINFO in"), dir);
    let output = Command::new("makepkg")
        .arg("--printsrcinfo")
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(
            format!("{} {}", gettextrs::gettext("makepkg --printsrcinfo failed:"), err).into(),
        );
    }

    let content = String::from_utf8_lossy(&output.stdout).into_owned();
    std::fs::write(dir.join(".SRCINFO"), output.stdout)?;
    Ok(content)
}

/// Parse pkgname/pkgver/pkgrel from already-fetched .SRCINFO text.
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

/// Detect the current git branch, then push to origin/<branch>.
/// Falls back to plain `git push` if branch detection fails.
fn push_to_aur(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Detect current branch
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "master".to_string());

    println!(">>> git push origin {}", branch);
    if let Err(e) = run_command("git", &["push", "origin", &branch], dir) {
        println!(
            "{} ({}) {}",
            gettextrs::gettext("Note: push to origin/<branch> failed"),
            e,
            gettextrs::gettext("Trying plain 'git push'...")
        );
        run_command("git", &["push"], dir)?;
    }
    Ok(())
}
