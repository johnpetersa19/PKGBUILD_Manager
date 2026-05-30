use std::path::Path;
use std::process::Command;
use super::{get_target_dir, run_command};

/// Update checksums in-place using `updpkgsums`.
pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    run_command("updpkgsums", &[], &target_dir)
        .map_err(|e| {
            if e.to_string().contains("NotFound") || e.to_string().contains("No such file") {
                gettextrs::gettext("updpkgsums not found. Install it with: sudo pacman -S pacman-contrib").into()
            } else {
                e
            }
        })
}

/// Generate checksums and print them to stdout using `makepkg -g`.
/// This is the equivalent of `makepkg -g >> PKGBUILD` — the caller decides what to do with output.
pub fn generate(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    println!(">>> makepkg -g (in {:?})", target_dir);

    let output = Command::new("makepkg")
        .arg("-g")
        .current_dir(&target_dir)
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{} {}", gettextrs::gettext("makepkg -g failed:"), err).into());
    }

    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}
