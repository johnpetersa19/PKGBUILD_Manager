use std::io;
use std::path::Path;
use std::process::Command;
use super::{get_target_dir, run_command};

/// Update checksums in-place using `updpkgsums`.
pub fn run(path: &Path) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;
    run_command("updpkgsums", &[], &target_dir).map_err(|e| {
        if let Some(io_err) = e.downcast_ref::<io::Error>() {
            if io_err.kind() == io::ErrorKind::NotFound {
                return anyhow::anyhow!(
                    "{}",
                    gettextrs::gettext(
                        "updpkgsums not found. Install it with: sudo pacman -S pacman-contrib",
                    )
                );
            }
        }
        e
    })
}

/// Generate checksums and print them to stdout using `makepkg -g`.
/// Bug #9 fix: adicionado handler para NotFound (makepkg não instalado),
/// produzindo mensagem amigável ao invés de panic de IO genérico.
pub fn generate(path: &Path) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;
    println!(">>> makepkg -g (in {:?})", target_dir);

    let output = Command::new("makepkg")
        .arg("-g")
        .current_dir(&target_dir)
        .output()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                anyhow::anyhow!(
                    "{}",
                    gettextrs::gettext(
                        "makepkg not found. Install it with: sudo pacman -S pacman"
                    )
                )
            } else {
                anyhow::anyhow!(e)
            }
        })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "{} {}",
            gettextrs::gettext("makepkg -g failed:"),
            err.trim()
        ));
    }

    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}
