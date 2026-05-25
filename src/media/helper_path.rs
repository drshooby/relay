use std::path::PathBuf;

const PROFILE: &str = env!("RELAY_BUILD_PROFILE");

/// Resolve path to the bundled Swift helper binary.
///
/// Resolution order (first match wins):
/// 1. `RELAY_HELPER_PATH` compile-time env (dev — set by build.rs OUT_DIR)
/// 2. Relative to current executable: `../Resources/relay-helper` (future .app bundle)
/// 3. Fallback: `target/{profile}/relay-helper` next to cargo workspace (local dev convenience)
///
/// v1: only (1) and (3) are exercised. (2) is documented for future packaging — do not delete.
pub fn resolve_helper_path() -> PathBuf {
    // 1. Compile-time path from build.rs
    let compile_time = option_env!("RELAY_HELPER_PATH");
    if let Some(p) = compile_time {
        let path = PathBuf::from(p);
        if path.exists() {
            return path;
        }
    }

    // 2. .app bundle relative path (not used in v1, documented for future bundling)
    if let Ok(exe) = std::env::current_exe() {
        let bundle_path = exe
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("Resources").join("relay-helper"));
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
        .join(crate::constants::HELPER_BINARY_NAME)
}
