use std::path::Path;
use super::get_target_dir;
use super::regenerate_srcinfo;

pub fn run(path: &Path) -> anyhow::Result<()> {
    let target_dir = get_target_dir(path)?;
    // Shared helper already logs and writes .SRCINFO for us
    let _ = regenerate_srcinfo(&target_dir)?;
    Ok(())
}
