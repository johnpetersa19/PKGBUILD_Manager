use std::path::Path;
use super::run_makepkg;

/// Run `makepkg -si [extra_flags]` in the target directory.
/// extra_flags: e.g. &["-c"], &["-f"], &["-r"], &["--nocheck"], etc.
pub fn run(path: &Path, extra_flags: &[&str]) -> anyhow::Result<()> {
    run_makepkg(path, &["-si"], extra_flags)
}
