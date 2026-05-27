use std::path::PathBuf;
use std::process::Command;

// Must match constants::HELPER_BINARY_NAME in src/constants.rs
const HELPER_BINARY_NAME: &str = "relay-helper";

// Must match constants::PREFS_APP_NAME in src/constants.rs
const PREFS_APP_NAME: &str = "RelayPreferences.app";
const PREFS_BINARY_NAME: &str = "RelayPreferences";

fn main() {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    println!("cargo:rustc-env=RELAY_BUILD_PROFILE={profile}");

    // Only compile on macOS
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "macos" {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");

    // -------------------------------------------------------------------------
    // Compile the Swift helper binary
    // -------------------------------------------------------------------------
    let dest = PathBuf::from(&out_dir).join(HELPER_BINARY_NAME);

    let status = Command::new("swiftc")
        .args(["-O", "helper/Sources/main.swift", "-o"])
        .arg(&dest)
        .status()
        .expect("failed to invoke swiftc — ensure Xcode CLI tools are installed");

    if !status.success() {
        panic!("swiftc failed to compile relay-helper");
    }

    // Copy the compiled helper to a stable path so cargo-packager can find it.
    let stable = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(&profile)
        .join(HELPER_BINARY_NAME);
    std::fs::copy(&dest, &stable)
        .unwrap_or_else(|e| panic!("failed to copy helper to stable target path: {e}"));

    println!("cargo:rerun-if-changed=helper/Sources/main.swift");
    println!("cargo:rustc-env=RELAY_HELPER_PATH={}", dest.display());

    // -------------------------------------------------------------------------
    // Compile the SwiftUI preferences app
    // -------------------------------------------------------------------------

    // Resolve SDK path via xcrun.
    let sdk_output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .expect("xcrun not found — Xcode CLI tools required");
    let sdk_path = String::from_utf8(sdk_output.stdout)
        .expect("xcrun output is not valid utf8")
        .trim()
        .to_string();

    // Collect all Swift source files under prefs/Sources/.
    let prefs_sources: Vec<PathBuf> = std::fs::read_dir("prefs/Sources")
        .expect("prefs/Sources directory not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "swift"))
        .map(|e| e.path())
        .collect();

    let prefs_binary_dest = PathBuf::from(&out_dir).join(PREFS_BINARY_NAME);

    let prefs_status = Command::new("swiftc")
        .arg("-sdk")
        .arg(&sdk_path)
        .arg("-O")
        .arg("-parse-as-library")
        .args(&prefs_sources)
        .arg("-o")
        .arg(&prefs_binary_dest)
        .status()
        .expect("failed to invoke swiftc for RelayPreferences");

    if !prefs_status.success() {
        panic!("swiftc failed to compile RelayPreferences");
    }

    // Assemble the .app bundle in OUT_DIR.
    let app_bundle = PathBuf::from(&out_dir).join(PREFS_APP_NAME);
    let macos_dir = app_bundle.join("Contents").join("MacOS");
    let resources_dir = app_bundle.join("Contents").join("Resources");
    std::fs::create_dir_all(&macos_dir)
        .unwrap_or_else(|e| panic!("failed to create prefs .app MacOS dir: {e}"));
    std::fs::create_dir_all(&resources_dir)
        .unwrap_or_else(|e| panic!("failed to create prefs .app Resources dir: {e}"));
    std::fs::copy(&prefs_binary_dest, macos_dir.join(PREFS_BINARY_NAME))
        .unwrap_or_else(|e| panic!("failed to copy prefs binary into .app: {e}"));
    std::fs::copy(
        "prefs/Info.plist",
        app_bundle.join("Contents").join("Info.plist"),
    )
    .unwrap_or_else(|e| panic!("failed to copy prefs Info.plist: {e}"));

    // Copy .app bundle to a stable path for cargo-packager and the path resolver.
    let stable_app = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(&profile)
        .join(PREFS_APP_NAME);
    let _ = std::fs::remove_dir_all(&stable_app);
    copy_dir_all(&app_bundle, &stable_app)
        .unwrap_or_else(|e| panic!("failed to copy prefs .app to stable target path: {e}"));

    println!("cargo:rerun-if-changed=prefs/Sources");
    println!("cargo:rerun-if-changed=prefs/Info.plist");
    println!("cargo:rustc-env=RELAY_PREFS_PATH={}", stable_app.display());
}

/// Recursively copy a directory tree from `src` to `dst`.
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
