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

use config::{GETTEXT_PACKAGE, LOCALEDIR};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain, gettext, LocaleCategory};
use std::env;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // setlocale MUST be called before bindtextdomain/textdomain so that
    // the C library initialises the locale from the environment (LANG,
    // LC_ALL, …). Without this call gettextrs silently falls back to the
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

    // Support optional `--` separator between path and flags so that
    // directories starting with '-' can still be used as path.
    // Forms aceitas:
    //   pkgbuild_manager build                # path = ".", flags = []
    //   pkgbuild_manager build /dir           # path = "/dir", flags = []
    //   pkgbuild_manager build -- -c -f       # path = ".",   flags = ["-c", "-f"]
    //   pkgbuild_manager build /dir -- -c -f  # path = "/dir", flags = ["-c", "-f"]
    //   pkgbuild_manager build -c -f          # path = ".",   flags = ["-c", "-f"]
    let (path_arg, extra_flags): (&str, Vec<&str>) = {
        let mut path: &str = ".";
        let mut flags: Vec<&str> = Vec::new();

        // Nenhum argumento extra: usa diretório atual
        if args.len() <= 2 {
            (path, flags)
        } else {
            // Procura por `--` a partir do índice 2 (depois do comando)
            let sep_pos = args[2..].iter().position(|s| s == "--");
            match sep_pos {
                Some(rel_idx) => {
                    let idx = 2 + rel_idx;
                    // Tudo depois de `--` são flags literais
                    flags = args[idx + 1..].iter().map(|s| s.as_str()).collect();
                    // Antes do `--` podemos ter ou não path explícito
                    if idx > 2 {
                        path = &args[2];
                    }
                    (path, flags)
                }
                None => {
                    // Sem `--`: mantém heurística antiga
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

    match command.as_str() {
        "build"            => actions::build::run(target_path, &[])?,
        "build-clean"      => actions::build::run(target_path, &["-c"])?,
        "build-force"      => actions::build::run(target_path, &["-f"])?,
        "build-nocheck"    => actions::build::run(target_path, &["--nocheck"])?,
        "build-nogpg"      => actions::build::run(target_path, &["--skippgpcheck"])?,
        "build-custom"     => actions::build::run(target_path, &extra_flags)?,

        "install"          => actions::install::run(target_path, &[])?,
        "install-clean"    => actions::install::run(target_path, &["-c"])?,
        "install-force"    => actions::install::run(target_path, &["-f"])?,
        "install-rmdeps"   => actions::install::run(target_path, &["-r"])?,
        "install-nocheck"  => actions::install::run(target_path, &["--nocheck"])?,
        "install-nogpg"    => actions::install::run(target_path, &["--skippgpcheck"])?,
        "install-custom"   => actions::install::run(target_path, &extra_flags)?,

        "fetch-sources"    => actions::build::run(target_path, &["-o"])?,

        "checksums"        => actions::checksums::run(target_path)?,
        "genchecksums"     => actions::checksums::generate(target_path)?,
        "srcinfo"          => actions::srcinfo::run(target_path)?,

        "namcap"           => actions::namcap::run(target_path)?,
        "shellcheck"       => actions::shellcheck::run(target_path)?,

        "clean"            => actions::clean::run(target_path, false)?,
        "clean-all"        => actions::clean::run(target_path, true)?,

        "aur-push"         => {
            let message = extra_flags.first().copied();
            actions::aur_push::run(target_path, message)?
        }
        "aur-push-tag"     => {
            let tag = extra_flags.first().copied()
                .ok_or_else(|| gettext("aur-push-tag requires a version tag argument"))?;
            actions::aur_push::run_with_tag(target_path, tag)?
        }

        "setup-nautilus"   => setup_nautilus()?,

        "help" | "-h" | "--help" => print_usage(),

        _ => {
            eprintln!("{}: {}", gettext("Unknown command"), command);
            print_usage();
            std::process::exit(1);
        }
    }

    Ok(())
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
fn setup_nautilus() -> Result<(), Box<dyn std::error::Error>> {
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
    let ext = PathBuf::from("/usr/share/nautilus-python/extensions/pkgbuild_manager.py");
    if !ext.exists() {
        return Err(gettext(
            "Nautilus extension not found at \
             /usr/share/nautilus-python/extensions/pkgbuild_manager.py. \
             Please reinstall the package."
        ).into());
    }

    // ------------------------------------------------------------------
    // 3. Restart Nautilus
    // ------------------------------------------------------------------
    let _ = Command::new("nautilus").arg("-q").status();

    println!(
        "{}",
        gettext(
            "PKGBUILD Manager: Nautilus extension active. \
             Right-click any PKGBUILD file to see the PKGBUILD submenu."
        )
    );
    Ok(())
}

fn print_usage() {
    println!("{}", gettext("PKGBUILD Manager - CLI Tool"));
    println!("\n{}", gettext("Usage:"));
    println!("  pkgbuild_manager <command> [path] [flags...]");

    println!("\n{}:", gettext("Build commands"));
    println!("  build              {}", gettext("Compile package (makepkg)"));
    println!("  build-clean        {}", gettext("Compile and clean srcdir (makepkg -c)"));
    println!("  build-force        {}", gettext("Force recompile (makepkg -f)"));
    println!("  build-nocheck      {}", gettext("Skip check() function (makepkg --nocheck)"));
    println!("  build-nogpg        {}", gettext("Skip PGP signature check (makepkg --skippgpcheck)"));
    println!("  build-custom       {}", gettext("Compile with custom flags passed after path"));
    println!("  fetch-sources      {}", gettext("Download and extract sources only (makepkg -o)"));

    println!("\n{}:", gettext("Install commands"));
    println!("  install            {}", gettext("Compile, install and resolve deps (makepkg -si)"));
    println!("  install-clean      {}", gettext("Compile, install and clean srcdir (makepkg -sic)"));
    println!("  install-force      {}", gettext("Force compile and install (makepkg -sif)"));
    println!("  install-rmdeps     {}", gettext("Install and remove makedeps after (makepkg -sir)"));
    println!("  install-nocheck    {}", gettext("Install without running check()"));
    println!("  install-nogpg      {}", gettext("Install skipping PGP checks"));
    println!("  install-custom     {}", gettext("Install with custom flags passed after path"));

    println!("\n{}:", gettext("Package metadata commands"));
    println!("  checksums          {}", gettext("Update checksums in PKGBUILD (updpkgsums)"));
    println!("  genchecksums       {}", gettext("Generate checksums and print to stdout (makepkg -g)"));
    println!("  srcinfo            {}", gettext("Regenerate .SRCINFO (makepkg --printsrcinfo)"));

    println!("\n{}:", gettext("Audit commands"));
    println!("  namcap             {}", gettext("Run namcap on PKGBUILD and built packages"));
    println!("  shellcheck         {}", gettext("Run shellcheck on PKGBUILD"));

    println!("\n{}:", gettext("Clean commands"));
    println!("  clean              {}", gettext("Clean srcdir with makepkg (makepkg -c)"));
    println!("  clean-all          {}", gettext("Remove src/, pkg/ and built packages"));

    println!("\n{}:", gettext("AUR/Git commands"));
    println!("  aur-push [msg]     {}", gettext("Stage, commit and push to AUR (auto message if not provided)"));
    println!("  aur-push-tag <ver> {}", gettext("Push with version tag (e.g. 1.2.3-1)"));

    println!("\n{}:", gettext("Other"));
    println!("  setup-nautilus     {}", gettext("Remove stale symlinks and verify the Nautilus extension"));
    println!("  help               {}", gettext("Show this help message"));
}
