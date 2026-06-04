use std::path::Path;
use std::process::Command;
use super::{get_target_dir, run_command, regenerate_srcinfo};

/// Stage PKGBUILD + .SRCINFO, commit with conventional AUR message, and push.
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
            ));
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
        let ref_str = stdout.trim();
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
