//! Persists window size to ~/.config/pkgbuild-manager/window-state.json
//! Shared format with the aur_push_gui crate.

use std::path::PathBuf;

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

pub fn load(key: &str, default_w: i32, default_h: i32) -> (i32, i32) {
    (|| -> Option<(i32, i32)> {
        let text = std::fs::read_to_string(state_path()).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        let obj = val.get(key)?;
        let w = obj.get("width")?.as_i64()? as i32;
        let h = obj.get("height")?.as_i64()? as i32;
        Some((w, h))
    })()
    .unwrap_or((default_w, default_h))
}

pub fn save(key: &str, width: i32, height: i32) {
    let path = state_path();
    let mut obj: serde_json::Map<String, serde_json::Value> = (|| -> Option<_> {
        let text = std::fs::read_to_string(&path).ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        val.as_object().cloned()
    })()
    .unwrap_or_default();

    obj.insert(
        key.to_string(),
        serde_json::json!({ "width": width, "height": height }),
    );

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&obj).unwrap_or_default());
}
