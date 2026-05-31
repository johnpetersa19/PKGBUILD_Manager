use std::path::{Path, PathBuf};
use std::process::Command;
use super::{get_target_dir, run_command};

/// Stage PKGBUILD + .SRCINFO, commit with conventional AUR message, and push to origin/master.
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
    // Resolve once — reused for both commit and tag steps.
    let target_dir = get_target_dir(path)?;

    run_with_dir(&target_dir, None)?;

    // Create annotated tag
    println!(">>> git tag -a {:?} -m {:?}", tag, tag);
    run_command("git", &["tag", "-a", tag, "-m", tag], &target_dir)?;

    // Push tag
    println!(">>> git push --tags");
    run_command("git", &["push", "--tags"], &target_dir)?;

    Ok(())
}

// Internal: perform the full stage → commit → push flow given an already-resolved dir.
fn run_with_dir(target_dir: &PathBuf, message: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
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
    let commit_status = Command::new("git")
        .args(["commit", "-m", commit_msg])
        .current_dir(target_dir)
        .status()?;

    if !commit_status.success() {
        println!(
            "{}",
            gettextrs::gettext("Note: nothing to commit or commit failed — continuing with push.")
        );
    }

    push_to_aur(target_dir)
}

/// Regenerate .SRCINFO and return its content (avoids a second makepkg call).
fn regenerate_srcinfo(dir: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    println!(">>> makepkg --printsrcinfo > .SRCINFO (in {:?})", dir);
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

    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("pkgname = ") {
            pkgname = val.to_string();
        } else if let Some(val) = line.strip_prefix("pkgver = ") {
            pkgver = val.to_string();
        } else if let Some(val) = line.strip_prefix("pkgrel = ") {
            pkgrel = val.to_string();
        }
    }

    (pkgname, pkgver, pkgrel)
}

fn push_to_aur(dir: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if let Err(e) = run_command("git", &["push", "origin", "master"], dir) {
        println!(
            "{} ({}) {}",
            gettextrs::gettext("Note: push to origin/master failed"),
            e,
            gettextrs::gettext("Trying plain 'git push'...")
        );
        run_command("git", &["push"], dir)?;
    }
    Ok(())
}
