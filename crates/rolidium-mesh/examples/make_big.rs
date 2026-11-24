// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Big meshes for the speed tests: `make_big sphere <triangles> <file.stl>` and `make_big holes <nx> <ny> <seg> <file.stl>` (a plate with nx * ny holes).
use rolidium_mesh::{check, gen, stl};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (mesh, path) = match a.get(1).map(String::as_str) {
        Some("sphere") => (gen::sphere(50.0, a[2].parse().unwrap()), a[3].clone()),
        Some("holes") => (
            gen::plate_with_holes(
                a[2].parse().unwrap(),
                a[3].parse().unwrap(),
                10.0,
                4.0,
                3.0,
                a[4].parse().unwrap(),
            ),
            a[5].clone(),
        ),
        _ => {
            eprintln!("make_big sphere <triangles> <file> | make_big holes <nx> <ny> <seg> <file>");
            std::process::exit(2);
        }
    };
    let r = check::check(&mesh);
    eprintln!("{} triangles, {}", mesh.triangles.len(), r.lines().join("; "));
    std::fs::write(&path, stl::write_binary(&mesh)).unwrap();
}
