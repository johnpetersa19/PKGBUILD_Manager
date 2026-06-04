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
/// `tag`: e.g. "1.2.3-1" — validated in main.rs before reaching here.
pub fn run_with_tag(path: &Path, tag: &str) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;

    // Step 1: commit + push to AUR (normal flow)
    run_with_dir(&target_dir, None)?;

    // Step 2: create the annotated tag locally
    println!("[STEP] git-tag start");
    println!(">>> git tag -a {:?} -m {:?}", tag, tag);
    run_command("git", &["tag", "-a", tag, "-m", tag], &target_dir)?;
    println!("[STEP] git-tag ok");

    // Step 3: push the tag to the remote.
    //
    // FIX: `git push --tags` failing after the commit was already pushed leaves
    // the repository in an inconsistent state (tag local-only, commit on AUR).
    // A true rollback is impossible once the commit is on the remote, so instead
    // we catch the error explicitly and emit a clearly-worded, actionable message
    // that tells the user exactly what happened and what command to run to recover.
    println!("[STEP] git-push-tag start");
    println!(">>> git push origin tag {}", tag);
    if let Err(push_err) = run_command("git", &["push", "origin", "tag", tag], &target_dir) {
        println!("[STEP] git-push-tag error: {}", push_err);
        let hint = format!("git push origin {}", tag);
        return Err(anyhow::anyhow!(
            "{}\n{}\n{}\n  {}\n({}: {})",
            gettextrs::gettext(
                "Warning: the commit was pushed to the AUR but the tag could not be pushed."
            ),
            gettextrs::gettext(
                "The tag exists locally only. This is an inconsistent state."
            ),
            gettextrs::gettext(
                "Once the network issue is resolved, push the tag manually with:"
            ),
            hint,
            gettextrs::gettext("original error"),
            push_err,
        ));
    }
    println!("[STEP] git-push-tag ok");

    Ok(())
}

// Internal: perform the full stage -> commit -> push flow given an already-resolved dir.
fn run_with_dir(target_dir: &Path, message: Option<&str>) -> anyhow::Result<()> {
    // Step 1: Regenerate .SRCINFO and parse package info from the same output in one pass.
    println!("[STEP] regen-srcinfo start");
    let srcinfo_content = regenerate_srcinfo(target_dir)?;
    println!("[STEP] regen-srcinfo ok");

    // Step 2: git status (informational, non-fatal)
    println!("[STEP] git-status start");
    if let Ok(o) = Command::new("git")
        .args(["status", "--short"])
        .current_dir(target_dir)
        .output()
    {
        let txt = String::from_utf8_lossy(&o.stdout);
        for line in txt.lines() {
            println!("  {}", line);
        }
    }
    println!("[STEP] git-status ok");

    // Step 3: git add
    println!("[STEP] git-add start");
    run_command("git", &["add", "PKGBUILD", ".SRCINFO"], target_dir)?;
    println!("[STEP] git-add ok");

    // Step 4: Build commit message and commit
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
            println!("{}", gettextrs::gettext("Note: nothing to commit or commit failed \u{2014} continuing with push."));
            println!("[STEP] git-commit ok");
        } else {
            println!("[STEP] git-commit error: {}", stderr.trim());
            return Err(anyhow::anyhow!(
                "{}: {}",
                gettextrs::gettext("git commit failed"),
                stderr.trim()
            ));
        }
    } else {
        println!("[STEP] git-commit ok");
    }

    // Step 5: push
    println!("[STEP] git-push start");
    push_to_aur(target_dir)?;
    println!("[STEP] git-push ok");

    Ok(())
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

/// Push to the AUR remote, auto-detecting the default branch.
fn push_to_aur(dir: &Path) -> anyhow::Result<()> {
    let default_branch = detect_remote_default_branch(dir);
    let branch = default_branch.as_deref().unwrap_or("master");

    println!(">>> git push origin {}", branch);
    if let Err(e) = run_command("git", &["push", "origin", branch], dir) {
        println!(
            "{} ({}) {}",
            gettextrs::gettext("Note: push to origin/master failed"),
            e,
            gettextrs::gettext("Attempting simple 'git push'...")
        );
        run_command("git", &["push"], dir)?;
    }
    Ok(())
}

/// Detect the remote default branch without any network call.
/// Uses only local git refs, which are instantaneous and never block the caller.
///
/// Strategy (no network, in order of preference):
///   1. git symbolic-ref refs/remotes/origin/HEAD
///      → resolves the tracking branch set by `git clone`/`git fetch`.
///      Example output: "refs/remotes/origin/main" → extracts "main".
///   2. Check if refs/remotes/origin/main exists locally.
///   3. Check if refs/remotes/origin/master exists locally.
///   4. Return None → caller falls back to "master".
fn detect_remote_default_branch(dir: &Path) -> Option<String> {
    // 1. Try symbolic-ref (available after clone or fetch --all)
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let ref_str = stdout.trim();
        if let Some(branch) = ref_str.rsplit('/').next() {
            let b = branch.trim().to_string();
            if !b.is_empty() && b != "HEAD" {
                return Some(b);
            }
        }
    }

    // 2. Check if origin/main exists as a local ref
    let check_ref = |refname: &str| -> bool {
        Command::new("git")
            .args(["rev-parse", "--verify", refname])
            .current_dir(dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };

    if check_ref("refs/remotes/origin/main") {
        return Some("main".to_string());
    }

    if check_ref("refs/remotes/origin/master") {
        return Some("master".to_string());
    }

    // 3. No local information — caller falls back to "master"
    None
}
