/* main.rs
 *
 * Copyright 2026 johnpetersa19
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

mod config;
mod actions;

use anyhow::Result;
use config::{GETTEXT_PACKAGE, LOCALEDIR, VERSION};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain, gettext, LocaleCategory};
use std::env;
use std::path::Path;

fn main() -> Result<()> {
    // setlocale MUST be called before bindtextdomain/textdomain so that
    // the C library initialises the locale from the environment (LANG,
    // LC_ALL, ...). Without this call gettextrs silently falls back to the
    // "C" locale and never loads any .mo file.
    gettextrs::setlocale(LocaleCategory::LcAll, "");

    let locale_dir = std::env::var("PKGBUILD_MANAGER_LOCALEDIR")
        .unwrap_or_else(|_| LOCALEDIR.to_string());
    let _ = bindtextdomain(GETTEXT_PACKAGE, &locale_dir);
    let _ = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    let _ = textdomain(GETTEXT_PACKAGE);

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let command = &args[1];

    if command == "--version" {
        println!("pkgbuild_manager {}", VERSION);
        return Ok(());
    }

    // Supported forms:
    //   pkgbuild_manager build                # path = ".", flags = []
    //   pkgbuild_manager build /dir           # path = "/dir", flags = []
    //   pkgbuild_manager build -- -c -f       # path = ".",   flags = ["-c", "-f"]
    //   pkgbuild_manager build /dir -- -c -f  # path = "/dir", flags = ["-c", "-f"]
    //   pkgbuild_manager build -c -f          # path = ".",   flags = ["-c", "-f"]
    let (path_arg, extra_flags): (&str, Vec<&str>) = {
        let mut path: &str = ".";
        let mut flags: Vec<&str> = Vec::new();

        // No extra argument: use current directory
        if args.len() <= 2 {
            (path, flags)
        } else {
            // Look for `--` separator starting from index 2 (after the command)
            let sep_pos = args[2..].iter().position(|s| s == "--");
            match sep_pos {
                Some(rel_idx) => {
                    let idx = 2 + rel_idx;
                    // Everything after `--` are literal flags
                    flags = args[idx + 1..].iter().map(|s| s.as_str()).collect();
                    // Before `--` we may or may not have an explicit path
                    if idx > 2 {
                        path = &args[2];
                    }
                    (path, flags)
                }
                None => {
                    // No `--`: keep legacy heuristic
                    match args.get(2) {
                        None => (".", Vec::new()),
                        Some(s) if s.starts_with('-') => {
                            flags = args[2..].iter().map(|s| s.as_str()).collect();
                            (".", flags)
                        }
                        Some(s) => {
                            path = s;
                            flags = args[3..].iter().map(|s| s.as_str()).collect();
                            (path, flags)
                        }
                    }
                }
            }
        }
    };

    let target_path = Path::new(path_arg);

    // FIX: extra_flags is now forwarded to ALL build/install commands,
    // not only the *-custom variants. This lets the user append extra makepkg
    // flags to any command without having to use -custom explicitly.
    match command.as_str() {
        "build"            => actions::build::run(target_path, &extra_flags),
        "build-clean"      => {
            let mut flags = vec!["-c"];
            flags.extend_from_slice(&extra_flags);
            actions::build::run(target_path, &flags)
        }
        "build-force"      => {
            let mut flags = vec!["-f"];
            flags.extend_from_slice(&extra_flags);
            actions::build::run(target_path, &flags)
        }
        "build-nocheck"    => {
            let mut flags = vec!["--nocheck"];
            flags.extend_from_slice(&extra_flags);
            actions::build::run(target_path, &flags)
        }
        "build-nogpg"      => {
            let mut flags = vec!["--skippgpcheck"];
            flags.extend_from_slice(&extra_flags);
            actions::build::run(target_path, &flags)
        }
        "build-custom"     => actions::build::run(target_path, &extra_flags),

        "install"          => actions::install::run(target_path, &extra_flags),
        "install-clean"    => {
            let mut flags = vec!["-c"];
            flags.extend_from_slice(&extra_flags);
            actions::install::run(target_path, &flags)
        }
        "install-force"    => {
            let mut flags = vec!["-f"];
            flags.extend_from_slice(&extra_flags);
            actions::install::run(target_path, &flags)
        }
        "install-rmdeps"   => {
            let mut flags = vec!["-r"];
            flags.extend_from_slice(&extra_flags);
            actions::install::run(target_path, &flags)
        }
        "install-nocheck"  => {
            let mut flags = vec!["--nocheck"];
            flags.extend_from_slice(&extra_flags);
            actions::install::run(target_path, &flags)
        }
        "install-nogpg"    => {
            let mut flags = vec!["--skippgpcheck"];
            flags.extend_from_slice(&extra_flags);
            actions::install::run(target_path, &flags)
        }
        "install-custom"   => actions::install::run(target_path, &extra_flags),

        // FIX: fetch-sources now forwards extra_flags (was discarding them before)
        "fetch-sources"    => {
            let mut flags = vec!["-o"];
            flags.extend_from_slice(&extra_flags);
            actions::build::run(target_path, &flags)
        }

        "checksums"        => actions::checksums::run(target_path),
        "genchecksums"     => actions::checksums::generate(target_path),
        "srcinfo"          => actions::srcinfo::run(target_path),

        "namcap"           => actions::namcap::run(target_path),
        "shellcheck"       => actions::shellcheck::run(target_path),

        "clean"            => actions::clean::run(target_path, false),
        "clean-all"        => actions::clean::run(target_path, true),

        "aur-push"         => {
            let message = extra_flags.first().copied();
            actions::aur_push::run(target_path, message)
        }
        "aur-push-tag"     => {
            let tag = extra_flags.first().copied()
                .ok_or_else(|| anyhow::anyhow!(gettext("aur-push-tag requires a version tag argument")))?;
            actions::aur_push::run_with_tag(target_path, tag)
        }

        "setup-nautilus"   => setup_nautilus(),

        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }

        _ => {
            eprintln!("{}: {}", gettext("Unknown command"), command);
            print_usage();
            std::process::exit(1);
        }
    }
}

/// setup-nautilus
///
/// Removes any stale user-land symlinks/dirs created by older versions,
/// verifies the Nautilus Python extension is installed, then restarts
/// Nautilus so the extension loads cleanly.
///
/// The Python extension (pkgbuild_manager.py) is the ONLY source of the
/// PKGBUILD context-menu. It reads scripts from
/// /usr/share/pkgbuild-manager/scripts/, translates labels via .mo files,
/// and filters internal helpers. No user-land symlinks are needed.
fn setup_nautilus() -> Result<()> {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    let home = std::env::var("HOME")?;
    let scripts_root = PathBuf::from(&home).join(".local/share/nautilus/scripts");

    // ------------------------------------------------------------------
    // 1. Remove stale symlinks / dirs left by previous versions
    // ------------------------------------------------------------------
    let stale_names: &[&str] = &[
        "00_Full Workflow", "01_Build", "02_Install", "02b_Build and Clean",
        "03_Update Checksums", "04_Update .SRCINFO", "05_Namcap", "05b_ShellCheck",
        "06_Push AUR", "07_Clean srcdir", "07b_Clean Everything",
        "00_Fluxo completo", "01_Compilar", "02_Instalar", "02b_Compilar e Limpar",
        "03_Atualizar checksums", "04_Atualizar .SRCINFO", "07_Limpar srcdir",
        "07b_Clean tudo", "07b_Limpar tudo",
        "Fluxo Completo", "Compilar", "Instalar", "Compilar e Limpar",
        "Atualizar Checksums", "Atualizar .SRCINFO", "Namcap", "ShellCheck",
        "Enviar para AUR", "Limpar srcdir", "Limpar Tudo",
        "_run_in_terminal",
    ];
    for name in stale_names {
        let p = scripts_root.join(name);
        if let Ok(meta) = p.symlink_metadata() {
            let ft = meta.file_type();
            if ft.is_symlink() || ft.is_file() {
                let _ = fs::remove_file(&p);
            } else if ft.is_dir() {
                let _ = fs::remove_dir_all(&p);
            }
        }
    }
    let pkgbuild_dir = scripts_root.join("PKGBUILD");
    if pkgbuild_dir.symlink_metadata().is_ok() {
        if let Err(e) = fs::remove_dir_all(&pkgbuild_dir) {
            eprintln!("{}: {e}", gettext("Warning: could not remove stale PKGBUILD dir"));
        }
    }

    // ------------------------------------------------------------------
    // 2. Verify the Python extension is installed
    // ------------------------------------------------------------------
    let ext_path = PathBuf::from("/usr/share/nautilus-python/extensions/pkgbuild_manager.py");
    if !ext_path.exists() {
        eprintln!(
            "{}: {}",
            gettext("Warning: Nautilus extension not found"),
            ext_path.display()
        );
        eprintln!("{}", gettext("Please reinstall pkgbuild-manager."));
    }

    // ------------------------------------------------------------------
    // 3. Restart Nautilus
    // ------------------------------------------------------------------
    println!("{}", gettext("Restarting Nautilus…"));
    if let Ok(status) = Command::new("nautilus").arg("-q").status() {
        if status.success() {
            std::thread::sleep(std::time::Duration::from_millis(600));
            let _ = Command::new("nautilus").spawn();
            println!("{}", gettext("Nautilus restarted."));
        }
    }

    Ok(())
}

fn print_usage() {
    println!("pkgbuild_manager — PKGBUILD Manager CLI\n");
    println!("Usage: pkgbuild_manager <command> [path] [-- extra-makepkg-flags]\n");
    println!("Build commands:");
    println!("  build              {}", gettext("Build the package (makepkg)"));
    println!("  build-clean        {}", gettext("Build and clean srcdir (-c)"));
    println!("  build-force        {}", gettext("Force rebuild even if package exists (-f)"));
    println!("  build-nocheck      {}", gettext("Build without running check() (--nocheck)"));
    println!("  build-nogpg        {}", gettext("Build skipping PGP checks (--skippgpcheck)"));
    println!("  build-custom       {}", gettext("Build with custom flags (pass after --)"));
    println!();
    println!("Install commands:");
    println!("  install            {}", gettext("Build and install (makepkg -si)"));
    println!("  install-clean      {}", gettext("Build, install and clean srcdir"));
    println!("  install-force      {}", gettext("Force build and install"));
    println!("  install-rmdeps     {}", gettext("Install and remove makedeps after (-r)"));
    println!("  install-nocheck    {}", gettext("Install without check()"));
    println!("  install-nogpg      {}", gettext("Install skipping PGP checks"));
    println!("  install-custom     {}", gettext("Install with custom flags (pass after --)"));
    println!();
    println!("Source / metadata:");
    println!("  fetch-sources      {}", gettext("Download and extract sources only (makepkg -o)"));
    println!("  checksums          {}", gettext("Update checksums in PKGBUILD (updpkgsums)"));
    println!("  genchecksums       {}", gettext("Generate checksums and print to stdout (makepkg -g)"));
    println!("  srcinfo            {}", gettext("Regenerate .SRCINFO (makepkg --printsrcinfo)"));
    println!();
    println!("Audit:");
    println!("  namcap             {}", gettext("Run namcap on PKGBUILD and built packages"));
    println!("  shellcheck         {}", gettext("Run shellcheck on PKGBUILD"));
    println!();
    println!("Clean:");
    println!("  clean              {}", gettext("Remove srcdir (makepkg -c)"));
    println!("  clean-all          {}", gettext("Remove srcdir, pkgdir, built packages and cache dirs"));
    println!();
    println!("AUR / Git:");
    println!("  aur-push [msg]     {}", gettext("Stage, commit (upgpkg: …) and push to AUR"));
    println!("  aur-push-tag <tag> {}", gettext("Same as aur-push plus creates an annotated tag"));
    println!();
    println!("Setup:");
    println!("  setup-nautilus     {}", gettext("Clean up stale scripts and restart Nautilus"));
    println!();
    println!("Other:");
    println!("  --version          {}", gettext("Print version and exit"));
    println!("  help, -h, --help   {}", gettext("Show this help"));
}
