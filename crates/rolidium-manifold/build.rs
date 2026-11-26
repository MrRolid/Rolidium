// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Compiles the Manifold library (third_party/manifold, Apache-2.0, without its cross-section part and without TBB) and a small C interface into the crate.

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../third_party/manifold");
    let src = root.join("src");
    let files = [
        "boolean3.cpp",
        "boolean_result.cpp",
        "constructors.cpp",
        "csg_tree.cpp",
        "edge_op.cpp",
        "execution_impl.cpp",
        "face_op.cpp",
        "impl.cpp",
        "manifold.cpp",
        "polygon.cpp",
        "properties.cpp",
        "quickhull.cpp",
        "sort.cpp",
        "subdivision.cpp",
        "smoothing.cpp",
        "tree2d.cpp",
    ];
    let mut b = cc::Build::new();
    b.cpp(true).std("c++17").warnings(false).opt_level(2);
    b.include(root.join("include")).include(&src);
    b.define("MANIFOLD_PAR", "-1").define("MANIFOLD_EXPORT", "");
    for f in files {
        b.file(src.join(f));
        println!("cargo:rerun-if-changed={}", src.join(f).display());
    }
    b.file("src/shim.cpp");
    println!("cargo:rerun-if-changed=src/shim.cpp");
    b.compile("rolidium_manifold");
}
