//! Shared window-state persistence for the aur_push_gui binaries.
//!
//! Stores window sizes to ~/.config/pkgbuild-manager/window-state.json.
//! All three GUI binaries (aur-push, release-gui, and any future one) read
//! and write the **same** JSON file so there is a single source of truth.
//!
//! Bug #1 fix: previously aur_dialog.rs and release_dialog.rs each contained
//! private copies of state_path/load_win_size/save_win_size.  Any divergence
//! between the copies would silently produce an inconsistent state file.
//! This module is the single canonical implementation.

use std::path::PathBuf;

#[allow(dead_code)]
pub fn state_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut h = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
            h.push(".config");
            h
        });
    base.join("pkgbuild-manager").join("window-state.json")
}

#[allow(dead_code)]
pub fn load(key: &str, default_w: i32, default_h: i32) -> (i32, i32) {
    (|| -> Option<(i32, i32)> {
        let text = std::fs::read_to_string(state_path()).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        let obj = val.get(key)?;
        Some((obj.get("width")?.as_i64()? as i32, obj.get("height")?.as_i64()? as i32))
    })()
    .unwrap_or((default_w, default_h))
}

#[allow(dead_code)]
pub fn save(key: &str, width: i32, height: i32) {
    let path = state_path();
    let mut obj: serde_json::Map<String, serde_json::Value> = (|| -> Option<_> {
        let text = std::fs::read_to_string(&path).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        val.as_object().cloned()
    })()
    .unwrap_or_default();
    obj.insert(key.to_string(), serde_json::json!({"width": width, "height": height}));
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&obj).unwrap_or_default());
}
