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
    let _ = bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR);
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
    println!("  help               {}", gettext("Show this help message"));
}
