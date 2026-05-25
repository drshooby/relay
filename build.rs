use std::path::PathBuf;
use std::process::Command;

const HELPER_BINARY_NAME: &str = "relay-helper";

fn main() {
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

    println!("cargo:rerun-if-changed=helper/Sources/main.swift");
    println!("cargo:rustc-env=RELAY_HELPER_PATH={}", dest.display());
}
