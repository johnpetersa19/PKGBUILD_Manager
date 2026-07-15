//! menu.json read / write and default menu structure.

use gettextrs::gettext;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Path helpers ────────────────────────────────────────────────────────────────

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

// ── All known actions ───────────────────────────────────────────────────────────────

pub fn all_actions() -> Vec<(String, String)> {
    [
        "00_Full Workflow", "01_Build", "02b_Build and Clean", "08_Build Force",
        "09_Build NoCheck", "10_Build NoGPG", "11_Fetch Sources", "02_Install",
        "12_Install Force", "13_Install RmDeps", "14_Install NoCheck", "15_Install NoGPG",
        "03_Update Checksums", "04_Update .SRCINFO", "16_Gen Checksums", "05_Namcap",
        "05b_ShellCheck", "06_Push AUR", "17_Push AUR Tag", "07_Clean srcdir",
        "07b_Clean Everything",
    ]
    .into_iter()
    .map(|id| (id.into(), action_label(id)))
    .collect()
}

fn action_label(id: &str) -> String {
    match id {
        "00_Full Workflow" => gettext("Full Workflow"),
        "01_Build" => gettext("Build"),
        "02b_Build and Clean" => gettext("Build and Clean"),
        "08_Build Force" => gettext("Force Build"),
        "09_Build NoCheck" => gettext("Build without Checks"),
        "10_Build NoGPG" => gettext("Build without GPG"),
        "11_Fetch Sources" => gettext("Fetch Sources"),
        "02_Install" => gettext("Install"),
        "12_Install Force" => gettext("Force Install"),
        "13_Install RmDeps" => gettext("Install and Remove Build Dependencies"),
        "14_Install NoCheck" => gettext("Install without Checks"),
        "15_Install NoGPG" => gettext("Install without GPG"),
        "03_Update Checksums" => gettext("Update Checksums"),
        "04_Update .SRCINFO" => gettext("Update .SRCINFO"),
        "16_Gen Checksums" => gettext("Generate Checksums"),
        "05_Namcap" => gettext("Namcap"),
        "05b_ShellCheck" => gettext("ShellCheck"),
        "06_Push AUR" => gettext("Push to AUR"),
        "17_Push AUR Tag" => gettext("Push AUR Tag"),
        "07_Clean srcdir" => gettext("Clean srcdir"),
        "07b_Clean Everything" => gettext("Clean Everything"),
        _ => id.to_string(),
    }
}

// ── Default menu ────────────────────────────────────────────────────────────────

pub fn default_menu() -> Vec<MenuGroup> {
    vec![
        MenuGroup {
            group: gettext("Actions"),
            items: vec![
                MenuItem {
                    id: "00_Full Workflow".into(),
                    label: action_label("00_Full Workflow"),
                    enabled: true,
                },
                MenuItem {
                    id: "02_Install".into(),
                    label: action_label("02_Install"),
                    enabled: true,
                },
                MenuItem {
                    id: "01_Build".into(),
                    label: action_label("01_Build"),
                    enabled: true,
                },
                MenuItem {
                    id: "02b_Build and Clean".into(),
                    label: action_label("02b_Build and Clean"),
                    enabled: true,
                },
            ],
        },
        MenuGroup {
            group: gettext("Metadata"),
            items: vec![
                MenuItem {
                    id: "03_Update Checksums".into(),
                    label: action_label("03_Update Checksums"),
                    enabled: true,
                },
                MenuItem {
                    id: "04_Update .SRCINFO".into(),
                    label: action_label("04_Update .SRCINFO"),
                    enabled: true,
                },
            ],
        },
        MenuGroup {
            group: gettext("Audit"),
            items: vec![
                MenuItem {
                    id: "05_Namcap".into(),
                    label: action_label("05_Namcap"),
                    enabled: true,
                },
                MenuItem {
                    id: "05b_ShellCheck".into(),
                    label: action_label("05b_ShellCheck"),
                    enabled: true,
                },
            ],
        },
        MenuGroup {
            group: gettext("Git / AUR"),
            items: vec![MenuItem {
                id: "06_Push AUR".into(),
                label: action_label("06_Push AUR"),
                enabled: true,
            }],
        },
        MenuGroup {
            group: gettext("Clean"),
            items: vec![
                MenuItem {
                    id: "07_Clean srcdir".into(),
                    label: action_label("07_Clean srcdir"),
                    enabled: true,
                },
                MenuItem {
                    id: "07b_Clean Everything".into(),
                    label: action_label("07b_Clean Everything"),
                    enabled: true,
                },
            ],
        },
    ]
}

// ── Load / save ────────────────────────────────────────────────────────────────

/// Result of loading menu.json.
/// Bug #10 fix: reports unknown item IDs that were stripped so the caller
/// (settings GUI) can surface a warning toast instead of silently dropping them.
pub struct LoadResult {
    pub groups: Vec<MenuGroup>,
    /// IDs that were present in menu.json but are not in all_actions().
    /// Non-empty only when the file was written by a future/different version.
    #[allow(dead_code)]
    pub unknown_ids: Vec<String>,
}

pub fn load() -> Vec<MenuGroup> {
    load_with_diagnostics().groups
}

pub fn load_with_diagnostics() -> LoadResult {
    let known: std::collections::HashSet<String> =
        all_actions().iter().map(|(id, _)| id.clone()).collect();

    let result = (|| -> Option<Vec<MenuGroup>> {
        let text = std::fs::read_to_string(config_file()).ok()?;
        serde_json::from_str(&text).ok()
    })();

    match result {
        None => LoadResult {
            groups: default_menu(),
            unknown_ids: vec![],
        },
        Some(mut groups) => {
            let mut unknown_ids: Vec<String> = Vec::new();
            for g in &mut groups {
                for item in &mut g.items {
                    if !known.contains(item.id.as_str()) {
                        unknown_ids.push(item.id.clone());
                    } else if item.label == item.id {
                        // Migrate labels saved by older releases without
                        // exposing the numeric script-order prefix.
                        item.label = action_label(&item.id);
                    }
                }
                g.items.retain(|i| known.contains(i.id.as_str()));
            }
            LoadResult {
                groups,
                unknown_ids,
            }
        }
    }
}

pub fn save(data: &[MenuGroup]) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(config_file(), json)
}
