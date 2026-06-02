use std::path::Path;
use super::run_makepkg;

/// Run `makepkg [extra_flags]` in the target directory.
/// extra_flags: e.g. &["-c"], &["-f"], &["--nocheck"], &["--skippgpcheck"], etc.
pub fn run(path: &Path, extra_flags: &[&str]) -> anyhow::Result<()> {
    run_makepkg(path, &[], extra_flags)
}
