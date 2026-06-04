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
    println!(">>> git tag -a {:?} -m {:?}", tag, tag);
    run_command("git", &["tag", "-a", tag, "-m", tag], &target_dir)?;

    // Step 3: push the tag to the remote.
    //
    // FIX: `git push --tags` failing after the commit was already pushed leaves
    // the repository in an inconsistent state (tag local-only, commit on AUR).
    // A true rollback is impossible once the commit is on the remote, so instead
    // we catch the error explicitly and emit a clearly-worded, actionable message
    // that tells the user exactly what happened and what command to run to recover.
    println!(">>> git push origin tag {}", tag);
    if let Err(push_err) = run_command("git", &["push", "origin", "tag", tag], &target_dir) {
        // Build a human-readable recovery hint using the exact tag name
        let hint = format!("git push origin {}", tag);
        return Err(anyhow::anyhow!(
            "{}\n\
             {}\n\
             {}\n  {}",
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
        ).context(push_err));
    }

    Ok(())
}

// Internal: perform the full stage -> commit -> push flow given an already-resolved dir.
fn run_with_dir(target_dir: &Path, message: Option<&str>) -> anyhow::Result<()> {
    use anyhow::Context as _;
    let _ = anyhow::Context::context as fn(_,_) -> _; // ensure trait in scope
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
            println!("{}", gettextrs::gettext("Note: nothing to commit or commit failed \u{2014} continuing with push."));
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

/// Bug #11 fix + Opt #6: detecta o branch padrão do remote sem nenhuma
/// chamada de rede. Usa apenas referências locais do git, que são
/// instantâneas e nunca bloqueiam a thread chamadora.
///
/// Estratégia (sem rede, em ordem de preferência):
///   1. git symbolic-ref refs/remotes/origin/HEAD
///      → resolve o tracking branch que o `git clone`/`git fetch` define.
///      Exemplo de saída: "refs/remotes/origin/main" → extrai "main".
///   2. Verifica se refs/remotes/origin/main existe localmente.
///   3. Verifica se refs/remotes/origin/master existe localmente.
///   4. Retorna None → caller usa "master" como fallback.
fn detect_remote_default_branch(dir: &Path) -> Option<String> {
    // 1. Tenta symbolic-ref (disponivel após clone ou fetch --all)
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output: "refs/remotes/origin/main\n"
        let ref_str = stdout.trim();
        // Extract the branch name after the last '/'
        if let Some(branch) = ref_str.rsplit('/').next() {
            let b = branch.trim().to_string();
            if !b.is_empty() && b != "HEAD" {
                return Some(b);
            }
        }
    }

    // 2. Verifica se origin/main existe como ref local
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

    // 3. Sem informação local — caller faz fallback para "master"
    None
}
