use std::path::PathBuf;
use std::process::Command;

// Must match constants::HELPER_BINARY_NAME in src/constants.rs
const HELPER_BINARY_NAME: &str = "relay-helper";

fn main() {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    println!("cargo:rustc-env=RELAY_BUILD_PROFILE={profile}");

    // Only compile on macOS
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "macos" {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
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
}
