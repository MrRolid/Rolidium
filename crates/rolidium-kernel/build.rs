// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Build script: obtains a static Open CASCADE build and compiles the C++ bridge.
//!
//! OCCT lookup order:
//! 1. `ROLIDIUM_OCCT_ROOT` (a directory with `include/opencascade` and `lib`),
//! 2. the prebuilt archive in `$ROLIDIUM_VENDOR_DIR` (default `~/rolidium-vendor/`), SHA-256 verified,
//! 3. only if that is missing: download of the same archive (needs `curl`), SHA-256 verified.

use sha2::{Digest, Sha256};
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// A prebuilt static Open CASCADE for one target: archive name, checksum and where headers and libraries are inside it.
struct Occt {
    name: &'static str,
    sha256: &'static str,
    /// Folder with the headers, relative to the unpacked archive.
    include: &'static str,
    /// Folder with the static libraries.
    lib: &'static str,
    /// System libraries the static Open CASCADE needs on top of its own.
    system_libs: &'static [&'static str],
}

const OCCT_LINUX: Occt = Occt {
    name: "occt-8_0_1_rev3-x86_64_unknown_linux_gnu",
    sha256: "f76c341123fc008391d6230b5e9157c5531166ed4cf301f6484f69a44aee59c8",
    include: "include/opencascade",
    lib: "lib",
    system_libs: &[],
};

const OCCT_WINDOWS_GNU: Occt = Occt {
    name: "occt-8_0_1_rev3-x86_64_pc_windows_gnu",
    sha256: "dc2cd5a2b4df2f58e6590f027fd04eef0b65d21a5003933d01e74e275c18301c",
    include: "inc",
    lib: "win64/gcc/lib",
    system_libs: &[
        "ws2_32", "user32", "advapi32", "gdi32", "shell32", "ole32", "psapi",
    ],
};

const OCCT_URL_BASE: &str = "https://github.com/lzpel/cadrum/releases/download/occt-8_0_1_rev3";

/// The archive that matches the target being built (not the machine the build runs on).
fn occt_for_target() -> &'static Occt {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env_abi = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    match (os.as_str(), env_abi.as_str(), arch.as_str()) {
        ("windows", "gnu", "x86_64") => &OCCT_WINDOWS_GNU,
        ("linux", _, "x86_64") => &OCCT_LINUX,
        _ => panic!("no prebuilt Open CASCADE for target {arch}-{os}-{env_abi}; set ROLIDIUM_OCCT_ROOT"),
    }
}

const OCCT_LIBS: &[&str] = &[
    "TKDESTEP",
    "TKDECascade",
    "TKDE",
    "TKXSBase",
    "TKFillet",
    "TKOffset",
    "TKShHealing",
    "TKBool",
    "TKBO",
    "TKMesh",
    "TKTopAlgo",
    "TKPrim",
    "TKGeomAlgo",
    "TKGeomBase",
    "TKBRep",
    "TKG3d",
    "TKG2d",
    "TKMath",
    "TKernel",
];

fn main() {
    println!("cargo:rerun-if-env-changed=ROLIDIUM_OCCT_ROOT");
    println!("cargo:rerun-if-env-changed=ROLIDIUM_VENDOR_DIR");
    println!("cargo:rerun-if-changed=src/bridge.cpp");
    println!("cargo:rerun-if-changed=src/bridge.h");
    println!("cargo:rerun-if-changed=src/occt.rs");

    let occt = occt_for_target();
    let (root, include, lib) = match env::var("ROLIDIUM_OCCT_ROOT") {
        // a local build: headers in include/opencascade, libraries in lib
        Ok(r) => {
            let r = PathBuf::from(r);
            (r.clone(), r.join("include").join("opencascade"), r.join("lib"))
        }
        Err(_) => {
            let r = fetch_prebuilt(occt);
            (r.clone(), r.join(occt.include), r.join(occt.lib))
        }
    };
    let _ = root;
    assert!(
        include.is_dir(),
        "OCCT headers not found in {}",
        include.display()
    );

    println!("cargo:rustc-link-search=native={}", lib.display());
    for l in OCCT_LIBS {
        println!("cargo:rustc-link-lib=static={l}");
    }
    for l in occt.system_libs {
        println!("cargo:rustc-link-lib={l}");
    }

    let windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let mut build = cxx_build::bridge("src/occt.rs");
    if windows {
        // link the C++ runtime statically, so rolidium.exe does not need libstdc++-6.dll next to it
        build.cpp_link_stdlib(None);
        // rustc does not know where the compiler keeps libstdc++.a: ask the compiler
        let cxx = build.get_compiler();
        for lib in ["libstdc++.a", "libwinpthread.a"] {
            let out = Command::new(cxx.path())
                .arg(format!("-print-file-name={lib}"))
                .output();
            if let Some(dir) = out
                .ok()
                .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
                .filter(|p| p.is_absolute())
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            {
                println!("cargo:rustc-link-search=native={}", dir.display());
            }
        }
        println!("cargo:rustc-link-lib=static=stdc++");
        println!("cargo:rustc-link-lib=static=winpthread");
    }
    build
        .file("src/bridge.cpp")
        .include(&include)
        .include("src")
        .std("c++17")
        .warnings(false)
        .define("_USE_MATH_DEFINES", None)
        .compile("rolidium_kernel_bridge");
}

fn target_dir() -> PathBuf {
    // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    out.ancestors()
        .nth(4)
        .expect("cannot locate target dir")
        .to_path_buf()
}

fn vendor_dir() -> Option<PathBuf> {
    if let Some(d) = env::var_os("ROLIDIUM_VENDOR_DIR") {
        return Some(PathBuf::from(d));
    }
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join("rolidium-vendor"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn fetch_prebuilt(occt: &Occt) -> PathBuf {
    let cache = target_dir().join("rolidium-occt");
    let root = cache.join(occt.name);
    if root.join(occt.include).is_dir() {
        return root;
    }
    std::fs::create_dir_all(&cache).unwrap();
    let file_name = format!("{}.tar.gz", occt.name);

    // 1. Local vendor copy (outside git), 2. download as a fallback. The hash is checked in both cases.
    let local = vendor_dir().map(|d| d.join(&file_name)).filter(|p| p.is_file());
    let bytes = match local {
        Some(path) => {
            println!(
                "cargo:warning=Using vendored Open CASCADE archive {}",
                path.display()
            );
            std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        }
        None => {
            println!("cargo:warning=No vendored archive found, downloading Open CASCADE (one time, ~44 MB)");
            let tgz = cache.join(&file_name);
            let status = Command::new("curl")
                .args(["-fsSL", "--retry", "3", "-o"])
                .arg(&tgz)
                .arg(format!("{OCCT_URL_BASE}/{file_name}"))
                .status()
                .expect("`curl` is required to download Open CASCADE; put the archive into ~/rolidium-vendor/ instead");
            assert!(status.success(), "downloading Open CASCADE failed");
            let b = std::fs::read(&tgz).unwrap();
            let _ = std::fs::remove_file(&tgz);
            b
        }
    };
    assert_eq!(
        sha256_hex(&bytes),
        occt.sha256,
        "Open CASCADE archive checksum mismatch"
    );
    let gz = flate2::read::GzDecoder::new(&bytes[..]);
    tar::Archive::new(gz)
        .unpack(&cache)
        .expect("unpacking Open CASCADE failed");
    assert!(root.join(occt.include).is_dir(), "unexpected archive layout");
    root
}
