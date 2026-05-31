use std::io;
use std::path::Path;
use std::process::Command;
use super::{get_target_dir, run_command};

/// Update checksums in-place using `updpkgsums`.
pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    run_command("updpkgsums", &[], &target_dir)
        .map_err(|e| {
            // Downcast to io::Error to check for NotFound reliably
            if let Some(io_err) = e.downcast_ref::<io::Error>() {
                if io_err.kind() == io::ErrorKind::NotFound {
                    return gettextrs::gettext(
                        "updpkgsums not found. Install it with: sudo pacman -S pacman-contrib",
                    )
                    .into();
                }
            }
            e
        })
}

/// Generate checksums and print them to stdout using `makepkg -g`.
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
