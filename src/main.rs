/* main.rs
 *
 * Copyright 2026 Unknown
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
use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain, gettext};
use std::env;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up gettext translations
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

    // Commands that accept [path] [flags...] pattern:
    //   pkgbuild_manager <command> [path] [extra flags...]
    //
    // If the second arg starts with '-', treat it as a flag with CWD as path.
    // Otherwise treat second arg as path and remaining as flags.
    let (path_arg, extra_flags): (&str, Vec<&str>) = match args.get(2) {
        None => (".", vec![]),
        Some(s) if s.starts_with('-') => (".", args[2..].iter().map(|s| s.as_str()).collect()),
        Some(s) => (s.as_str(), args[3..].iter().map(|s| s.as_str()).collect()),
    };

    let target_path = Path::new(path_arg);

    match command.as_str() {
        // --- makepkg variants ---
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

        // --- checksums & srcinfo ---
        "checksums"        => actions::checksums::run(target_path)?,
        "genchecksums"     => actions::checksums::generate(target_path)?,
        "srcinfo"          => actions::srcinfo::run(target_path)?,

        // --- audit / quality ---
        "namcap"           => actions::namcap::run(target_path)?,
        "shellcheck"       => actions::shellcheck::run(target_path)?,

        // --- clean ---
        "clean"            => actions::clean::run(target_path, false)?,
        "clean-all"        => actions::clean::run(target_path, true)?,

        // --- AUR git ---
        "aur-push"         => {
            // aur-push [path] [commit message]
            let message = extra_flags.first().copied();
            actions::aur_push::run(target_path, message)?
        }
        "aur-push-tag"     => {
            let tag = extra_flags.first().copied()
                .ok_or_else(|| gettext("aur-push-tag requires a version tag argument"))?;
            actions::aur_push::run_with_tag(target_path, tag)?
        }

        "setup-nautilus"   => {
            setup_nautilus()?;
        }

        "help" | "-h" | "--help" => {
            print_usage();
        }
        _ => {
            eprintln!("{}: {}", gettext("Unknown command"), command);
            print_usage();
            std::process::exit(1);
        }
    }

    Ok(())
}

fn setup_nautilus() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    let home = std::env::var("HOME")?;
    let base_scripts_dir = PathBuf::from(&home).join(".local/share/nautilus/scripts");
    let scripts_dir = base_scripts_dir.join("PKGBUILD");

    // 1. Clean up old top-level symlinks if they exist to prevent clutter
    let old_names = vec![
        "00_Full Workflow", "00_Fluxo completo",
        "01_Build", "01_Compilar",
        "02_Install", "02_Instalar",
        "02b_Build and Clean", "02b_Compilar e Limpar",
        "03_Update Checksums", "03_Atualizar checksums",
        "04_Update .SRCINFO", "04_Atualizar .SRCINFO",
        "05_Namcap",
        "05b_ShellCheck",
        "06_Push AUR",
        "07_Clean srcdir", "07_Limpar srcdir",
        "07b_Clean Everything", "07b_Clean tudo", "07b_Limpar tudo",
        "_run_in_terminal"
    ];
    for name in old_names {
        let old_file = base_scripts_dir.join(name);
        if old_file.exists() || old_file.is_symlink() {
            let _ = fs::remove_file(old_file);
        }
    }

    // 2. Setup the PKGBUILD subdirectory
    if scripts_dir.exists() {
        let _ = fs::remove_dir_all(&scripts_dir);
    }
    fs::create_dir_all(&scripts_dir)?;

    let mut system_scripts_dir = PathBuf::from("/usr/share/nautilus-scripts");
    if !system_scripts_dir.exists() {
        let local_dir = PathBuf::from("data/nautilus-scripts");
        if local_dir.exists() {
            system_scripts_dir = local_dir;
        }
    }

    let scripts = vec![
        ("00_Full Workflow", "00_Full Workflow"),
        ("01_Build", "01_Build"),
        ("02_Install", "02_Install"),
        ("02b_Build and Clean", "02b_Build and Clean"),
        ("03_Update Checksums", "03_Update Checksums"),
        ("04_Update .SRCINFO", "04_Update .SRCINFO"),
        ("05_Namcap", "05_Namcap"),
        ("05b_ShellCheck", "05b_ShellCheck"),
        ("06_Push AUR", "06_Push AUR"),
        ("07_Clean srcdir", "07_Clean srcdir"),
        ("07b_Clean Everything", "07b_Clean Everything"),
        ("_run_in_terminal", "_run_in_terminal"),
    ];

    for (file_name, gettext_key) in scripts {
        let src = system_scripts_dir.join(file_name);
        if src.exists() {
            let dest_name = if gettext_key == "_run_in_terminal" {
                "_run_in_terminal".to_string()
            } else {
                gettext(gettext_key)
            };
            let dest = scripts_dir.join(dest_name);
            let _ = symlink(&src, &dest);
        }
    }

    println!("{}", gettext("Nautilus scripts successfully configured under 'PKGBUILD' submenu."));
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
    println!("  setup-nautilus     {}", gettext("Symlink scripts to user directory with localization"));
    println!("  help               {}", gettext("Show this help message"));
}
