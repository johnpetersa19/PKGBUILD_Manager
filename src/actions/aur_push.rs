use std::path::Path;
use std::process::Command;
use super::{get_target_dir, run_command, read_pkgbuild_info};

/// Stage PKGBUILD + .SRCINFO, commit with conventional AUR message, and push to origin/master.
///
/// If `message` is None, the commit message is auto-generated from the PKGBUILD:
///   "upgpkg: pkgname ver-rel"
pub fn run(path: &Path, message: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;

    // Regenerate .SRCINFO before staging (best practice)
    regenerate_srcinfo(&target_dir)?;

    // Stage
    run_command("git", &["add", "PKGBUILD", ".SRCINFO"], &target_dir)?;

    // Build commit message
    let auto_msg;
    let commit_msg: &str = if let Some(m) = message {
        m
    } else {
        let (name, ver, rel) = read_pkgbuild_info(&target_dir)?;
        auto_msg = format!("upgpkg: {} {}-{}", name, ver, rel);
        &auto_msg
    };

    println!(">>> git commit -m {:?}", commit_msg);
    let commit_status = Command::new("git")
        .args(["commit", "-m", commit_msg])
        .current_dir(&target_dir)
        .status()?;

    if !commit_status.success() {
        println!("{}", gettextrs::gettext("Note: nothing to commit or commit failed — continuing with push."));
    }

    // Push to AUR (master branch is the AUR default)
    push_to_aur(&target_dir)
}

/// Same as `run` but also creates an annotated version tag and pushes it.
///
/// `tag`: e.g. "1.2.3-1"
pub fn run_with_tag(path: &Path, tag: &str) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;

    // Stage and commit first
    run(path, None)?;

    // Create annotated tag
    println!(">>> git tag -a {:?} -m {:?}", tag, tag);
    run_command("git", &["tag", "-a", tag, "-m", tag], &target_dir)?;

    // Push tag
    println!(">>> git push --tags");
    run_command("git", &["push", "--tags"], &target_dir)?;

    Ok(())
}

fn regenerate_srcinfo(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!(">>> makepkg --printsrcinfo > .SRCINFO (in {:?})", dir);
    let output = Command::new("makepkg")
        .arg("--printsrcinfo")
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{} {}", gettextrs::gettext("makepkg --printsrcinfo failed:"), err).into());
    }

    let srcinfo_path = dir.join(".SRCINFO");
    std::fs::write(srcinfo_path, &output.stdout)?;
    Ok(())
}

fn push_to_aur(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Try origin/master (standard AUR branch) first, fall back to plain push
    if let Err(e) = run_command("git", &["push", "origin", "master"], dir) {
        println!("{} ({}) {}", gettextrs::gettext("Note: push to origin/master failed"), e, gettextrs::gettext("Trying plain 'git push'..."));
        run_command("git", &["push"], dir)?;
    }
    Ok(())
}
