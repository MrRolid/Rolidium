// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
// Embeds a short git hash of the build (with a "+" when the working tree has uncommitted changes).
use std::process::Command;

fn main() {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let mut hash = git(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "nogit".into());
    if git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty()) {
        hash.push('+');
    }
    // "0.2.0" only for a clean build of exactly the tag v<version>; every other build is "0.2.0-dev" (development version)
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let exact_tag =
        git(&["describe", "--exact-match", "--tags", "HEAD"]).is_some_and(|t| t == format!("v{version}"));
    let label = if exact_tag && !hash.ends_with('+') {
        version
    } else {
        format!("{version}-dev")
    };
    println!("cargo:rustc-env=ROLIDIUM_VERSION_LABEL={label}");
    println!("cargo:rustc-env=ROLIDIUM_BUILD_HASH={hash}");
    println!("cargo:rerun-if-changed=../../.git/refs/tags");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
}
