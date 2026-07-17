use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const BLUEPRINTS: &[&str] = &[
    "clean",
    "settings",
    "push",
    "release",
    "step-row",
    "add-action-dialog",
    "branch-popover",
    "unknown-repository",
    "clone-progress",
    "error-alert",
    "attachment-chooser",
    "progress-panel",
];

fn main() {
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    for name in BLUEPRINTS {
        let source = Path::new("data/ui").join(format!("{name}.blp"));
        let output = output_dir.join(format!("{name}.ui"));
        println!("cargo:rerun-if-changed={}", source.display());
        let status = Command::new("blueprint-compiler")
            .arg("compile")
            .arg("--output")
            .arg(&output)
            .arg(&source)
            .status()
            .unwrap_or_else(|error| {
                panic!("blueprint-compiler is required to build the GUI: {error}")
            });
        assert!(status.success(), "failed to compile {}", source.display());
    }
}
