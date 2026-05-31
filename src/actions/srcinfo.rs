use std::path::Path;
use std::process::Command;
use std::fs::File;
use std::io::Write;
use super::get_target_dir;

pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = get_target_dir(path)?;
    println!("{} {:?}", gettextrs::gettext(">>> Regenerating .SRCINFO in"), target_dir);

    let output = Command::new("makepkg")
        .arg("--printsrcinfo")
        .current_dir(&target_dir)
        .output()?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{}: {}",
            gettextrs::gettext("makepkg --printsrcinfo failed"),
            err_msg
        ).into());
    }

    let srcinfo_path = target_dir.join(".SRCINFO");
    let mut file = File::create(srcinfo_path)?;
    file.write_all(&output.stdout)?;

    Ok(())
}
