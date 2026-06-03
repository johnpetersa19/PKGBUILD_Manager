//! menu.json read / write and default menu structure.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Path helpers ──────────────────────────────────────────────────────────────

fn config_dir() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut h = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
            h.push(".config");
            h
        });
    base.join("pkgbuild-manager")
}

fn config_file() -> PathBuf {
    config_dir().join("menu.json")
}

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuGroup {
    pub group: String,
    pub items: Vec<MenuItem>,
}

// ── All known actions ─────────────────────────────────────────────────────────

pub fn all_actions() -> Vec<(&'static str, &'static str)> {
    vec![
        ("00_Full Workflow",     "Full Workflow"),
        ("01_Build",             "Build"),
        ("02b_Build and Clean",  "Build and Clean"),
        ("08_Build Force",       "Build Force"),
        ("09_Build NoCheck",     "Build NoCheck"),
        ("10_Build NoGPG",       "Build NoGPG"),
        ("11_Fetch Sources",     "Fetch Sources"),
        ("02_Install",           "Install"),
        ("12_Install Force",     "Install Force"),
        ("13_Install RmDeps",    "Install RmDeps"),
        ("14_Install NoCheck",   "Install NoCheck"),
        ("15_Install NoGPG",     "Install NoGPG"),
        ("03_Update Checksums",  "Update Checksums"),
        ("04_Update .SRCINFO",   "Update .SRCINFO"),
        ("16_Gen Checksums",     "Gen Checksums"),
        ("05_Namcap",            "Namcap"),
        ("05b_ShellCheck",       "ShellCheck"),
        ("06_Push AUR",          "Push AUR"),
        ("17_Push AUR Tag",      "Push AUR Tag"),
        ("07_Clean srcdir",      "Clean srcdir"),
        ("07b_Clean Everything", "Clean Everything"),
    ]
}

// ── Default menu ──────────────────────────────────────────────────────────────

pub fn default_menu() -> Vec<MenuGroup> {
    vec![
        MenuGroup {
            group: "Actions".into(),
            items: vec![
                MenuItem { id: "00_Full Workflow".into(),    label: "Full Workflow".into(),    enabled: true },
                MenuItem { id: "02_Install".into(),          label: "Install".into(),          enabled: true },
                MenuItem { id: "01_Build".into(),            label: "Build".into(),            enabled: true },
                MenuItem { id: "02b_Build and Clean".into(), label: "Build and Clean".into(),  enabled: true },
            ],
        },
        MenuGroup {
            group: "Metadata".into(),
            items: vec![
                MenuItem { id: "03_Update Checksums".into(), label: "Update Checksums".into(), enabled: true },
                MenuItem { id: "04_Update .SRCINFO".into(),  label: "Update .SRCINFO".into(),  enabled: true },
            ],
        },
        MenuGroup {
            group: "Audit".into(),
            items: vec![
                MenuItem { id: "05_Namcap".into(),      label: "Namcap".into(),      enabled: true },
                MenuItem { id: "05b_ShellCheck".into(), label: "ShellCheck".into(), enabled: true },
            ],
        },
        MenuGroup {
            group: "Git / AUR".into(),
            items: vec![
                MenuItem { id: "06_Push AUR".into(), label: "Push AUR".into(), enabled: true },
            ],
        },
        MenuGroup {
            group: "Clean".into(),
            items: vec![
                MenuItem { id: "07_Clean srcdir".into(),      label: "Clean srcdir".into(),      enabled: true },
                MenuItem { id: "07b_Clean Everything".into(), label: "Clean Everything".into(),  enabled: true },
            ],
        },
    ]
}

// ── Load / save ───────────────────────────────────────────────────────────────

pub fn load() -> Vec<MenuGroup> {
    let known: std::collections::HashSet<&str> =
        all_actions().iter().map(|(id, _)| *id).collect();

    (|| -> Option<Vec<MenuGroup>> {
        let text = std::fs::read_to_string(config_file()).ok()?;
        let mut groups: Vec<MenuGroup> = serde_json::from_str(&text).ok()?;
        for g in &mut groups {
            g.items.retain(|i| known.contains(i.id.as_str()));
        }
        Some(groups)
    })()
    .unwrap_or_else(default_menu)
}

pub fn save(data: &[MenuGroup]) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(config_file(), json)
}
