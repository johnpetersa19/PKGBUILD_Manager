// validate.rs — PKGBUILD static-analysis actions (no compilation)
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::process::{Command, Stdio};
use anyhow::{anyhow, Result};
use gettextrs::gettext;
use super::get_target_dir;

// ── helpers ───────────────────────────────────────────────────────────────────

fn makepkg(dir: &Path, args: &[&str]) -> Result<()> {
    println!(">>> makepkg {} (in {:?})", args.join(" "), dir);
    let status = Command::new("makepkg")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| anyhow!("PKGBUILD Manager: {} 'makepkg': {}", gettext("failed to spawn"), e))?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("makepkg {} {}", args.join(" "), gettext("failed")))
    }
}

// ── public actions ────────────────────────────────────────────────────────────

/// `makepkg --printsrcinfo > /dev/null`
/// Fastest offline check: validates syntax, mandatory variables and all
/// package() / prepare() / build() function signatures without touching
/// any source file.
pub fn syntax(path: &Path) -> Result<()> {
    let dir = get_target_dir(path)?;
    println!(">>> {}", gettext("Validating PKGBUILD syntax (makepkg --printsrcinfo)…"));
    let output = Command::new("makepkg")
        .arg("--printsrcinfo")
        .current_dir(&dir)
        .output()
        .map_err(|e| anyhow!("failed to run makepkg --printsrcinfo: {}", e))?;

    // Always print stderr so the user sees makepkg warnings
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        eprintln!("{}", stderr.trim());
    }

    if output.status.success() {
        println!("✔ {}", gettext("PKGBUILD syntax OK"));
        Ok(())
    } else {
        Err(anyhow!("✖ {} — {}", gettext("PKGBUILD has syntax errors"), stderr.trim()))
    }
}

/// `makepkg --nobuild --noextract`
/// Parses all variables and function bodies, resolves architecture arrays
/// and pkgver(), without downloading or extracting anything.
pub fn parse(path: &Path) -> Result<()> {
    let dir = get_target_dir(path)?;
    println!(">>> {}", gettext("Parsing PKGBUILD variables (makepkg --nobuild --noextract)…"));
    makepkg(&dir, &["--nobuild", "--noextract"])
}

/// `makepkg --verifysource`
/// Downloads the declared sources (requires network) and verifies their
/// checksums against the sums listed in the PKGBUILD.
pub fn verify_sources(path: &Path) -> Result<()> {
    let dir = get_target_dir(path)?;
    println!(">>> {}", gettext("Verifying source checksums (makepkg --verifysource)…"));
    makepkg(&dir, &["--verifysource"])
}

/// `makepkg --syncdeps --nobuild`
/// Resolves and installs declared depends/makedepends from the repos
/// without actually building the package.
pub fn check_deps(path: &Path) -> Result<()> {
    let dir = get_target_dir(path)?;
    println!(">>> {}", gettext("Checking declared dependencies (makepkg --syncdeps --nobuild)…"));
    makepkg(&dir, &["--syncdeps", "--nobuild"])
}

/// Full offline validation suite: syntax → parse → namcap → shellcheck.
/// Does NOT download sources or install deps.
pub fn all_offline(path: &Path) -> Result<()> {
    println!("\n=== {} ===", gettext("validate-offline: PKGBUILD full offline check"));

    let mut errors: Vec<String> = Vec::new();

    if let Err(e) = syntax(path)       { errors.push(format!("[syntax]    {e}")); }
    if let Err(e) = parse(path)        { errors.push(format!("[parse]     {e}")); }
    if let Err(e) = super::namcap::run(path)     { errors.push(format!("[namcap]    {e}")); }
    if let Err(e) = super::shellcheck::run(path) { errors.push(format!("[shellcheck]{e}")); }

    if errors.is_empty() {
        println!("\n✔ {}", gettext("All offline checks passed."));
        Ok(())
    } else {
        eprintln!("\n✖ {} {}:", errors.len(), gettext("check(s) failed"));
        for e in &errors { eprintln!("  {e}"); }
        Err(anyhow!("{} {}", errors.len(), gettext("validate check(s) failed")))
    }
}
