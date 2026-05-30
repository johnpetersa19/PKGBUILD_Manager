use std::path::Path;
use super::{get_target_dir, run_command};

/// Run `makepkg -si [extra_flags]` in the target directory.
/// extra_flags: e.g. &["-c"], &["-f"], &["-r"], &["--nocheck"], etc.
pub fn run(path: &Path, extra_flags: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    let mut args: Vec<&str> = vec!["-si"];
    args.extend_from_slice(extra_flags);
    run_command("makepkg", &args, &target_dir)
}
