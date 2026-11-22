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
    // "0.2.1" only for a clean build of exactly the tag v<version>; every other build is "0.2.1-dev" (development version)
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
    windows_icon();
}

/// Windows: the icon of rolidium.exe (a resource file compiled with windres and passed to the linker).
/// Without windres the program is still built, only without the icon in Explorer.
fn windows_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=../../assets/appicon/rolidium.rc");
    println!("cargo:rerun-if-changed=../../assets/appicon/rolidium.ico");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let out = out_dir.join("rolidium-icon.o");
    // the version and the author go into the properties of the exe
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut parts: Vec<String> = version
        .split('.')
        .map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
        .collect();
    parts.resize(4, "0".into());
    let rc_text = std::fs::read_to_string("../../assets/appicon/rolidium.rc")
        .unwrap_or_default()
        .replace("@VERSION_COMMA@", &parts.join(","))
        .replace("@VERSION@", &version);
    let rc = out_dir.join("rolidium.rc");
    let _ = std::fs::write(&rc, rc_text);
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
    let ok = Command::new(&windres)
        .current_dir("../../assets/appicon")
        .arg("-O")
        .arg("coff")
        .arg("-i")
        .arg(&rc)
        .arg("-o")
        .arg(&out)
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        println!("cargo:rustc-link-arg-bins={}", out.display());
    } else {
        println!("cargo:warning=windres failed or is missing: rolidium.exe is built without its icon");
    }
}
