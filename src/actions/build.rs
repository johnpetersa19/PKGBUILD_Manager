use std::path::Path;
use super::{get_target_dir, run_command};

/// Run `makepkg [extra_flags]` in the target directory.
/// extra_flags: e.g. &["-c"], &["-f"], &["--nocheck"], &["--skippgpcheck"], etc.
pub fn run(path: &Path, extra_flags: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    run_command("makepkg", extra_flags, &target_dir)
}
