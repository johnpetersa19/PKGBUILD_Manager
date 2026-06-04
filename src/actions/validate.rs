// validate.rs — PKGBUILD static-analysis actions (no compilation)
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::process::{Command, Stdio};
use anyhow::{anyhow, Result};
use gettextrs::gettext;
use super::get_target_dir;

// ── helpers ────────────────────────────────────────────────────────────────────────────────

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

// ── public actions ───────────────────────────────────────────────────────────────────

/// `makepkg --printsrcinfo`
/// Fastest offline check: validates syntax, mandatory variables and all
/// package() / prepare() / build() function signatures without touching
/// any source file. This is the only makepkg invocation that is
/// guaranteed to be truly offline for ALL PKGBUILD types, including
/// VCS packages with a dynamic pkgver() function.
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
        println!("\u2714 {}", gettext("PKGBUILD syntax OK"));
        Ok(())
    } else {
        Err(anyhow!("\u2716 {} — {}", gettext("PKGBUILD has syntax errors"), stderr.trim()))
    }
}

/// `makepkg --nobuild --noextract`
/// Parses all variables and function bodies, resolves architecture arrays
/// and pkgver() without downloading or extracting anything.
///
/// **Caveat:** PKGBUILDs with a dynamic `pkgver()` function may trigger
/// network access even with `--noextract`, because makepkg can invoke
/// pkgver() to resolve the version during this step. Use `validate-parse`
/// only when you accept that possibility. For a guaranteed offline check,
/// use `validate-syntax` (or `validate`, which uses --printsrcinfo only).
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

/// Full offline validation suite: syntax \u2192 namcap \u2192 shellcheck.
/// Does NOT download sources, install deps, or invoke pkgver().
///
/// FIX: the previous implementation called parse() here, which runs
/// `makepkg --nobuild --noextract` and can trigger network access on VCS
/// packages with a dynamic pkgver() function — contrary to the
/// "all_offline" contract. The parse step is removed from this suite;
/// users who want it can run `validate-parse` explicitly.
pub fn all_offline(path: &Path) -> Result<()> {
    println!("\n=== {} ===", gettext("validate-offline: PKGBUILD full offline check"));

    let mut errors: Vec<String> = Vec::new();

    // syntax() uses --printsrcinfo: guaranteed offline for all PKGBUILD types.
    // parse() is intentionally omitted here — see doc-comment above.
    if let Err(e) = syntax(path)                  { errors.push(format!("[syntax]    {e}")); }
    if let Err(e) = super::namcap::run(path)       { errors.push(format!("[namcap]    {e}")); }
    if let Err(e) = super::shellcheck::run(path)   { errors.push(format!("[shellcheck]{e}")); }

    if errors.is_empty() {
        println!("\n\u2714 {}", gettext("All offline checks passed."));
        Ok(())
    } else {
        eprintln!("\n\u2716 {} {}:", errors.len(), gettext("check(s) failed"));
        for e in &errors { eprintln!("  {e}"); }
        Err(anyhow!("{} {}", errors.len(), gettext("validate check(s) failed")))
    }
}
