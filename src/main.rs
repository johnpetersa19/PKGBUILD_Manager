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
mod host;

use anyhow::Result;
use config::{GETTEXT_PACKAGE, LOCALEDIR, VERSION};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain, gettext, LocaleCategory};
use std::env;
use std::path::Path;

fn main() -> Result<()> {
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

    // Argument parsing rules (conventional POSIX/GNU style):
    //
    //   pkgbuild_manager <command> [path] [flags…]
    //   pkgbuild_manager <command> [path] -- [extra-flags…]
    //
    // The optional '--' separator splits the path/flags region from any
    // extra flags that should be forwarded verbatim to the underlying tool
    // (makepkg, git, …).  Everything after '--' is treated as an extra flag
    // regardless of its form.
    //
    // When '--' is absent:
    //   • If args[2] starts with '-' it is treated as a flag, NOT a path,
    //     and the path defaults to ".".  This is the standard Unix convention
    //     (options begin with '-'; positional arguments do not).
    //   • Otherwise args[2] is the path and args[3..] are flags.
    //
    // NOTE: a path that genuinely begins with '-' (extremely rare on real
    // filesystems) must be disambiguated with the '--' separator:
    //   pkgbuild_manager build -- -my-weird-dir      # NOT supported: '-my-weird-dir' after '--' is a flag
    //   pkgbuild_manager build ./-my-weird-dir       # CORRECT: prefix with './'
    // This matches the behaviour of every standard Unix tool (ls, cp, rm, …).

    let (path_arg, extra_flags): (&str, Vec<&str>) = {
        let mut path: &str = ".";
        let mut flags: Vec<&str> = Vec::new();

        if args.len() <= 2 {
            // No path/flags provided — use defaults.
            (path, flags)
        } else {
            // Search for a '--' separator anywhere in args[2..].
            let sep_pos = args[2..].iter().position(|s| s == "--");

            match sep_pos {
                Some(rel_idx) => {
                    // '--' found at absolute index `idx`.
                    let idx = 2 + rel_idx;

                    // Everything after '--' are extra flags forwarded verbatim.
                    flags = args[idx + 1..].iter().map(|s| s.as_str()).collect();

                    // args[2] is the path only when the separator is NOT at
                    // args[2] itself (i.e. the user supplied a path before '--').
                    // Example:  build /some/path -- -f   →  path="/some/path", flags=["-f"]
                    // Example:  build -- -f              →  path=".",           flags=["-f"]
                    if idx > 2 {
                        path = &args[2];
                    }
                    (path, flags)
                }
                None => {
                    // No '--' separator.  Use the starts_with('-') heuristic:
                    // if args[2] looks like a flag, treat the entire tail as flags.
                    // This is the standard Unix convention — options start with '-',
                    // paths do not (use './' prefix if your path starts with '-').
                    match args.get(2) {
                        None => (".", Vec::new()),
                        Some(s) if s.starts_with('-') => {
                            // args[2] is a flag — no path argument provided.
                            flags = args[2..].iter().map(|s| s.as_str()).collect();
                            (".", flags)
                        }
                        Some(s) => {
                            // args[2] is the path; the rest are flags.
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

    fn merge_flags<'a>(base: &[&'a str], extra: &[&'a str]) -> Vec<&'a str> {
        let mut v = base.to_vec();
        v.extend_from_slice(extra);
        v
    }

    match command.as_str() {
        "build"            => actions::build::run(target_path, &extra_flags),
        "build-clean"      => actions::build::run(target_path, &merge_flags(&["-c"], &extra_flags)),
        "build-force"      => actions::build::run(target_path, &merge_flags(&["-f"], &extra_flags)),
        "build-nocheck"    => actions::build::run(target_path, &merge_flags(&["--nocheck"], &extra_flags)),
        "build-nogpg"      => actions::build::run(target_path, &merge_flags(&["--skippgpcheck"], &extra_flags)),
        // Intentional pass-through alias for `build`.
        // Exists as a distinct subcommand name so GUI frontends (Paru GUI, Nautilus scripts)
        // can expose a dedicated "custom flags" entry point without special-casing `build`.
        // Behaviour: identical to `build` — all extra_flags are forwarded unchanged.
        "build-custom"     => actions::build::run(target_path, &extra_flags),

        "install"          => actions::install::run(target_path, &extra_flags),
        "install-clean"    => actions::install::run(target_path, &merge_flags(&["-c"], &extra_flags)),
        "install-force"    => actions::install::run(target_path, &merge_flags(&["-f"], &extra_flags)),
        "install-rmdeps"   => actions::install::run(target_path, &merge_flags(&["-r"], &extra_flags)),
        "install-nocheck"  => actions::install::run(target_path, &merge_flags(&["--nocheck"], &extra_flags)),
        "install-nogpg"    => actions::install::run(target_path, &merge_flags(&["--skippgpcheck"], &extra_flags)),
        // Intentional pass-through alias for `install`.
        // Same rationale as `build-custom` above: distinct name for GUI/script consumers
        // that need to invoke install with arbitrary user-supplied flags.
        // Behaviour: identical to `install` — all extra_flags are forwarded unchanged.
        "install-custom"   => actions::install::run(target_path, &extra_flags),

        "fetch-sources"    => actions::build::run(target_path, &merge_flags(&["-o"], &extra_flags)),

        "checksums"        => actions::checksums::run(target_path),
        "genchecksums"     => actions::checksums::generate(target_path),
        "srcinfo"          => actions::srcinfo::run(target_path),

        "namcap"           => actions::namcap::run(target_path),
        "shellcheck"       => actions::shellcheck::run(target_path),

        // ── Validate commands (no compilation) ──────────────────────────────
        "validate"         => actions::validate::all_offline(target_path),
        "validate-syntax"  => actions::validate::syntax(target_path),
        "validate-parse"   => actions::validate::parse(target_path),
        "validate-sources" => actions::validate::verify_sources(target_path),
        "validate-deps"    => actions::validate::check_deps(target_path),

        "clean"            => actions::clean::run(target_path, false),
        "clean-all"        => actions::clean::run(target_path, true),

        "aur-push"         => {
            let message = extra_flags.first().copied();
            actions::aur_push::run(target_path, message)
        }
        "aur-push-tag"     => {
            let tag = extra_flags.first().copied()
                .ok_or_else(|| anyhow::anyhow!(gettext("aur-push-tag requires a version tag argument")))?;
            if tag.is_empty() {
                return Err(anyhow::anyhow!(gettext("aur-push-tag: tag cannot be empty")));
            }
            if tag.contains(' ') {
                return Err(anyhow::anyhow!(
                    "{}: '{}' — {}",
                    gettext("aur-push-tag: invalid tag"),
                    tag,
                    gettext("tags cannot contain spaces (expected format: pkgver-pkgrel, e.g. 1.2.3-1)")
                ));
            }
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

fn setup_nautilus() -> Result<()> {
    use std::fs;
    use std::path::PathBuf;

    let home = std::env::var("HOME")?;
    let scripts_root = PathBuf::from(&home).join(".local/share/nautilus/scripts");

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
            eprintln!("{}: {e}", gettext("Warning: Could not remove obsolete PKGBUILD directory"));
        }
    }

    let ext_path = [
        "/usr/share/nautilus-python/extensions/pkgbuild_manager.py",
        "/usr/local/share/nautilus-python/extensions/pkgbuild_manager.py",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists());
    if ext_path.is_none() {
        eprintln!(
            "{}\n  /usr/share/nautilus-python/extensions/pkgbuild_manager.py\n  /usr/local/share/nautilus-python/extensions/pkgbuild_manager.py",
            gettext("Warning: Nautilus Python extension not found at"),
        );
        eprintln!("{}", gettext("Install the pkgbuild-manager package to get the extension."));
    } else {
        let ext_path = ext_path.expect("checked above");
        println!("{}: {}", gettext("Extension found"), ext_path.display());

        // Nautilus loads extensions from both the system and per-user data
        // directories. Keeping this provider in both places duplicates every
        // context-menu item. Prefer the packaged system extension when it is
        // available and remove only this project's known user-side copies.
        let mut user_extensions = vec![
            PathBuf::from(&home)
                .join(".local/share/nautilus-python/extensions/pkgbuild_manager.py"),
        ];
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            let xdg_extension = PathBuf::from(data_home)
                .join("nautilus-python/extensions/pkgbuild_manager.py");
            if !user_extensions.iter().any(|path| path == &xdg_extension) {
                user_extensions.push(xdg_extension);
            }
        }

        for user_extension in user_extensions {
            if user_extension.is_file() || user_extension.is_symlink() {
                fs::remove_file(&user_extension).map_err(|error| {
                    anyhow::anyhow!(
                        "{}: {}: {error}",
                        gettext("Could not remove duplicate Nautilus extension"),
                        user_extension.display()
                    )
                })?;
                println!(
                    "{}: {}",
                    gettext("Removed duplicate Nautilus extension"),
                    user_extension.display()
                );
            }
        }
    }

    println!("{}", gettext("Restarting Nautilus\u{2026}"));
    let _ = crate::host::command("nautilus").arg("-q").status();
    std::thread::sleep(std::time::Duration::from_millis(800));
    let _ = crate::host::command("nautilus").spawn();

    println!("{}", gettext("Done. Right-click a PKGBUILD directory to see the menu."));
    Ok(())
}

fn print_usage() {
    println!("PKGBUILD Manager - CLI Tool\n");
    println!("Usages:\n");
    println!("Compilation Commands:");
    println!("  build              {}", gettext("Compiled package (makepkg)"));
    println!("  build-clean        {}", gettext("Compile and clean srcdir (makepkg -c)"));
    println!("  build-force        {}", gettext("Force recompilation (makepkg -f)"));
    println!("  build-nocheck      {}", gettext("Basic check() function (makepkg --nocheck)"));
    println!("  build-nogpg        {}", gettext("Popular PGP signature check (makepkg --skippgpcheck)"));
    println!("  build-custom       {}", gettext("Same as 'build' — all extra flags after the path are forwarded to makepkg"));
    println!();
    println!("Installation Commands:");
    println!("  install            {}", gettext("Compile, install and resolve dependencies (makepkg -si)"));
    println!("  install-clean      {}", gettext("Compile, install and clean srcdir (makepkg -sic)"));
    println!("  install-force      {}", gettext("Force compilation and installation (makepkg -sif)"));
    println!("  install-rmdeps     {}", gettext("Install and remove makedeps afterwards (makepkg -sir)"));
    println!("  install-nocheck    {}", gettext("Install without running check()"));
    println!("  install-nogpg      {}", gettext("Install skipping PGP checks"));
    println!("  install-custom     {}", gettext("Same as 'install' — all extra flags after the path are forwarded to makepkg"));
    println!();
    println!("Package metadata commands:");
    println!("  fetch-sources      {}", gettext("Download and extract sources only (makepkg -o)"));
    println!("  checksums          {}", gettext("Update checksums without PKGBUILD (updpkgsums)"));
    println!("  genchecksums       {}", gettext("Generate checksums and print to standard output (makepkg -g)"));
    println!("  srcinfo            {}", gettext("Regenerate .SRCINFO (makepkg --printsrcinfo)"));
    println!();
    println!("Auditing commands:");
    println!("  namcap             {}", gettext("Run namcap on PKGBUILD and compiled packages"));
    println!("  shellcheck         {}", gettext("Run ShellCheck on PKGBUILD"));
    println!();
    println!("Validate commands (no compilation):");
    println!("  validate           {}", gettext("Full offline check: syntax + namcap + shellcheck (no network)"));
    println!("  validate-syntax    {}", gettext("Validate syntax only (makepkg --printsrcinfo)"));
    println!("  validate-parse     {}", gettext("Parse variables/functions without extracting (makepkg --nobuild --noextract); may access network on VCS packages"));
    println!("  validate-sources   {}", gettext("Download sources and verify checksums (makepkg --verifysource)"));
    println!("  validate-deps      {}", gettext("Check declared dependencies exist in repos (makepkg --syncdeps --nobuild)"));
    println!();
    println!("Cleanup commands:");
    println!("  clean              {}", gettext("Clean srcdir with makepkg (makepkg -c)"));
    println!("  clean-all          {}", gettext("Remove src/, packages/ and compiled packages"));
    println!();
    println!("AUR/Git commands:");
    println!("  aur-push [msg]     {}", gettext("Stage, commit and push to the AUR (automatic message if not provided)"));
    println!("  aur-push-tag <tag> {}", gettext("Push with version tag (e.g., 1.2.3-1)"));
    println!();
    println!("Other:");
    println!("  setup-nautilus     {}", gettext("Remove obsolete symbolic links and check the Nautilus extension"));
    println!("  --version          {}", gettext("Display program version"));
    println!("  help, -h, --help   {}", gettext("Display this help message"));
}
