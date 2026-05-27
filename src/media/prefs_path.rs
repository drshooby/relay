use std::path::PathBuf;

const PROFILE: &str = env!("RELAY_BUILD_PROFILE");

/// Resolve path to the bundled RelayPreferences.app.
///
/// Resolution order (first match wins):
/// 1. `RELAY_PREFS_PATH` compile-time env (dev — set by build.rs)
/// 2. Relative to current executable: `../Resources/RelayPreferences.app` (.app bundle)
/// 3. Fallback: `target/{profile}/RelayPreferences.app` next to cargo workspace
pub fn resolve_prefs_path() -> PathBuf {
    // 1. Compile-time path from build.rs
    if let Some(p) = option_env!("RELAY_PREFS_PATH") {
        let path = PathBuf::from(p);
        if path.exists() {
            return path;
        }
    }

    // 2. .app bundle relative path
    if let Ok(exe) = std::env::current_exe() {
        let bundle_path = exe
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("Resources").join(crate::constants::PREFS_APP_NAME));
        if let Some(p) = bundle_path {
            if p.exists() {
                return p;
            }
        }
    }

    // 3. Fallback: next to cargo workspace target dir
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(PROFILE)
        .join(crate::constants::PREFS_APP_NAME)
}
