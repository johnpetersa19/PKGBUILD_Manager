//! Process launcher for system tools used from native and Flatpak builds.

use std::process::Command;

pub fn is_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
        || std::env::var_os("FLATPAK_ID").is_some()
}

/// Run Arch packaging and desktop tools on the host from inside Flatpak.
pub fn command(program: &str) -> Command {
    if is_flatpak() {
        let mut command = Command::new("flatpak-spawn");
        command.arg("--host").arg(program);
        command
    } else {
        Command::new(program)
    }
}
