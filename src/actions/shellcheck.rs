use std::path::Path;
use super::{get_target_dir, run_command};

/// Run `shellcheck --shell=bash --exclude=SC2034,SC2154,SC2164 PKGBUILD`.
///
/// The excluded rules are intentionally omitted because they fire on valid
/// PKGBUILD patterns that makepkg handles itself:
///   SC2034 – variables "unused" but consumed by makepkg's own scope
///   SC2154 – variables referenced before assignment (normal in PKGBUILD)
///   SC2164 – `cd` without error check (makepkg wraps everything safely)
pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    run_command(
        "shellcheck",
        &["--shell=bash", "--exclude=SC2034,SC2154,SC2164", "PKGBUILD"],
        &target_dir,
    )
    .map_err(|e| {
        if e.to_string().contains("NotFound") || e.to_string().contains("No such file") {
            gettextrs::gettext("shellcheck not found. Install it with: sudo pacman -S shellcheck").into()
        } else {
            e
        }
    })
}
